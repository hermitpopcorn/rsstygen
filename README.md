# rsstygen

## What is this

Rust version of [rssgen](https://github.com/hermitpopcorn/rssgen).

Basically, a Rust CLI app to generate RSS files from crawling sites. And also upload them somewhere via FTP.

I use it to generate manga updates feed (mainly Bokuyaba. Yangaru has completed. Thank you Uoyama-sensei) so I can get notified of new chapters from manga sites that don't have RSS. These free chapters disappear after a while you know? Gotta stay updated and read them while they're there.

## Now what

This thing's page-crawling process involves puppeteering a local Chrome installation using [chromedriver](https://chromedriver.chromium.org/) so install it and make sure running "chromedriver" on your terminal will start one up. Use `cargo build --release` to build the thing, and then copy `rsstygen.sample.toml` to the same folder as the executable and edit the settings in it (and also rename it to just `rsstygen.toml`).