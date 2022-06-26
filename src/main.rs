use colored::Colorize;
use std::{env, fs};
use anyhow::{Result, anyhow};
use rusqlite::Connection;
use std::time::Duration;

mod generator;
use crate::generator::{generator, Instruction};

#[tokio::main]
async fn main() {
	// Read config file
	println!("{} Reading config file...", "[MAIN]".blue());
	let mut manga = vec![];
	{
		let config = match get_config() {
			Ok(c) => c,
			Err(e) => {
				println!("{} {} ({})", "[MAIN]".blue(), "Failed getting configuration data.".red(), e);
				std::process::exit(1);
			},
		};

		for (title, instructions) in config.as_table().unwrap() {
			println!("{} Found config for: {}", "[MAIN]".blue(), &title.cyan());
			let instructions = instructions.as_table().unwrap();
			let instruction = Instruction {
				title: String::from(title),
				url: String::from(instructions.get("url").unwrap().as_str().unwrap_or("")),
				list_node: String::from(instructions.get("list_node").unwrap().as_str().unwrap_or("")),
				js_script: String::from(instructions.get("js_script").unwrap().as_str().unwrap_or("")),
			};
			manga.push(instruction);
		}
	}

	// Prep database
	println!("{} Preparing database...", "[MAIN]".blue());
	match get_db() {
		Ok(c) => {
			match prep_db_table(&c) {
				Ok(()) => {},
				Err(e) => {
					println!("{} {}. ({})", "[MAIN]".blue(), "Failed preparing database table connection to database".red(), e);
					std::process::exit(1);
				},
			}
		},
		Err(e) => {
			println!("{} {}. ({})", "[MAIN]".blue(), "Failed establishing connection to database".red(), e);
			std::process::exit(1);
		},
	};

	// Generate
	println!("{} Starting generator...", "[MAIN]".blue());
	match generator(manga).await {
		Ok(()) => println!("{} {} finished.", "[MAIN]".blue(), "Generator".green()),
		Err(e) => println!("{} {} {}. ({})", "[MAIN]".blue(), "Generator".green(), "crashed".red(), e),
	}

	// Saraba
	println!("{} All done. Goodbye!", "[MAIN]".blue());
	std::thread::sleep(Duration::from_secs(2));
}

fn get_config() -> Result<toml::value::Value> {
	let mut config_file_path = env::current_exe()?;
	config_file_path.pop();
	config_file_path.push("rsstygen.toml");

	let config: String = String::from_utf8_lossy(
		&fs::read(config_file_path)?
	).into_owned();
	let config: toml::Value = config.parse()?;

	Ok(config)
}

fn get_db() -> Result<Connection> {
	let mut db_path = env::current_exe()?;
	db_path.pop();
	db_path.push("rsstygen.sqlite");

	if !db_path.exists() {
		fs::write(&db_path, "")?;
	}

	match Connection::open(db_path.to_str().unwrap()) {
		Ok(c) => Ok(c),
		Err(e) => Err(anyhow!(e)),
	}
}

fn prep_db_table(connection: &Connection) -> Result<()> {
	match connection.execute(r#"CREATE TABLE IF NOT EXISTS "Chapters" (
		"id"			INTEGER,
		"manga"			VARCHAR(255) NOT NULL,
		"title"			VARCHAR(255) NOT NULL,
		"url"			VARCHAR(255) NOT NULL,
		"date"			DATETIME,
		"created_at"	DATETIME NOT NULL,
		"updated_at"	DATETIME NOT NULL,
		PRIMARY KEY("id" AUTOINCREMENT)
	)"#, []) {
		Ok(_) => Ok(()),
		Err(e) => Err(anyhow!(e)),
	}
}