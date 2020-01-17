#[macro_use]
extern crate log;

use chrono::Local;
use clap::{App, Arg};
use log::{Level, LevelFilter, Metadata, Record};
use std::process;

pub struct SimpleLogger;

impl log::Log for SimpleLogger {
    #[inline]
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    #[inline]
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "{}: {} - {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            );
        }
    }

    #[inline]
    fn flush(&self) {}
}

fn main() {
    log::set_logger(&SimpleLogger).unwrap();
    log::set_max_level(LevelFilter::Info);

    let app = App::new("gather")
        .about("Gathers memory tracking data from a given machine")
        .arg(Arg::with_name("TARGET").required(false));

    let matches = app.get_matches();

    let target = matches.value_of("TARGET");
    let result = cli_core::cmd_gather::main(target);

    if let Err(error) = result {
        error!("{}", error);
        process::exit(1);
    }
}
