# iceman command reference

Compact reference for every `iceman` subcommand and flag. For SQL guidance over
metadata views, see `sql.md`. For catalog config, see `config.md`.

## Global flags

These apply to every subcommand:

| Flag                         | Purpose                                                  |
|------------------------------|----------------------------------------------------------|
| `-c, --config PATH`          | Path to config file. Env: `ICEMAN_CONFIG`.               |
| `--catalog NAME`             | Pick a named catalog from the config.                    |
| `--output text\|json`        | Output format. Default `text`.                           |
| `--uri URI`                  | Override catalog URI (also used for type inference).     |
| `--credential CRED`          | Override catalog credential.                             |
| `--warehouse PATH`           | Override warehouse location.                             |

## `iceman list [PATTERN]`

Walks every namespace (recursively) and lists both namespaces and tables.

- `PATTERN` is an optional glob filter applied to the full dot-separated identifier
  (e.g. `analytics.*`, `*orders*`, `db.schema.fact_*`).
- Output columns: `type` (`namespace` | `table`), `name`.

```
iceman list
iceman list 'analytics.*'
iceman list --output json | jq -r 'select(.type=="table") | .name'
```

## `iceman describe IDENT`

Auto-detects whether `IDENT` is a namespace or a table and prints its description.

| Flag                  | Purpose                                                       |
|-----------------------|---------------------------------------------------------------|
| `--entity any`        | Default. Try table first, fall back to namespace.             |
| `--entity namespace`  | Force namespace lookup.                                       |
| `--entity table`      | Force table lookup.                                           |

```
iceman describe analytics
iceman describe analytics.events --entity table
```

## `iceman inspect IDENT [METADATA_TABLE]`

Inspect a table's internals. Two modes:

**Typed mode** (positional metadata table):

| Value             | Returns                                                  |
|-------------------|----------------------------------------------------------|
| `snapshots`       | All snapshots.                                           |
| `history`         | Snapshot ancestry of the current ref.                    |
| `metadata-log`    | Metadata file evolution.                                 |
| `refs`            | Branches and tags.                                       |
| `manifests`       | Manifests in the current (or `--snapshot-id`) snapshot.  |
| `all-manifests`   | Manifests across all snapshots.                          |
| `entries`         | Manifest entries in the current snapshot.                |
| `all-entries`     | Manifest entries across all snapshots.                   |
| `files`           | Live data + delete files.                                |
| `data-files`      | Live data files only.                                    |
| `delete-files`    | Live delete files only.                                  |
| `all-data-files`  | Data files across all snapshots.                         |
| `all-delete-files`| Delete files across all snapshots.                       |
| `partitions`      | Per-partition record/file counts.                        |

**SQL mode** (`-q / --query`):

Runs DuckDB SQL across the metadata views. The view set is the same as the typed
mode names (snake_case: `snapshots`, `history`, `metadata_log_entries`, `refs`,
`manifests`, `all_manifests`, `entries`, `all_entries`, `files`, `data_files`,
`delete_files`, `all_data_files`, `all_delete_files`, `partitions`). Mutually
exclusive with the positional metadata table.

| Flag                   | Purpose                                                |
|------------------------|--------------------------------------------------------|
| `-q, --query SQL`      | DuckDB SQL over the metadata views.                    |
| `--snapshot-id N`      | Use snapshot `N` instead of current (typed mode only). |
| `--limit N`            | Cap displayed rows.                                    |
| `-v, --verbose`        | Show all columns. Default is a terse subset chosen for at-a-glance reading. |

```
iceman inspect analytics.events snapshots --limit 5
iceman inspect analytics.events files --snapshot-id 5723145 --output json
iceman inspect analytics.events -q \
  "SELECT count(*) AS n_files, sum(file_size_in_bytes) AS bytes FROM files"
```

## Output format

`--output text` (default) prints aligned ASCII (no Unicode borders). iceman writes
straight to stdout - there is no built-in pager. Same behavior for every command.

- On a TTY: columns are space-padded for readability. Wide tables (especially
  `iceman inspect -v`) will line-wrap unless you pipe through a pager yourself.
- When piped or redirected: plain TSV. Header on line 1, columns are tab-separated, no
  padding. `awk -F'\t'`, `cut -f`, and friends work directly.

**Reading wide tables.** Pipe through `less -S` so it truncates instead of wrapping:

```sh
iceman inspect TABLE files -v | less -S          # scroll left/right
iceman inspect TABLE files -v | less -RSFX       # also auto-quit if it fits
iceman inspect TABLE files -v | column -t -s$'\t' | less -S   # re-aligned
```

`-S` is the load-bearing flag - without it `less` wraps and the table is unreadable
again.

`--output json` (multi-row commands) emits JSONL: one compact JSON object per line.
Pipe straight to `jq '.field'` - no `jq '.[]'` needed - or to any line-based JSON tool.
`iceman describe` is a single entity, so it stays as one pretty-printed JSON object.

By default, `iceman inspect` shows a terse column subset; pass `-v` for the full set.
The SQL form (`-q`) chooses columns explicitly via `SELECT`, so `-v` doesn't apply.

