pub mod iceman;

use clap::{Subcommand, ValueEnum};

// Shared types used by both CLIs

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum EntityType {
    Any,
    Namespace,
    Table,
}

// Shared command enums used by both CLIs

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List namespaces or tables
    List {
        /// Parent namespace
        parent: Option<String>,
    },

    /// Describe a namespace or table
    Describe {
        /// Namespace or table identifier
        identifier: String,

        /// Entity type to describe
        #[arg(long, default_value = "any")]
        entity: EntityType,
    },

    /// List data files of a table
    Files {
        /// Table identifier
        identifier: String,

        /// Include file history
        #[arg(long)]
        history: bool,
    },

    /// Get the schema of a table
    Schema {
        /// Table identifier
        identifier: String,
    },

    /// Get the partition spec of a table
    Spec {
        /// Table identifier
        identifier: String,
    },

    /// Get the UUID of a table
    Uuid {
        /// Table identifier
        identifier: String,
    },

    /// Get the location of a table
    Location {
        /// Table identifier
        identifier: String,
    },

    /// Print version information
    Version,

    /// Rename a table
    Rename {
        /// Source table identifier
        from: String,

        /// Target table identifier
        to: String,
    },

    /// List refs (branches and tags) of a table
    #[command(name = "list-refs")]
    ListRefs {
        /// Table identifier
        identifier: String,

        /// Filter by ref type
        #[arg(long = "type")]
        ref_type: Option<String>,
    },

    /// Create a namespace
    Create {
        #[command(subcommand)]
        command: CreateCommand,
    },

    /// Drop a table or namespace
    Drop {
        #[command(subcommand)]
        command: DropCommand,
    },

    /// Get, set, or remove properties on namespaces and tables
    Properties {
        #[command(subcommand)]
        command: PropertiesCommand,
    },

    /// Initialize a default config file
    Init,

    /// Show resolved configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

// -- create --

#[derive(Debug, Subcommand)]
pub enum CreateCommand {
    /// Create a namespace
    Namespace {
        /// Namespace identifier
        identifier: String,
    },
}

// -- drop --

#[derive(Debug, Subcommand)]
pub enum DropCommand {
    /// Drop a table
    Table {
        /// Table identifier
        identifier: String,
    },

    /// Drop a namespace
    Namespace {
        /// Namespace identifier
        identifier: String,
    },
}

// -- properties --

#[derive(Debug, Subcommand)]
pub enum PropertiesCommand {
    /// Get properties
    Get {
        #[command(subcommand)]
        command: PropertiesGetCommand,
    },

    /// Set a property
    Set {
        #[command(subcommand)]
        command: PropertiesSetCommand,
    },

    /// Remove a property
    Remove {
        #[command(subcommand)]
        command: PropertiesRemoveCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum PropertiesGetCommand {
    /// Get namespace properties
    Namespace {
        /// Namespace identifier
        identifier: String,

        /// Specific property name (omit for all)
        property_name: Option<String>,
    },

    /// Get table properties
    Table {
        /// Table identifier
        identifier: String,

        /// Specific property name (omit for all)
        property_name: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum PropertiesSetCommand {
    /// Set a namespace property
    Namespace {
        /// Namespace identifier
        identifier: String,

        /// Property name
        property_name: String,

        /// Property value
        property_value: String,
    },

    /// Set a table property
    Table {
        /// Table identifier
        identifier: String,

        /// Property name
        property_name: String,

        /// Property value
        property_value: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum PropertiesRemoveCommand {
    /// Remove a namespace property
    Namespace {
        /// Namespace identifier
        identifier: String,

        /// Property name
        property_name: String,
    },

    /// Remove a table property
    Table {
        /// Table identifier
        identifier: String,

        /// Property name
        property_name: String,
    },
}

// -- config --

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the path to the active config file
    Path,

    /// Show the resolved configuration
    Show,
}
