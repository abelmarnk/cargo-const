use crate::{compat::Compat, utils::print_error};
use clap::Parser;
use std::sync::OnceLock;

pub mod bound;
pub mod compat;
pub mod error;
pub mod provider;
pub mod utils;

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn get_config() -> &'static Config {
    CONFIG.get().as_ref().unwrap()
}

fn set_config(args: &Args) {
    CONFIG.set(Config::from_args(args)).unwrap();
}

#[derive(Debug)]
pub struct Config {
    verbose: bool,
}

impl Config {
    fn from_args(args: &Args) -> Config {
        Config {
            verbose: args.verbose,
        }
    }
}

#[derive(Parser)]
struct Args {
    /// Log level
    #[clap(short, long, global = true)]
    verbose: bool,

    #[clap(subcommand)]
    subcommand: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    Compat(Compat),
}

fn main() {
    let args = Args::parse();

    set_config(&args);

    let result = match args.subcommand {
        SubCommand::Compat(compat) => compat.run(),
    };

    if let Err(error) = result {
        print_error(&error);
    }
}
