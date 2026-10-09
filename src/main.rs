use clap::Parser;
use interlint::app::cli::Cli;
use interlint::app::{run, RunOutcome};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match run(cli).await {
        Ok(RunOutcome::Clean) => std::process::exit(0),
        Ok(RunOutcome::Breaking) => std::process::exit(1),
        Ok(RunOutcome::Degraded) => std::process::exit(3),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    }
}
