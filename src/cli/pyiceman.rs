use std::path::PathBuf;

use clap::Parser;

use super::{Command, OutputFormat};

#[derive(Debug, Parser)]
#[command(name = "pyiceman", about = "PyIceberg-compatible Iceberg CLI")]
pub struct PyIcemanCli {
    /// Path to .pyiceberg.yaml config file (overrides default search)
    #[arg(long, short, global = true)]
    pub config: Option<PathBuf>,

    /// Catalog name from config
    #[arg(long, global = true)]
    pub catalog: Option<String>,

    /// Verbose output
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    pub output: OutputFormat,

    /// Catalog URI (overrides config)
    #[arg(long, global = true)]
    pub uri: Option<String>,

    /// Catalog credential (overrides config)
    #[arg(long, global = true)]
    pub credential: Option<String>,

    /// Warehouse location (overrides config)
    #[arg(long, global = true)]
    pub warehouse: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}
