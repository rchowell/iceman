use anyhow::Result;
use clap::Parser;

use iceman::catalog::resolve_catalog;
use iceman::cli::{Command, Identifier, IcemanCli, SkillAction, VERSION};
use iceman::commands::{describe, inspect, list, skill};
use iceman::render::{render_one, render_rows};

#[tokio::main]
async fn main() -> Result<()> {
    sigpipe::reset();
    let cli = IcemanCli::parse();

    if let Command::Skill { action } = &cli.command {
        return match action {
            SkillAction::Install {
                location,
                user,
                force,
            } => skill::install(location.as_deref(), *user, *force),
        };
    }

    if let Command::Version = &cli.command {
        println!("iceman {VERSION}");
        return Ok(());
    }

    match cli.command {
        Command::List { ref pattern } => {
            let catalog = resolve_catalog(&cli).await?;
            let entries = list::run(catalog.as_ref(), pattern.as_deref()).await?;
            render_rows(&entries, cli.output)
        }

        Command::Describe {
            ref identifier,
            ref entity,
        } => {
            let catalog = resolve_catalog(&cli).await?;
            let ident = Identifier::parse(identifier);
            let described = describe::run(catalog.as_ref(), &ident, entity).await?;
            render_one(&described, cli.output)
        }

        Command::Inspect {
            ref identifier,
            ref table,
            ref query,
            snapshot_id,
            limit,
        } => {
            let catalog = resolve_catalog(&cli).await?;
            let ident = Identifier::parse(identifier);
            let loaded = catalog.load_table(&ident.as_table()?).await?;
            inspect::run(
                &loaded,
                table.as_ref(),
                query.as_deref(),
                snapshot_id,
                limit,
                cli.output,
            )
            .await
        }

        Command::Skill { .. } | Command::Version => {
            unreachable!("handled before catalog resolution")
        }
    }
}
