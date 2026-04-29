use std::collections::BTreeMap;
use std::io::{self, Write};

use anyhow::Result;
use iceberg::{Catalog, Namespace};
use serde::Serialize;

use crate::cli::{EntityType, Identifier};
use crate::render::DisplayText;

#[derive(Debug, Serialize)]
pub struct SchemaField {
    pub id: i32,
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub required: bool,
}

#[derive(Debug, Serialize)]
pub struct PartitionField {
    pub name: String,
    pub source_id: i32,
    pub transform: String,
}

#[derive(Debug, Serialize)]
pub struct SortField {
    pub field: String,
    pub direction: String,
    pub null_order: String,
}

#[derive(Debug, Serialize)]
pub struct TableInfo {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub identifier: String,
    pub location: String,
    pub uuid: String,
    pub format_version: u8,
    pub current_snapshot_id: Option<i64>,
    pub current_snapshot_timestamp_ms: Option<i64>,
    pub snapshot_count: usize,
    pub schema: Vec<SchemaField>,
    pub partition_spec: Vec<PartitionField>,
    pub sort_order: Vec<SortField>,
    pub properties: BTreeMap<String, String>,

    #[serde(skip)]
    pub current_snapshot_op: String,
}

#[derive(Debug, Serialize)]
pub struct NamespaceInfo {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub identifier: String,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Described {
    Table(TableInfo),
    Namespace(NamespaceInfo),
}

impl DisplayText for Described {
    fn fmt_text(&self, w: &mut dyn Write) -> io::Result<()> {
        match self {
            Described::Table(t) => t.fmt_text(w),
            Described::Namespace(n) => n.fmt_text(w),
        }
    }
}

impl DisplayText for TableInfo {
    fn fmt_text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "table:     {}", self.identifier)?;
        writeln!(w, "location:  {}", self.location)?;
        writeln!(w, "uuid:      {}", self.uuid)?;
        writeln!(w, "format:    v{}", self.format_version)?;
        if let Some(id) = self.current_snapshot_id {
            let ts_str = self
                .current_snapshot_timestamp_ms
                .map(fmt_timestamp)
                .unwrap_or_default();
            writeln!(w, "snapshot:  {id} ({}, {ts_str})", self.current_snapshot_op)?;
        }
        writeln!(w, "history:   {} snapshots", self.snapshot_count)?;

        writeln!(w, "\nschema:")?;
        let name_w = self.schema.iter().map(|f| f.name.len()).max().unwrap_or(4).max(4);
        let type_w = self.schema.iter().map(|f| f.field_type.len()).max().unwrap_or(4).max(4);
        writeln!(
            w,
            "  {:>4}  {:<name_w$}  {:<type_w$}  required",
            "id", "name", "type"
        )?;
        for f in &self.schema {
            writeln!(
                w,
                "  {:>4}  {:<name_w$}  {:<type_w$}  {}",
                f.id, f.name, f.field_type, f.required
            )?;
        }

        if !self.partition_spec.is_empty() {
            writeln!(w, "\npartition:")?;
            for pf in &self.partition_spec {
                writeln!(w, "  {}  {}", pf.name, pf.transform)?;
            }
        }

        if !self.sort_order.is_empty() {
            writeln!(w, "\nsort-order:")?;
            for sf in &self.sort_order {
                writeln!(w, "  {}  {}  {}", sf.field, sf.direction, sf.null_order)?;
            }
        }

        if !self.properties.is_empty() {
            writeln!(w, "\nproperties:")?;
            let kw = self.properties.keys().map(String::len).max().unwrap_or(0);
            for (k, v) in &self.properties {
                writeln!(w, "  {k:<kw$}  {v}")?;
            }
        }

        Ok(())
    }
}

