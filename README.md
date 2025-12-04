# cargo-const

`cargo-const` is a command-line tool for analyzing crate compatibility in Rust projects. It helps you find versions of a crate that are compatible with your project's dependencies by inspecting the `Cargo.lock` file.

---

## What it does

- Find versions of a crate compatible with your project's dependencies.
- Limit the number of results or list all found versions.
- Filter versions by maximum supported Rust version.

---

## Installation

Install via Cargo:

```bash
cargo install cargo-const
````

---

## Commands

### `compat`

Finds compatible versions of a crate based on your project's dependencies.

#### Arguments

* `dependency` – The crate to check for compatibility (required).

#### Flags

* `-v, --verbose` – Enable verbose logging.
* `-i, --include-yanked` – Include yanked versions in the results.
* `-c, --count <COUNT>` – Number of versions to list. Can be a number or `"all"` (default: `5`).
* `-p, --path <PATH>` – Path to your `Cargo.lock` file (default: `Cargo.lock`).
* `-m, --max-version <VERSION>` – Maximum Rust version supported by the crate.

---

## Example

Find all compatible versions of the `indexmap` crate with verbose logging enabled:

```bash
user:~$ cargo-const compat indexmap --count all --verbose
Info: Cache successfully created at "/home/user/.local/share/cargo-const-0.2.0/dependencies/toml_edit/0.23.7"
Info: Cache successfully created at "/home/user/.local/share/cargo-const-0.2.0/versions/indexmap"
Compatible versions found:

2.12.0   min-rust-version = 1.82
2.11.4   min-rust-version = 1.63
```

## Implementation Notes

* Fetches all crate information from the project's `Cargo.lock`.
* Determines compatible versions by combining dependency bounds; in cases where multiple unrelated dependents impose disjoint constraints, this may incorrectly conclude that no compatible versions exist (i.e., it may treat resolvable scenarios as unsatisfiable).
---

## Contributing

Contributions are welcome! Fork the repository, make your changes, and open a pull request.

---

## License

MIT License
