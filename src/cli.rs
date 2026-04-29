use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use iceberg::{NamespaceIdent, TableIdent};

#[derive(Debug, Clone)]
pub struct Identifier {
    raw: String,
    parts: Vec<String>,
}

impl Identifier {
    #[must_use]
    pub fn parse(s: &str) -> Self {
        Self {
            raw: s.to_string(),
            parts: s.split('.').map(String::from).collect(),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    pub fn as_table(&self) -> Result<TableIdent> {
        Ok(TableIdent::from_strs(self.parts.iter().cloned())?)
    }

    pub fn as_namespace(&self) -> Result<NamespaceIdent> {
        Ok(NamespaceIdent::from_strs(self.parts.iter().cloned())?)
    }
}


/// Iceberg metadata tables.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MetadataTable {
    Snapshots,
    History,
    MetadataLog,
    Refs,
    Manifests,
    AllManifests,
    Entries,
    AllEntries,
    Files,
    DataFiles,
    DeleteFiles,
    AllDataFiles,
    AllDeleteFiles,
    Partitions,
}

impl MetadataTable {
    pub const ALL: &'static [Self] = &[
        Self::Snapshots,
        Self::History,
        Self::MetadataLog,
        Self::Refs,
        Self::Manifests,
        Self::AllManifests,
        Self::Entries,
        Self::AllEntries,
        Self::Files,
        Self::DataFiles,
        Self::DeleteFiles,
        Self::AllDataFiles,
        Self::AllDeleteFiles,
        Self::Partitions,
    ];

    #[must_use]
    pub fn sql_name(self) -> &'static str {
        match self {
            Self::Snapshots => "snapshots",
            Self::History => "history",
            Self::MetadataLog => "metadata_log_entries",
            Self::Refs => "refs",
            Self::Manifests => "manifests",
            Self::AllManifests => "all_manifests",
            Self::Entries => "entries",
            Self::AllEntries => "all_entries",
            Self::Files => "files",
            Self::DataFiles => "data_files",
            Self::DeleteFiles => "delete_files",
            Self::AllDataFiles => "all_data_files",
            Self::AllDeleteFiles => "all_delete_files",
            Self::Partitions => "partitions",
        }
    }

    #[must_use]
    pub fn requires_snapshot(self) -> bool {
        !matches!(
            self,
            Self::Snapshots | Self::History | Self::MetadataLog | Self::Refs
        )
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Entity type to describe
#[derive(Debug, Clone, ValueEnum)]
pub enum EntityType {
    Any,
    Namespace,
    Table,
}

pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("ICEMAN_GIT_HASH"),
    ")"
);

#[derive(Debug, Parser)]
#[command(
    name = "iceman",
    about = "A lightweight Iceberg CLI for AI agents",
    version = VERSION
)]
pub struct IcemanCli {
    /// Path to config file (overrides default)
    #[arg(long, short, global = true, env = "ICEMAN_CONFIG")]
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

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List namespaces and tables (combined). Optional glob pattern filters by full identifier.
    List {
        /// Glob pattern (e.g. "analytics.*", "*orders*")
        pattern: Option<String>,
    },
    /// Describe a namespace or table
    Describe {
        /// Namespace or table identifier (dot-separated)
        identifier: String,

        /// Entity type to describe
        #[arg(long, default_value = "any")]
        entity: EntityType,
    },
    /// Inspect table metadata (snapshots, files, manifests, partitions, etc.)
    Inspect {
        /// Table identifier (dot-separated)
        identifier: String,
        /// Metadata table to inspect (mutually exclusive with --query)
        table: Option<MetadataTable>,
        /// DuckDB SQL query over metadata tables (snapshots, history, files, etc.)
        #[arg(long, short = 'q', conflicts_with = "table")]
        query: Option<String>,
        /// Snapshot ID (defaults to current snapshot)
        #[arg(long)]
        snapshot_id: Option<i64>,
        /// Max rows to display
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Manage Claude skills bundled with iceman
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// Print version and git ref
    Version,
}

#[derive(Debug, Subcommand)]
pub enum SkillAction {
    /// Install the bundled iceman skill so Claude Code can pick it up
    Install {
        /// Parent dir; an `iceman/` subdir is created inside.
        /// Defaults to ./.claude/skills (or ~/.claude/skills with --user).
        location: Option<PathBuf>,
        /// Install into ~/.claude/skills/iceman/ (overrides LOCATION).
        #[arg(long, conflicts_with = "location")]
        user: bool,
        /// Overwrite an existing installation.
        #[arg(long)]
        force: bool,
    },
}
