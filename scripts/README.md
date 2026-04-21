# Iceman Scripts

Test table generator for Iceberg — creates tables with controlled data distributions from YAML specs.

## Setup

```bash
cd scripts
uv sync
```

## Usage

```bash
python main.py tables/t1_region.yaml --dry-run         # validate spec
python main.py tables/t1_region.yaml --catalog default  # generate table
python main.py tables/t1_region.yaml --scale 0.01       # tiny test run
python main.py tables/t1_region.yaml --scale 10         # 10 GB
```

Generate all tables:
```bash
for f in tables/t*.yaml; do python main.py "$f" --catalog default; done
```

## Scale

`scale` controls data volume where `1 = 1 GB`. The script generates batches and tracks actual written bytes (`pa.Table.nbytes`) until reaching the target. Supports fractional values (e.g. `--scale 0.001` for quick tests).

## Tables

| Spec | Partitioning | Purpose |
|------|-------------|---------|
| `t1_region` | `identity(region)` | Partition pruning with 10 partitions |
| `t2_daily` | `day(ts)` | Partition pruning with ~730 daily partitions |
| `t3_sorted` | None, sorted by id | File-level stats with non-overlapping ranges |
| `t4_values` | None, unsorted | File-level stats on unsorted data |
| `t5_deletes` | `identity(region)` + deletes | Delete files + merge-on-read |

## Spec Format

See `tables/*.yaml` for examples. Sections:

- **`schema`** — column name -> `{type, distribution, range/values/length, ...}`
- **`partitions`** — column name -> `{transform, args?}`
- **`order_by`** — column name -> `{direction, null_order}`
- **`sort_data_by`** — sort generated data before writing (for file-level stats)
- **`write_chunk_size`** — rows per file (for controlling file count)
- **`properties`** — Iceberg table properties
- **`delete_filter`** — row filter applied after writing (creates delete files)

### Types

`boolean`, `int`, `long`, `float`, `double`, `decimal(p,s)`, `date`, `timestamp`, `string`, `binary`

### Distributions

`uniform`, `normal`, `zipf`, `skewed`, `sequence`
