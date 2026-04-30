use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};

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
    fn headers(verbose: bool) -> &'static [&'static str];
    fn row(&self, verbose: bool) -> Vec<Cell>;
}

pub trait DisplayText {
    fn fmt_text(&self, w: &mut dyn Write) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOpts {
    pub verbose: bool,
}

pub fn render_rows<T: Serialize + Tabular>(
    rows: &[T],
    fmt: OutputFormat,
    opts: RenderOpts,
) -> Result<()> {
    match fmt {
        OutputFormat::Json => write_jsonl(rows),
        OutputFormat::Text => {
            let stdout = io::stdout();
            let mut w = stdout.lock();
            print_table(&mut w, T::headers(opts.verbose), rows, opts.verbose)
        }
    }
}

/// Render a precomputed string table (headers + rows of strings): padded on a TTY,
/// plain TSV when piped.
pub fn render_string_table(
    headers: &[&str],
    rows: &[Vec<String>],
    numeric: &[bool],
    _opts: RenderOpts,
) -> Result<()> {
    let stdout = io::stdout();
    let mut w = stdout.lock();
    if io::stdout().is_terminal() {
        write_padded_strings(&mut w, headers, rows, numeric)
    } else {
        write_tsv(&mut w, headers, rows)
    }
}

/// Stream a sequence of `serde_json::Value` rows as JSONL.
pub fn render_jsonl_values(rows: &[&serde_json::Value]) -> Result<()> {
    let mut out = io::stdout().lock();
    for row in rows {
        serde_json::to_writer(&mut out, row)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

pub fn render_one<T: Serialize + DisplayText>(
    item: &T,
    fmt: OutputFormat,
    _opts: RenderOpts,
) -> Result<()> {
    match fmt {
        OutputFormat::Json => {
            let mut out = io::stdout().lock();
            serde_json::to_writer_pretty(&mut out, item)?;
            out.write_all(b"\n")?;
            Ok(())
        }
        OutputFormat::Text => {
            let stdout = io::stdout();
            let mut w = stdout.lock();
            item.fmt_text(&mut w).map_err(anyhow::Error::from)
        }
    }
}

fn write_jsonl<T: Serialize>(rows: &[T]) -> Result<()> {
    let mut out = io::stdout().lock();
    for row in rows {
        serde_json::to_writer(&mut out, row)?;
        out.write_all(b"\n")?;
    }
    Ok(())
}

fn print_table<T: Tabular, W: Write + ?Sized>(
    w: &mut W,
    headers: &[&str],
    rows: &[T],
    verbose: bool,
) -> Result<()> {
    let cells: Vec<Vec<Cell>> = rows.iter().map(|r| r.row(verbose)).collect();
    let displayed: Vec<Vec<String>> = cells
        .iter()
        .map(|r| r.iter().map(Cell::display).collect())
        .collect();

    if io::stdout().is_terminal() {
        write_padded(w, headers, &cells, &displayed)
    } else {
        write_tsv(w, headers, &displayed)
    }
}

fn write_padded<W: Write + ?Sized>(
    w: &mut W,
    headers: &[&str],
    cells: &[Vec<Cell>],
    displayed: &[Vec<String>],
) -> Result<()> {
    let cols = headers.len();
    let numeric: Vec<bool> = (0..cols)
        .map(|c| !cells.is_empty() && cells.iter().all(|r| r[c].is_numeric()))
        .collect();
    write_padded_strings(w, headers, displayed, &numeric)
}

fn write_padded_strings<W: Write + ?Sized>(
    w: &mut W,
    headers: &[&str],
    displayed: &[Vec<String>],
    numeric: &[bool],
) -> Result<()> {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in displayed {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let mut line = String::new();
    write_row(&mut line, headers.iter().copied(), &widths, numeric);
    writeln!(w, "{line}")?;

    for row in displayed {
        line.clear();
        write_row(&mut line, row.iter().map(String::as_str), &widths, numeric);
        writeln!(w, "{line}")?;
    }
    Ok(())
}

fn write_row<'a>(
    line: &mut String,
    cells: impl IntoIterator<Item = &'a str>,
    widths: &[usize],
    numeric: &[bool],
) {
    let last = widths.len().saturating_sub(1);
    for (i, cell) in cells.into_iter().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        if i == last && !numeric[i] {
            line.push_str(cell);
        } else if numeric[i] {
            let _ = write!(line, "{cell:>width$}", width = widths[i]);
        } else {
            let _ = write!(line, "{cell:<width$}", width = widths[i]);
        }
    }
}

fn write_tsv<W: Write + ?Sized>(
    w: &mut W,
    headers: &[&str],
    displayed: &[Vec<String>],
) -> Result<()> {
    writeln!(w, "{}", headers.join("\t"))?;
    for row in displayed {
        writeln!(w, "{}", row.join("\t"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_writes_tab_separated_values_with_header() {
        let mut buf = Vec::new();
        let headers = ["id", "name"];
        let rows = vec![
            vec!["1".to_string(), "alpha".to_string()],
            vec!["22".to_string(), "beta gamma".to_string()],
        ];
        write_tsv(&mut buf, &headers, &rows).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(out, "id\tname\n1\talpha\n22\tbeta gamma\n");
    }

    #[test]
    fn padded_aligns_numeric_right_and_strips_trailing_text() {
        let mut buf = Vec::new();
        let headers = ["id", "name"];
        let rows = vec![
            vec!["1".to_string(), "alpha".to_string()],
            vec!["22".to_string(), "beta".to_string()],
        ];
        write_padded_strings(&mut buf, &headers, &rows, &[true, false]).unwrap();
        let out = String::from_utf8(buf).unwrap();
        // numeric col right-aligned to width 2; trailing text col not padded.
        assert_eq!(out, "id  name\n 1  alpha\n22  beta\n");
    }
}
