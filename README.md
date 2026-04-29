# Iceman

An Apache Iceberg CLI built in Rust, designed for AI agents.

Powered by [iceberg-rust](https://github.com/apache/iceberg-rust).

## Install

```sh
cargo install --path .
```

## Quick start

### iceman

```sh
iceman init                # creates ~/.config/iceman/config.toml
iceman config show         # print resolved config
```

Edit `~/.config/iceman/config.toml`:

```toml
default-catalog = "local"

[catalog.local]
type = "rest"
uri = "http://localhost:8181"
warehouse = "my_warehouse"
s3.endpoint = "http://localhost:9000"
```

## Usage

```
iceman [OPTIONS] <COMMAND>

Commands:
  list        List namespaces or tables
  describe    Describe a namespace or table
  files       List data files of a table
  schema      Get the schema of a table
  spec        Get the partition spec of a table
  uuid        Get the UUID of a table
  location    Get the location of a table
  rename      Rename a table
  list-refs   List refs (branches and tags)
  create      Create a namespace
  drop        Drop a table or namespace
  properties  Get, set, or remove properties
  config      Show resolved configuration
  init        Initialize a default config file
  version     Print version

Options:
  -c, --config <PATH>       Override config file path
      --catalog <NAME>      Select catalog from config
      --output text|json    Output format (default: text)
  -v, --verbose             Verbose output
```

## License

Apache-2.0
