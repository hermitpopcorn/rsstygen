use anyhow::{Result};
use colored::Colorize;
use ftp::FtpStream;
use std::{env, fs, fs::File};
use std::io::BufReader;

pub struct FtpSettings {
	pub host: String,
	pub port: u16,
	pub username: String,
	pub password: String,
	pub target_path: String,
}

pub fn upload(settings: FtpSettings) -> Result<()> {
	// Connect to FTP
	let mut ftp_stream = FtpStream::connect(settings.host.to_owned() + ":" + settings.port.to_string().as_str())?;
	ftp_stream.login(&settings.username, &settings.password)?;
	ftp_stream.cwd(&settings.target_path)?;

	// Get RSS files
	let mut rss_files_path = env::current_exe()?;
	rss_files_path.pop();
	rss_files_path.push("rsstygen-rssfiles");

	if rss_files_path.exists() {
		for entry in fs::read_dir(rss_files_path)? {
			let entry = entry?;
			let path = entry.path();
			if path.is_file() {
				let filename = &path.file_name().unwrap().to_str().unwrap();
				let file = File::open(&path)?;
				println!("{} Uploading {}...", "[UPLD]".yellow(), &filename.cyan());
				match ftp_stream.put(filename, &mut BufReader::new(file)) {
					Ok(_) => {},
					Err(e) => println!("{} {} {}. ({})", "[UPLD]".yellow(), "Error uploading".red(), &filename.cyan(), e),
				}
			}
		}
	}

	Ok(())
}