## Composing with Unix tools

Output is pipe-first. The two modes complement each other:

| Tool       | Use with TSV                                            | Use with JSONL                                           |
|------------|---------------------------------------------------------|----------------------------------------------------------|
| `cut`      | `cut -f3` picks column 3 by index                       | -                                                        |
| `awk`      | `awk -F'\t' 'NR>1 {sum+=$3} END{print sum}'`           | works but `jq` is cleaner                                |
| `sort`     | `sort -t$'\t' -k3 -nr` sort by column 3 numeric desc    | `jq -s 'sort_by(.field)[]'`                              |
| `uniq -c`  | `cut -f1 \| sort \| uniq -c` value frequencies          | `jq -r '.field' \| sort \| uniq -c`                      |
| `grep`     | row-level text filter on the table                      | line-level filter on the JSON object                     |
| `jq`       | -                                                       | streams object-by-object: `jq -r '.field'`, no `.[]`     |
| `jq -s`    | -                                                       | slurp into array: `jq -s 'length'`, `jq -s 'group_by(...)'` |
| `column -t`| `column -t -s$'\t'` re-aligns the TSV in a terminal     | -                                                        |
| `less -S`  | scroll horizontally instead of wrapping wide tables     | also useful for huge JSONL dumps                         |
| `head/tail`| works (skip header with `tail -n +2`)                   | works directly (each line is a row)                      |
| `wc -l`    | rows = `wc -l` minus 1 for the header                   | rows = `wc -l` exactly                                   |

### Single-tab guarantee

TSV uses exactly one `\t` between columns - never a run. Two consecutive tabs would be
parsed as an empty middle field by `cut -f` and `awk -F'\t'`, shifting every column
index after that point. Iceman never emits that. Column indexes are stable across rows;
`cut -f5` picks the 5th column on every row, header included.

### Empty cells

Null / missing values render as the empty string in TSV, so a row like
`a\t\tc` (two tabs) means "field 2 is null". `awk -F'\t'` will report `NF` equal to
the column count and `$2 == ""`. `cut -f2` will print an empty line for that row.

### Patterns

```sh
# pick one column → list of values
iceman inspect TABLE files | tail -n +2 | cut -f4

# stream the same column from JSONL (column name, not index)
iceman inspect TABLE files --output json | jq -r '.file_path'

# numeric aggregation on TSV
iceman inspect TABLE files \
  | awk -F'\t' 'NR>1 {bytes+=$3; n++} END {printf "%d files, %d bytes, avg %d\n", n, bytes, bytes/n}'

# sort largest-first on a column index
iceman inspect TABLE files | sort -t$'\t' -k3 -nr | head -20

# group / count via the shell
iceman inspect TABLE files --output json \
  | jq -r '.content' | sort | uniq -c

# join iceman output to external data via the file_path column
iceman inspect TABLE files --output json \
  | jq -r '.file_path' \
  | xargs -n1 ls -l

# pretty-print TSV in the terminal after filtering
iceman inspect TABLE files | grep -v 'staging/' | column -t -s$'\t' | less -S
```

### When to prefer `-q SQL` over shell pipes

DuckDB inside iceman can do joins, aggregations, and window functions directly across
metadata views. Reach for `-q` when:

- You need to join two views (e.g. `entries` to `snapshots`).
- You need numeric aggregates (`sum`, `avg`, `count`) over thousands of rows.
- You need conditional filtering with multiple predicates.

Reach for shell pipes when:

- You're combining iceman with external data (filesystem, other CLIs, other tables).
- You want one column as a list to feed into `xargs` / a loop.
- The "query" is really just a projection or a simple filter.

## Empty-snapshot behavior

A table with no current snapshot still has metadata, but most views need a
snapshot to materialize.

| Snapshot required? | Metadata tables                                              |
|--------------------|--------------------------------------------------------------|
| No                 | `snapshots`, `history`, `metadata-log`, `refs`               |
| Yes                | `manifests`, `all-manifests`, `entries`, `all-entries`, `files`, `data-files`, `delete-files`, `all-data-files`, `all-delete-files`, `partitions` |

In SQL mode, snapshot-required views are simply skipped during materialization
- the query sees them as missing tables and DuckDB raises a parse error.

## Exit behavior

- Returns non-zero on any error (catalog load failure, table not found, SQL parse error).
- `iceman list` on an empty catalog returns 0 with no rows.

## `iceman skill install [LOCATION]`

Installs this skill (the one you are reading) onto disk so Claude Code can pick it up.

| Position / Flag     | Purpose                                                      |
|---------------------|--------------------------------------------------------------|
| `LOCATION` (pos.)   | Parent dir; `iceman/` is created inside. Default `./.claude/skills`. |
| `--user`            | Install into `~/.claude/skills/iceman/` instead.             |
| `--force`           | Overwrite an existing install.                               |

```
iceman skill install                  # ./.claude/skills/iceman/
iceman skill install --user           # ~/.claude/skills/iceman/
iceman skill install /tmp/skills      # /tmp/skills/iceman/
iceman skill install --user --force   # refresh after upgrading iceman
```
