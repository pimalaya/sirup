mod cli;
mod config;
mod repl;
mod session;

use clap::Parser;
use pimalaya_toolbox::terminal::{error::ErrorReport, log::Logger, printer::StdoutPrinter};

use crate::cli::SirupCli;

fn main() {
    let cli = SirupCli::parse();

    Logger::init(&cli.log);

    let mut printer = StdoutPrinter::new(&cli.json);
    let config_paths = cli.config_paths.as_ref();
    let result = cli.command.exec(&mut printer, config_paths);

    ErrorReport::eval(&mut printer, result)
}
