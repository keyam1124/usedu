use anyhow::Result;
use clap::Parser;
use usedu::cli::Cli;

fn main() -> Result<()> {
    usedu::cli::run(Cli::parse())
}
