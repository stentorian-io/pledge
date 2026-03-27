mod acquire;
mod cli;
mod compare;
mod discover;
mod report;
mod types;

use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match run(cli).await {
        Ok(true) => std::process::exit(1),
        Ok(false) => std::process::exit(0),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}

async fn run(_cli: Cli) -> anyhow::Result<bool> {
    Ok(false)
}
