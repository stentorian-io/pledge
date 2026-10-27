use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Parser)]
#[command(
    name = "interlint",
    version,
    about = "Catch breaking API changes before publishing",
    after_help = "Build your package before running interlint; npm lifecycle scripts are disabled.\nExit codes: 0 compatible, 1 breaking changes, 2 tool error, 3 incomplete analysis."
)]
pub struct Cli {
    /// Package root containing package.json (one package per invocation)
    #[arg(default_value = ".")]
    pub path: std::path::PathBuf,

    /// Published version or dist-tag to compare against (defaults to latest)
    #[arg(long, value_name = "VERSION_OR_TAG")]
    pub against: Option<String>,

    /// Report format; JSON writes a single document to stdout
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,

    /// ANSI colors in text output
    #[arg(long, value_enum, default_value_t)]
    pub color: ColorMode,

    /// Clear the managed TypeScript toolchain cache and exit
    #[arg(long, conflicts_with = "against")]
    pub clear_cache: bool,
}
