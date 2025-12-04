use owo_colors::OwoColorize;
use std::{str::FromStr, time::Duration};

use crate::{error::ConstError, get_config};

pub const MAX_CACHE_AGE: u64 = 60 * 60 * 24 * 7; // 1 week
pub const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MY_USER_AGENT: &str = "my-user-agent (the25thjohndoe@gmail.com)";

pub fn print_header_and_items<I, T>(header: &str, items: I)
where
    I: IntoIterator<Item = T>,
    T: std::fmt::Display,
{
    println!("{}:", header.bold().cyan());
    println!();
    for item in items {
        println!("{}", item.bold().blue());
    }
}

pub fn print_error(error: &ConstError) {
    print!("{}: {}", "Error".bold().red(), error.bright_red());
}

pub fn print_warning(message: &str) {
    println!("{}: {}", "Warning".bold().yellow(), message.bright_yellow());
}

pub fn print_info(message: &str) {
    if get_config().verbose {
        println!("{}: {}", "Info".bold().cyan(), message.bright_cyan());
    }
}

pub fn get_rust_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut places = version.split('.').map(|place| u64::from_str(place).ok());
    Some((
        places.next()??,
        places.next().unwrap_or(Some(0))?,
        places.next().unwrap_or(Some(0))?,
    ))
}

pub fn now_as_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}
