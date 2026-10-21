use clap::Parser;
use ttrackii_cli::{cli::Cli, run};

#[tokio::main]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
