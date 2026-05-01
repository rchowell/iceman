# iceman catalog configuration

Default config path: `~/.config/iceman/config.toml`. Override with `--config PATH`
or `ICEMAN_CONFIG=path`.

## File shape

```toml
default-catalog = "prod"

[catalog.prod]
type = "rest"
uri = "https://catalog.example.com"
warehouse = "s3://my-bucket/warehouse"
credential = "Bearer abc..."

[catalog.dev]
type = "sql"
uri = "sqlite:///tmp/iceberg.db"
warehouse = "/tmp/iceberg/warehouse"
```

- `default-catalog` is used when neither `--catalog` nor a single-catalog config
  applies. If absent, iceman looks for a catalog literally named `default`.
- Each `[catalog.NAME]` block is keyed by a name you pick; pass it via `--catalog NAME`.
- Top-level `type`, `uri`, `warehouse` are pulled out; any other keys flow through as
  catalog properties (e.g. `oauth2-server-uri`, `scope`, `region`).

## Supported catalog kinds

| `type`     | Required keys                              | Notes                                    |
|------------|--------------------------------------------|------------------------------------------|
| `rest`     | `uri`, often `warehouse`, `credential`     | OAuth/Bearer auth via `credential`.      |
| `glue`     | (AWS env / profile)                        | Uses the default AWS credential chain.   |
| `s3tables` | `warehouse` = table bucket ARN             | `warehouse` maps to `table_bucket_arn`.  |
| `hive`     | `uri = thrift://host:port`                 | Hive Metastore over Thrift.              |
| `sql`      | `uri = sqlite://...` or `postgresql://...` | JDBC-style catalog (SQLite or Postgres). |

## Type inference

If you omit `type`, iceman infers from the URI scheme (see `infer_catalog_type`):

| URI prefix                | Inferred type |
|---------------------------|---------------|
| `http://` / `https://`    | `rest`        |
| `thrift://`               | `hive`        |
| `sqlite:` / `postgresql:` | `sql`         |

Other schemes require an explicit `type`.

## Runtime overrides

Global flags override config values for the resolved catalog:

```
iceman --catalog dev list
iceman --uri http://localhost:8181 --warehouse s3://b/wh list
iceman --catalog prod --credential "$ICEBERG_TOKEN" describe analytics
```

`ICEMAN_CONFIG=path/to/config.toml iceman list` is equivalent to `--config`.

## Examples

### REST catalog with bearer token

```toml
[catalog.prod]
type = "rest"
uri = "https://catalog.example.com"
warehouse = "s3://prod-warehouse/"
credential = "Bearer eyJ..."
```

### Local SQLite catalog (good for testing)

```toml
default-catalog = "local"

[catalog.local]
type = "sql"
uri = "sqlite:///tmp/iceberg.db"
warehouse = "/tmp/iceberg/warehouse"
```

### AWS S3 Tables

```toml
[catalog.s3t]
type = "s3tables"
warehouse = "arn:aws:s3tables:us-east-1:123456789012:bucket/my-bucket"
```

(AWS credentials come from the standard credential chain - env vars, profile, IRSA, etc.)

### AWS Glue

```toml
[catalog.glue]
type = "glue"
warehouse = "s3://my-warehouse/"
```

### Hive Metastore

```toml
[catalog.hive]
type = "hive"
uri = "thrift://hive-metastore.internal:9083"
warehouse = "s3://my-warehouse/"
```
