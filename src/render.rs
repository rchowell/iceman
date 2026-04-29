use std::fmt::Write as _;
use std::io::{self, Write};

use anyhow::Result;
use serde::Serialize;

use crate::cli::OutputFormat;

pub enum Cell {
    Str(String),
    Int(i64),
    UInt(u64),
    Bool(bool),
    Null,
}

impl Cell {
    fn display(&self) -> String {
        match self {
            Cell::Str(s) => s.clone(),
            Cell::Int(n) => n.to_string(),
            Cell::UInt(n) => n.to_string(),
            Cell::Bool(b) => b.to_string(),
            Cell::Null => String::new(),
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(self, Cell::Int(_) | Cell::UInt(_))
    }
}

pub trait Tabular {
    fn headers() -> &'static [&'static str];
    fn row(&self) -> Vec<Cell>;
}

pub trait DisplayText {
    fn fmt_text(&self, w: &mut dyn Write) -> io::Result<()>;
}

pub fn render_rows<T: Serialize + Tabular>(rows: &[T], fmt: OutputFormat) -> Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(rows)?);
        }
        OutputFormat::Text => print_table(rows),
    }
    Ok(())
}

pub fn render_one<T: Serialize + DisplayText>(item: &T, fmt: OutputFormat) -> Result<()> {
    match fmt {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(item)?);
        }
        OutputFormat::Text => {
            let stdout = io::stdout();
            let mut w = stdout.lock();
            item.fmt_text(&mut w)?;
        }
    }
    Ok(())
}

fn print_table<T: Tabular>(rows: &[T]) {
    let headers = T::headers();
    let cells: Vec<Vec<Cell>> = rows.iter().map(Tabular::row).collect();

    let cols = headers.len();
    let numeric: Vec<bool> = (0..cols)
        .map(|c| !cells.is_empty() && cells.iter().all(|r| r[c].is_numeric()))
        .collect();

    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    let displayed: Vec<Vec<String>> = cells
        .iter()
        .map(|r| r.iter().map(Cell::display).collect())
        .collect();
    for row in &displayed {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let mut line = String::new();
    for (i, h) in headers.iter().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        let _ = if numeric[i] {
            write!(line, "{h:>width$}", width = widths[i])
        } else {
            write!(line, "{h:<width$}", width = widths[i])
        };
    }
    println!("{line}");

    for row in &displayed {
        line.clear();
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            let _ = if numeric[i] {
                write!(line, "{cell:>width$}", width = widths[i])
            } else {
                write!(line, "{cell:<width$}", width = widths[i])
            };
        }
        println!("{line}");
    }
}
