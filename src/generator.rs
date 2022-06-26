use std::{env, fs};
use anyhow::{Result, anyhow};
use tokio::process::Command;
use fantoccini::{Client, ClientBuilder, Locator};
use colored::Colorize;
use std::time::Duration;
use rusqlite::params;
use chrono::{DateTime, FixedOffset, Local};
use rss::{ItemBuilder, ChannelBuilder, Guid};
use std::collections::HashMap;

use crate::get_db;

#[derive(Debug)]
pub struct Chapter {
	pub id: Option<usize>,
	pub manga: String,
	pub title: String,
	pub url: String,
	pub date: Option<DateTime<FixedOffset>>,
	pub created_at: Option<DateTime<Local>>,
	pub updated_at: Option<DateTime<Local>>,
}

#[derive(Debug)]
pub struct Instruction {
	pub title: String,
	pub url: String,
	pub list_node: String,
	pub js_script: String,
}

pub async fn generate(instructions: Vec<Instruction>) -> Result<()> {
	// Create map of titles before giving it to crawlers
	let titles: HashMap<String, String> = instructions.iter().map(|i| (i.title.to_owned(), i.url.to_owned())).collect();

	start_crawlers(instructions).await?;

	write_rss_from_db(titles).await?;
	
	Ok(())
}

async fn start_crawlers(instructions: Vec<Instruction>) -> Result<()> {
	// Start chromedriver
	println!("{} {}", "[GENR]".green(), "Starting chromedriver...");
	let mut driver = Command::new("chromedriver")
        .arg("--port=4444")
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
        .spawn()?;
	std::thread::sleep(Duration::from_secs(2));

	// Crawl in subprocesses
	let mut handles = vec![];
	let mut counter = 0;
	for instruction in instructions {
		handles.push(tokio::spawn(async move {
			// Prevent crawlers from starting on the same time for arbitrary purposes
			std::thread::sleep(Duration::from_secs(counter * 2));

			let mut try_count = 0;
			while try_count < 3 {
				if try_count > 0 {
					println!("{} Waiting 3 seconds before retrying for {}...", "[GENR]".green(), instruction.title.cyan());
					std::thread::sleep(Duration::from_secs(3));
				}

				try_count += 1;
				println!("{} Crawl attempt for {} number {}.", "[GENR]".green(), instruction.title.cyan(), try_count);

				// Create new connection
				let c = match get_db() {
					Ok(c) => c,
					Err(e) => {
						println!("{} {} {}. ({})", "[GENR]".green(), instruction.title.cyan(), "thread failed establishing a connection to database".red(), e);
						continue;
					},
				};

				println!("{} Crawling {}...", "[GENR]".green(), instruction.title.cyan());
				let chapters = crawl(&instruction).await;

				match chapters {
					Err(e) => {
						println!("{} Crawling {} {}. ({})", "[GENR]".green(), instruction.title.cyan(), "yielded no results".red(), e);
						continue;
					},
					Ok(chapters) => {
						if chapters.len() < 1 {
							println!("{} Crawling {} finished but {}.", "[GENR]".green(), instruction.title.cyan(), "yielded no results".red());
							continue;
						}

						println!("{} Crawling {} finished.", "[GENR]".green(), instruction.title.cyan());
						for chapter in chapters {
							let row_exists: bool = c.query_row("SELECT id FROM Chapters WHERE manga = ? AND title = ?", [&instruction.title, &chapter.title], |_| Ok(true)).unwrap_or(false);
							if !row_exists {
								println!("{} Inserting {} {} into database...", "[GENR]".green(), &instruction.title.cyan(), &chapter.title.magenta());
								c.execute(
									"INSERT INTO Chapters (manga, title, url, date, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
									params![
										&chapter.manga,
										&chapter.title,
										&chapter.url,
										&chapter.date,
										&chapter.created_at,
										&chapter.updated_at,
									],
								).expect("failed inserting data");
							}
						}
						break;
					},
				}
			}
		}));

		counter += 1;
	}

	// Wait for all of them to finish before continuing
	futures::future::join_all(handles).await;

	// Finish crawling by killing the chromedriver
	println!("{} {}", "[GENR]".green(), "Killing the chromedriver...");
	std::thread::sleep(Duration::from_secs(1));
	driver.kill().await?;

	Ok(())
}

