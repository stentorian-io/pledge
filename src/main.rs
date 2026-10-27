use clap::Parser;
use interlint::app::cli::Cli;
use interlint::app::{report, run};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let cli = Cli::parse();
    let format = cli.format;
    match run(cli).await {
        Ok(outcome) => ExitCode::from(outcome.exit_code()),
        Err(error) => {
            if let Err(report_error) = report::print_error(format, &error) {
                eprintln!("error writing report: {report_error}");
            }
            ExitCode::from(2)
        }
    }
}
