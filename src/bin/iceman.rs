use anyhow::Result;
use clap::Parser;
use iceberg::{Catalog, TableIdent};

use iceman::cli::iceman::IcemanCli;
use iceman::cli::{
    Command, ConfigAction, CreateCommand, DropCommand, OutputFormat, PropertiesCommand,
    PropertiesGetCommand, PropertiesRemoveCommand, PropertiesSetCommand,
};
use iceman::core::catalog::load_catalog;
use iceman::core::config::iceman::{default_config_path, init_default_config, load_config};
use iceman::core::config::CatalogConfig;
use iceman::inspect;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = IcemanCli::parse();

    match cli.command {
        Command::Inspect {
            ref identifier,
            ref table,
            snapshot_id,
            limit,
        } => {
            let catalog = resolve_catalog(&cli).await?;
            let json = matches!(cli.output, OutputFormat::Json);
            let parts: Vec<String> = identifier.split('.').map(String::from).collect();
            let table_ident = TableIdent::from_strs(parts)?;
            let loaded = catalog.load_table(&table_ident).await?;
            inspect::run(&loaded, table, snapshot_id, limit, json).await
        }
        Command::List { parent } => {
            todo!("list namespaces/tables under {:?}", parent)
        }
        Command::Describe { identifier, entity } => {
            todo!("describe {:?} {}", entity, identifier)
        }
        Command::Files {
            identifier,
            history,
        } => {
            todo!("files {} (history={})", identifier, history)
        }
        Command::Schema { identifier } => {
            todo!("schema {}", identifier)
        }
        Command::Spec { identifier } => {
            todo!("spec {}", identifier)
        }
        Command::Uuid { identifier } => {
            todo!("uuid {}", identifier)
        }
        Command::Location { identifier } => {
            todo!("location {}", identifier)
        }
        Command::Version => {
            println!("iceman {}", env!("CARGO_PKG_VERSION"));
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
        Command::Create { command } => match command {
            CreateCommand::Namespace { identifier } => {
                todo!("create namespace {}", identifier)
            }
        },
        Command::Drop { command } => match command {
            DropCommand::Table { identifier } => {
                todo!("drop table {}", identifier)
            }
            DropCommand::Namespace { identifier } => {
                todo!("drop namespace {}", identifier)
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
                    None => default_config_path()?,
                };
                println!("{}", path.display());
                Ok(())
            }
            ConfigAction::Show => {
                let cfg = load_config(cli.config.as_deref())?;
                let out = toml::to_string_pretty(&cfg)?;
                print!("{out}");
                Ok(())
            }
        },
    }
}

async fn resolve_catalog(cli: &IcemanCli) -> Result<std::sync::Arc<dyn Catalog>> {
    let cfg = load_config(cli.config.as_deref())?;
    let mut catalog_ref: CatalogConfig = cfg.resolve_catalog(cli.catalog.as_deref())?;
    catalog_ref.apply_overrides(
        cli.uri.as_deref(),
        cli.warehouse.as_deref(),
        cli.credential.as_deref(),
        None,
    );
    load_catalog(&catalog_ref).await
}
