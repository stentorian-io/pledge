use clap::Parser;

#[derive(Parser)]
#[command(name = "interlint", version, about = "Catch breaking API changes before publishing")]
pub struct Cli {
    #[arg(default_value = ".")]
    pub path: std::path::PathBuf,
}
