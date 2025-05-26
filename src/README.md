# 🧊 Yetii CLI

**Yetii** is a command-line tool built in Rust to help developers integrate ERP systems with flexible configuration management. It enables initializing config files and performing ODBC driver checks in preparation for querying various databases.

> ⚠️ This is a work-in-progress tool. Features are still under development.

---

## 🚀 Current Features

### ✅ `init` — Initialize Configuration

Generate a starter Yetii configuration YAML file with default values.

**Usage:**

```bash
  yetii init --config yetii.config --path .
```

* `--config`, `-c`: Name of the config file (default: `yetii.config`)
* `--path`, `-p`: Target directory for the config file (default: current directory)

Creates a YAML file like this:

```yaml
version: "0.0.1"
name: "yetii.config"
...
```

---

### ✅ `odbc` — Check for Installed ODBC Drivers

Lists ODBC drivers available on your system.

**Usage:**

```bash
  yetii odbc
```

Useful before trying to connect to databases using ODBC.

---

## 🔧 Building and Running

### Build the project

```bash
  cargo build
```

### Run the CLI

```bash
  cargo run -- <COMMAND>
```

---

## 📅 Roadmap

* [x] Config initialization
* [x] ODBC environment check
* [ ] Load and parse full YAML config
* [ ] Execute parameterized SQL queries
* [ ] API endpoint delivery (POST/PUT)
* [ ] Error handling and monitoring
* [ ] Config validation + schema

---

## 📄 License

MIT © 2025 Emmanuel Muthiani

---

## 🧪 Dev Notes

This tool uses the [`clap`](https://docs.rs/clap/latest/clap/) crate for CLI parsing and `serde_yaml` for config serialization.

```rust
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
```

---

## 🤝 Contributing

This is an internal project for now. Open to external feedback soon.