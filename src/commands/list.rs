use std::collections::VecDeque;

use anyhow::Result;
use iceberg::{Catalog, NamespaceIdent};
use serde::Serialize;

use crate::render::{Cell, Tabular};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Namespace,
    Table,
}

impl EntryKind {
    fn as_str(self) -> &'static str {
        match self {
            EntryKind::Namespace => "namespace",
            EntryKind::Table => "table",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListEntry {
    #[serde(rename = "type")]
    pub kind: EntryKind,
    pub name: String,
}

impl Tabular for ListEntry {
    fn headers(_verbose: bool) -> &'static [&'static str] {
        &["type", "name"]
    }

    fn row(&self, _verbose: bool) -> Vec<Cell> {
        vec![
            Cell::Str(self.kind.as_str().to_string()),
            Cell::Str(self.name.clone()),
        ]
    }
}

pub async fn run(catalog: &dyn Catalog, pattern: Option<&str>) -> Result<Vec<ListEntry>> {
    let mut entries: Vec<ListEntry> = Vec::new();
    let mut queue: VecDeque<NamespaceIdent> =
        catalog.list_namespaces(None).await?.into_iter().collect();

    while let Some(ns) = queue.pop_front() {
        entries.push(ListEntry {
            kind: EntryKind::Namespace,
            name: ns.to_string(),
        });

        if let Ok(children) = catalog.list_namespaces(Some(&ns)).await {
            for child in children {
                queue.push_back(child);
            }
        }

        if let Ok(tables) = catalog.list_tables(&ns).await {
            for t in tables {
                entries.push(ListEntry {
                    kind: EntryKind::Table,
                    name: t.to_string(),
                });
            }
        }
    }

    if let Some(pat) = pattern {
        let glob_pat = glob::Pattern::new(pat)?;
        entries.retain(|e| glob_pat.matches(&e.name));
    }

    Ok(entries)
}