impl DisplayText for NamespaceInfo {
    fn fmt_text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "namespace:  {}", self.identifier)?;
        if !self.properties.is_empty() {
            writeln!(w, "\nproperties:")?;
            let kw = self.properties.keys().map(String::len).max().unwrap_or(0);
            for (k, v) in &self.properties {
                writeln!(w, "  {k:<kw$}  {v}")?;
            }
        }
        Ok(())
    }
}

pub async fn run(
    catalog: &dyn Catalog,
    identifier: &Identifier,
    entity: &EntityType,
) -> Result<Described> {
    match entity {
        EntityType::Table => {
            let ident = identifier.as_table()?;
            let loaded = catalog.load_table(&ident).await?;
            Ok(Described::Table(table_info(&loaded, identifier.as_str())))
        }
        EntityType::Namespace => {
            let ident = identifier.as_namespace()?;
            let ns = catalog.get_namespace(&ident).await?;
            Ok(Described::Namespace(namespace_info(&ns, identifier.as_str())))
        }
        EntityType::Any => {
            if identifier.parts().len() >= 2
                && let Ok(ident) = identifier.as_table()
                && let Ok(loaded) = catalog.load_table(&ident).await
            {
                return Ok(Described::Table(table_info(&loaded, identifier.as_str())));
            }
            let ident = identifier.as_namespace()?;
            let ns = catalog.get_namespace(&ident).await?;
            Ok(Described::Namespace(namespace_info(&ns, identifier.as_str())))
        }
    }
}

fn table_info(table: &iceberg::table::Table, identifier: &str) -> TableInfo {
    let meta = table.metadata();
    let schema_struct = meta.current_schema();
    let spec = meta.default_partition_spec();
    let sort_order = meta.default_sort_order();

    let current = meta.current_snapshot();
    let snap_id = meta.current_snapshot_id();
    let snap_ts = current.map_or(0, |s| s.timestamp_ms());
    let snap_op = current
        .map(|s| s.summary().operation.as_str().to_string())
        .unwrap_or_default();

    let fields = schema_struct.as_struct().fields();
    let schema: Vec<SchemaField> = fields
        .iter()
        .map(|f| SchemaField {
            id: f.id,
            name: f.name.clone(),
            field_type: f.field_type.to_string(),
            required: f.required,
        })
        .collect();

    let partition_spec: Vec<PartitionField> = if spec.is_unpartitioned() {
        Vec::new()
    } else {
        spec.fields()
            .iter()
            .map(|pf| PartitionField {
                name: pf.name.clone(),
                source_id: pf.source_id,
                transform: pf.transform.to_string(),
            })
            .collect()
    };

    let sort_order_rows: Vec<SortField> = sort_order
        .fields
        .iter()
        .map(|sf| {
            let name = fields
                .iter()
                .find(|f| f.id == sf.source_id)
                .map_or("?", |f| f.name.as_str())
                .to_string();
            SortField {
                field: name,
                direction: sf.direction.to_string(),
                null_order: sf.null_order.to_string(),
            }
        })
        .collect();

    let properties: BTreeMap<String, String> = meta
        .properties()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    TableInfo {
        kind: "table",
        identifier: identifier.to_string(),
        location: meta.location().to_string(),
        uuid: meta.uuid().to_string(),
        format_version: meta.format_version() as u8,
        current_snapshot_id: snap_id,
        current_snapshot_timestamp_ms: if snap_ts > 0 { Some(snap_ts) } else { None },
        snapshot_count: meta.history().len(),
        schema,
        partition_spec,
        sort_order: sort_order_rows,
        properties,
        current_snapshot_op: snap_op,
    }
}

fn namespace_info(ns: &Namespace, identifier: &str) -> NamespaceInfo {
    let properties: BTreeMap<String, String> = ns
        .properties()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    NamespaceInfo {
        kind: "namespace",
        identifier: identifier.to_string(),
        properties,
    }
}

fn fmt_timestamp(ms: i64) -> String {
    if ms == 0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp_millis(ms).map_or_else(
        || ms.to_string(),
        |dt: chrono::DateTime<chrono::Utc>| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
}

