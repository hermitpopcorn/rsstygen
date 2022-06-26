use anyhow::{Result, anyhow};
use tokio::process::Command;
use fantoccini::{Client, ClientBuilder, Locator};
use colored::Colorize;
use std::time::Duration;
use rusqlite::params;
use chrono::{DateTime, FixedOffset};

use crate::get_db;

#[derive(Debug)]
pub struct Chapter {
	pub title: String,
	pub url: String,
	pub date: Option<DateTime<FixedOffset>>,
}

#[derive(Debug)]
pub struct Instruction {
	pub title: String,
	pub url: String,
	pub list_node: String,
	pub js_script: String,
}

pub async fn generator(instructions: Vec<Instruction>) -> Result<()> {
	start_crawlers(instructions).await?;
	
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
									"INSERT INTO Chapters (manga, title, url, date, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?)",
									params![
										&instruction.title,
										&chapter.title,
										&chapter.url,
										&chapter.date,
										chrono::offset::Local::now(),
										chrono::offset::Local::now(),
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
	driver.kill().await.unwrap();

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

		let ch = Chapter {
			title: String::from(title),
			url: String::from(url),
			date: if date.len() > 0 { Some(date.parse().unwrap()) } else { None },
		};
		println!("{:?}", ch);
		chapters.push(ch);
	}

	Ok(chapters)
}