mod analyser;
#[cfg(feature = "benchmark")]
mod benchmark;
mod cli;
mod commands;
mod config;
mod edits;
mod output;
mod packages;
mod parallel;
mod php_pattern;
mod progress;
mod rename;
mod symbols;

fn main() -> std::process::ExitCode {
    cli::run()
}
