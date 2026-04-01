use anyhow::Result;
use clap::Parser;

use iceman::cli::pyiceman::PyIcemanCli;
use iceman::cli::{
    Command, ConfigAction, CreateCommand, DropCommand, PropertiesCommand, PropertiesGetCommand,
    PropertiesRemoveCommand, PropertiesSetCommand,
};
use iceman::core::config::pyiceberg::{find_config_file, init_default_config, load_config};

fn main() -> Result<()> {
    let cli = PyIcemanCli::parse();

    match cli.command {
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
