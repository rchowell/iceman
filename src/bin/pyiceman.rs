use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use iceberg::Catalog;
use iceberg::{NamespaceIdent, TableIdent};

use iceman::cli::{
    Command, ConfigAction, CreateCommand, DropCommand, EntityType, OutputFormat, PropertiesCommand,
    PropertiesGetCommand, PropertiesRemoveCommand, PropertiesSetCommand,
};
use iceman::core::catalog::load_catalog;
use iceman::core::config::CatalogConfig;
use iceman::core::config::pyiceberg::{find_config_file, init_default_config, load_config};

/// This is the 'pyiceman' CLI definition.
#[derive(Debug, Parser)]
#[command(name = "pyiceman", about = "A pyiceberg-compatible CLI")]
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
    /// UGI (overrides config)
    #[arg(long, global = true)]
    pub ugi: Option<String>,
    /// The pyiceberg command to execute
    #[command(subcommand)]
    pub command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = PyIcemanCli::parse();

    match cli.command {
        Command::List { ref parent } => {
            let catalog = resolve_catalog(&cli).await?;
            let json = matches!(cli.output, OutputFormat::Json);

            let items: Vec<String> = match parent {
                None => {
                    let namespaces = catalog.list_namespaces(None).await?;
                    namespaces.iter().map(|ns| ns.to_string()).collect()
                }
                Some(parent_str) => {
                    let ns = NamespaceIdent::from_vec(
                        parent_str.split('.').map(String::from).collect(),
                    )?;
                    let tables = catalog.list_tables(&ns).await?;
                    tables.iter().map(|t| t.to_string()).collect()
                }
            };

            if json {
                println!("{}", serde_json::to_string(&items)?);
            } else {
                for item in &items {
                    println!("{item}");
                }
            }
            Ok(())
        }
        Command::Describe {
            ref identifier,
            ref entity,
        } => {
            let catalog = resolve_catalog(&cli).await?;
            let json = matches!(cli.output, OutputFormat::Json);
            let parts: Vec<String> = identifier.split('.').map(String::from).collect();

            let mut found_namespace = false;
            let mut found_table = false;
            let mut last_error: Option<anyhow::Error> = None;

            // Try namespace
            if matches!(entity, EntityType::Namespace | EntityType::Any) && !parts.is_empty() {
                let ns_ident = NamespaceIdent::from_vec(parts.clone())?;
                match catalog.get_namespace(&ns_ident).await {
                    Ok(ns) => {
                        let props = ns.properties();
                        if json {
                            println!("{}", serde_json::to_string(props)?);
                        } else {
                            for (k, v) in props {
                                println!("{k}\t{v}");
                            }
                        }
                        found_namespace = true;
                    }
                    Err(e) => {
                        if !matches!(entity, EntityType::Any) || parts.len() == 1 {
                            return Err(anyhow::anyhow!(e));
                        }
                    }
                }
            }

            // Try table
            if matches!(entity, EntityType::Table | EntityType::Any) && parts.len() > 1 {
                let table_ident = TableIdent::from_strs(parts)?;
                match catalog.load_table(&table_ident).await {
                    Ok(table) => {
                        if json {
                            let metadata = table.metadata();
                            let value = serde_json::json!({
                                "identifier": table.identifier().to_string(),
                                "metadata_location": table.metadata_location().unwrap_or(""),
                                "metadata": {
                                    "format-version": metadata.format_version() as u8,
                                    "table-uuid": metadata.uuid().to_string(),
                                    "location": metadata.location(),
                                    "last-updated-ms": metadata.last_updated_ms(),
                                    "properties": metadata.properties(),
                                    "current-schema-id": metadata.current_schema_id(),
                                    "default-spec-id": metadata.default_partition_spec_id(),
                                    "default-sort-order-id": metadata.default_sort_order_id(),
                                    "snapshots": metadata.snapshots()
                                        .map(|s| serde_json::json!({
                                            "snapshot-id": s.snapshot_id(),
                                            "schema-id": s.schema_id(),
                                            "manifest-list": s.manifest_list(),
                                            "summary": s.summary(),
                                            "timestamp-ms": s.timestamp_ms(),
                                        }))
                                        .collect::<Vec<_>>(),
                                }
                            });
                            println!("{}", serde_json::to_string(&value)?);
                        } else {
                            let metadata = table.metadata();
                            println!(
                                "Table format version\t{}",
                                metadata.format_version() as u8
                            );
                            println!(
                                "Metadata location\t{}",
                                table.metadata_location().unwrap_or("")
                            );
                            println!("Table UUID\t{}", metadata.uuid());
                            println!("Last Updated\t{}", metadata.last_updated_ms());
                            println!("Partition spec\t{:?}", metadata.default_partition_spec());
                            println!("Sort order\t{:?}", metadata.default_sort_order());
                            println!("Current schema\t{:?}", metadata.current_schema());
                            println!("Current snapshot\t{:?}", metadata.current_snapshot());
                            println!("Snapshots");
                            for snapshot in metadata.snapshots() {
                                println!(
                                    "\tSnapshot {}, schema {}: {}",
                                    snapshot.snapshot_id(),
                                    snapshot.schema_id().unwrap_or(0),
                                    snapshot.manifest_list(),
                                );
                            }
                            println!("Properties");
                            for (k, v) in metadata.properties() {
                                println!("\t{k}\t{v}");
                            }
                        }
                        found_table = true;
                    }
                    Err(e) => {
                        if !matches!(entity, EntityType::Any) {
                            return Err(anyhow::anyhow!(e));
                        }
                        last_error = Some(anyhow::anyhow!(e));
                    }
                }
            }

            if !found_namespace && !found_table {
                match last_error {
                    Some(e) => return Err(e),
                    None => anyhow::bail!("Table or namespace does not exist: {identifier}"),
                }
            }
            Ok(())
        }
        Command::Files {
            identifier,
            history,
        } => {
            todo!("files {} (history={})", identifier, history)
        }
        Command::Schema { ref identifier } => {
            let catalog = resolve_catalog(&cli).await?;
            let json = matches!(cli.output, OutputFormat::Json);
            let parts: Vec<String> = identifier.split('.').map(String::from).collect();
            let table_ident = TableIdent::from_strs(parts)?;
            let table = catalog.load_table(&table_ident).await?;
            let schema = table.metadata().current_schema();
            if json {
                println!("{}", serde_json::to_string(schema.as_ref())?);
            } else {
                for field in schema.as_struct().fields() {
                    let doc = field.doc.as_deref().unwrap_or("");
                    println!("{}\t{}\t{}", field.name, field.field_type, doc);
                }
            }
            Ok(())
        }
        Command::Spec { ref identifier } => {
            let catalog = resolve_catalog(&cli).await?;
            let json = matches!(cli.output, OutputFormat::Json);
            let parts: Vec<String> = identifier.split('.').map(String::from).collect();
            let table_ident = TableIdent::from_strs(parts)?;
            let table = catalog.load_table(&table_ident).await?;
            let spec = table.metadata().default_partition_spec();
            if json {
                println!("{}", serde_json::to_string(spec.as_ref())?);
            } else {
                if spec.fields().is_empty() {
                    println!("[]");
                } else {
                    for field in spec.fields() {
                        println!(
                            "{}\t{}\t{}\t{}",
                            field.field_id, field.source_id, field.name, field.transform
                        );
                    }
                }
            }
            Ok(())
        }
        Command::Uuid { ref identifier } => {
            let catalog = resolve_catalog(&cli).await?;
            let json = matches!(cli.output, OutputFormat::Json);
            let parts: Vec<String> = identifier.split('.').map(String::from).collect();
            let table_ident = TableIdent::from_strs(parts)?;
            let table = catalog.load_table(&table_ident).await?;
            let uuid = table.metadata().uuid();
            if json {
                println!("{}", serde_json::json!({"uuid": uuid.to_string()}));
            } else {
                println!("{uuid}");
            }
            Ok(())
        }
        Command::Location { ref identifier } => {
            let catalog = resolve_catalog(&cli).await?;
            let json = matches!(cli.output, OutputFormat::Json);
            let parts: Vec<String> = identifier.split('.').map(String::from).collect();
            let table_ident = TableIdent::from_strs(parts)?;
            let table = catalog.load_table(&table_ident).await?;
            let location = table.metadata().location();
            if json {
                println!("{}", serde_json::to_string(location)?);
            } else {
                println!("{location}");
            }
            Ok(())
        }
        Command::Version => {
            println!("pyiceman {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Rename { from, to } => {
            todo!("rename {} -> {}", from, to)
        }
        Command::ListRefs {
            identifier,
            ref_type,
        } => {
            todo!("list-refs {} (type={:?})", identifier, ref_type)
        }
        Command::Create { ref command } => match command {
            CreateCommand::Namespace { identifier } => {
                let catalog = resolve_catalog(&cli).await?;
                let ns = NamespaceIdent::from_vec(
                    identifier.split('.').map(String::from).collect(),
                )?;
                catalog.create_namespace(&ns, HashMap::new()).await?;
                println!("Created namespace: {identifier}");
                Ok(())
            }
        },
        Command::Drop { ref command } => match command {
            DropCommand::Table { identifier } => {
                let catalog = resolve_catalog(&cli).await?;
                let parts: Vec<String> = identifier.split('.').map(String::from).collect();
                let table_ident = TableIdent::from_strs(parts)?;
                catalog.drop_table(&table_ident).await?;
                println!("Dropped table: {identifier}");
                Ok(())
            }
            DropCommand::Namespace { identifier } => {
                let catalog = resolve_catalog(&cli).await?;
                let ns = NamespaceIdent::from_vec(
                    identifier.split('.').map(String::from).collect(),
                )?;
                catalog.drop_namespace(&ns).await?;
                println!("Dropped namespace: {identifier}");
                Ok(())
            }
        },
        Command::Properties { command } => match command {
            PropertiesCommand::Get { command } => match command {
                PropertiesGetCommand::Namespace {
                    identifier,
                    property_name,
                } => {
                    todo!(
                        "properties get namespace {} {:?}",
                        identifier,
                        property_name
                    )
                }
                PropertiesGetCommand::Table {
                    identifier,
                    property_name,
                } => {
                    todo!("properties get table {} {:?}", identifier, property_name)
                }
            },
            PropertiesCommand::Set { command } => match command {
                PropertiesSetCommand::Namespace {
                    identifier,
                    property_name,
                    property_value,
                } => {
                    todo!(
                        "properties set namespace {} {}={}",
                        identifier,
                        property_name,
                        property_value
                    )
                }
                PropertiesSetCommand::Table {
                    identifier,
                    property_name,
                    property_value,
                } => {
                    todo!(
                        "properties set table {} {}={}",
                        identifier,
                        property_name,
                        property_value
                    )
                }
            },
            PropertiesCommand::Remove { command } => match command {
                PropertiesRemoveCommand::Namespace {
                    identifier,
                    property_name,
                } => {
                    todo!(
                        "properties remove namespace {} {}",
                        identifier,
                        property_name
                    )
                }
                PropertiesRemoveCommand::Table {
                    identifier,
                    property_name,
                } => {
                    todo!("properties remove table {} {}", identifier, property_name)
                }
            },
        },
        Command::Init => {
            let path = init_default_config()?;
            println!("{}", path.display());
            Ok(())
        }
        Command::Config { action } => match action {
            ConfigAction::Path => {
                let path = match &cli.config {
                    Some(p) => p.clone(),
                    None => find_config_file()
                        .ok_or_else(|| anyhow::anyhow!("no .pyiceberg.yaml found"))?,
                };
                println!("{}", path.display());
                Ok(())
            }
            ConfigAction::Show => {
                let cfg = load_config(cli.config.as_deref())?;
                let out = serde_yaml::to_string(&cfg)?;
                print!("{out}");
                Ok(())
            }
        },
    }
}

/// Load config, resolve the catalog, apply CLI overrides, and connect.
async fn resolve_catalog(cli: &PyIcemanCli) -> Result<std::sync::Arc<dyn Catalog>> {
    let cfg = load_config(cli.config.as_deref())?;
    let mut catalog_ref: CatalogConfig = cfg.resolve_catalog(cli.catalog.as_deref())?;
    catalog_ref.apply_overrides(
        cli.uri.as_deref(),
        cli.warehouse.as_deref(),
        cli.credential.as_deref(),
        cli.ugi.as_deref(),
    );
    load_catalog(&catalog_ref).await
}