async fn crawl(instruction: &Instruction) -> Result<Vec<Chapter>> {
	let client = ClientBuilder::native().connect("http://localhost:4444").await?;

	let chapters = get_chapters(&client, instruction).await;

	println!("{} Closing {} crawler...", "[GENR]".green(), &instruction.title.cyan());
	match client.close().await {
		Ok(_) => {},
		Err(e) => { println!("{} {} {} {}. ({})", "[GENR]".green(), "Error closing".red(), &instruction.title.cyan(), "crawler".red(), e); },
	}

	chapters
}

async fn get_chapters(client: &Client, instruction: &Instruction) -> Result<Vec<Chapter>> {
	let mut chapters = vec![];

	client.goto(&instruction.url).await?;
	client.wait().at_most(Duration::from_secs(30)).for_element(Locator::Css(&instruction.list_node)).await?;
	let chapter_list = client.execute(&instruction.js_script, vec![]).await?;
	let chapter_list = chapter_list.as_array().ok_or(anyhow!("failed converting chapter list to array"))?;

	for chapter in chapter_list {
		let title = chapter.get("title").ok_or(anyhow!("failed getting title from chapter"))?;
		let title = title.as_str().ok_or(anyhow!("failed converting chapter title to string"))?;

		let url = chapter.get("url").ok_or(anyhow!("failed getting url from chapter"))?;
		let url = url.as_str().ok_or(anyhow!("failed converting chapter url to string"))?;
		
		let date = match chapter.get("date") {
			Some(date) => date.as_str().ok_or(anyhow!("failed converting chapter date to string"))?,
			None => "",
		};

		let now = chrono::offset::Local::now();

		let ch = Chapter {
			id: None,
			manga: String::from(&instruction.title),
			title: String::from(title),
			url: String::from(url),
			date: if date.len() > 0 { Some(date.parse()?) } else { None },
			created_at: Some(now),
			updated_at: Some(now),
		};
		chapters.push(ch);
	}

	Ok(chapters)
}

async fn write_rss_from_db(titles: HashMap<String, String>) -> Result<()> {
	println!("{} {}", "[GENR]".green(), "Loading chapters from database...");

	let c = match get_db() {
		Ok(c) => c,
		Err(e) => {
			println!("{} {}. ({})", "[GENR]".green(), "Failed establishing connection to db for RSS generation".red(), e);
			return Err(e);
		},
	};

	// Loop for every title in config
	for (title, url) in titles {
		// Get chapters from database
		let mut s = c.prepare("SELECT id, manga, title, url, date, created_at, updated_at FROM Chapters WHERE manga = ? ORDER BY CASE WHEN date <> NULL THEN date ELSE created_at END DESC, title DESC")?;
		let chapters = s.query([&title])?.mapped(|r: &rusqlite::Row| -> rusqlite::Result<Chapter> {
			Ok(Chapter {
				id: r.get(0)?,
				manga: r.get(1)?,
				title: r.get(2)?,
				url: r.get(3)?,
				date: r.get(4)?,
				created_at: r.get(5)?,
				updated_at: r.get(6)?,
			})
		});

		// Setup RSS channel
		let mut channel = ChannelBuilder::default()
			.title(&title)
			.link(&url)
			.description(String::from(&title) + " chapters automatically generated by crawling.")
			.build();

		// Insert chapters into RSS items vector
		let mut items = vec![];
		for chapter in chapters {
			let chapter = chapter?;

			let item = ItemBuilder::default()
				.title(chapter.title)
				.link(chapter.url)
				.pub_date(if chapter.date.is_some() {
					chapter.date.ok_or(anyhow!("failed unpacking chapter date"))?.to_rfc2822()
				} else {
					chapter.created_at.ok_or(anyhow!("failed unpacking chapter created_at"))?.to_rfc2822()
				})
				.guid(Guid {
					value: chapter.id.ok_or(anyhow!("failed converting chapter ID to String"))?.to_string(),
					permalink: false,
				})
				.build();
			items.push(item);
		}

		// Push the items into the channel and get the channel's whole XML string
		channel.set_items(items);
		channel.write_to(::std::io::sink())?;

		// Write to file
		let mut rss_file_path = env::current_exe()?;
		rss_file_path.pop();
		rss_file_path.push("rsstygen-rssfiles");
		if !rss_file_path.exists() {
			fs::create_dir_all(&rss_file_path)?;
		}
		rss_file_path.push(String::from(&title) + ".rss");
		fs::write(&rss_file_path, channel.to_string())?;
	}

	Ok(())
}