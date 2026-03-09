# AGENTS.md - Prism

Prism is an agentic PR review CLI tool written in Rust (edition 2024).
The project is early-stage with a single binary crate located in `prism-cli/`.

---

## Project Structure

```
prism/
├── AGENTS.md
├── README.md
├── .gitignore
└── prism-cli/
    ├── Cargo.toml        # crate manifest (name = "prism", v0.1.0)
    ├── Cargo.lock
    └── src/
        ├── main.rs       # entry point, CLI parsing (async via tokio)
        ├── review.rs     # ReviewTarget parsing + review orchestration
        └── github/
            ├── mod.rs    # re-exports
            ├── client.rs # GitHub API client (GITHUB_TOKEN auth)
            ├── repo.rs   # detect owner/repo from git remote origin
            └── types.rs  # serde structs for GitHub API responses
```

The binary crate lives in `prism-cli/`. There is no Cargo workspace at the
repo root; all cargo commands must be run from `prism-cli/`.

### Environment Variables

| Variable       | Required | Purpose                                              |
|----------------|----------|------------------------------------------------------|
| `GITHUB_TOKEN` | Yes      | GitHub personal access token for API authentication  |
| `RUST_LOG`     | No       | Override log level (default: `info`)                 |

---

## Build / Run / Test Commands

All commands are run from the `prism-cli/` directory.

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run the CLI
cargo run -- review <pr_number>
cargo run -- review <github_pr_url>
cargo run -- review <commit_sha>

# Run all tests
cargo test

# Run a single test by name (substring match)
cargo test <test_name>

# Run tests in a specific module
cargo test <module_path>::

# Run tests with output printed (even passing tests)
cargo test -- --nocapture

# Lint with clippy
cargo clippy -- -D warnings

# Format code
cargo fmt

# Check formatting without modifying files
cargo fmt -- --check
```

---

## Dependencies

| Crate        | Version | Purpose                              |
|------------- |---------|--------------------------------------|
| `clap`       | 4.5.60  | CLI argument parsing (derive macros) |
| `log`        | 0.4.17  | Logging facade                       |
| `env_logger` | 0.11.5  | Log output to stderr                 |
| `anyhow`     | 1       | Application error handling           |
| `reqwest`    | 0.12    | HTTP client for GitHub API           |
| `serde`      | 1       | Serialization/deserialization        |
| `tokio`      | 1       | Async runtime                        |

---

## Code Style Guidelines

### Formatting

- Use `cargo fmt` (rustfmt) with default settings -- no `.rustfmt.toml` overrides.
- Run `cargo fmt` before committing.

### Linting

- Use `cargo clippy` with default settings -- no `.clippy.toml` overrides.
- Treat all clippy warnings as errors: `cargo clippy -- -D warnings`.
- Fix clippy suggestions rather than suppressing them with `#[allow(...)]`
  unless there is a documented reason.

### Imports

- Use explicit imports (`use crate::module::Item;`), not glob imports (`use crate::module::*;`).
- Group imports in this order, separated by blank lines:
  1. Standard library (`std::`)
  2. External crates (`clap::`, `log::`, etc.)
  3. Internal crate modules (`crate::`, `super::`)
- Use nested imports to reduce line count: `use clap::{Parser, Subcommand};`

### Types and Data

- Prefer strong typing over primitive types -- use newtypes or enums where it
  adds clarity.
- Use `#[derive(...)]` for standard trait implementations (`Debug`, `Clone`,
  `PartialEq`, etc.) rather than manual impls unless custom behavior is needed.
- Use Rust edition 2024 features where appropriate.

### Naming Conventions

Follow standard Rust conventions:

| Item                     | Convention       | Example              |
|--------------------------|------------------|----------------------|
| Crates                   | `snake_case`     | `prism_cli`          |
| Modules                  | `snake_case`     | `pr_review`          |
| Types / Traits / Enums   | `PascalCase`     | `ReviewCommand`      |
| Functions / Methods      | `snake_case`     | `review_pr`          |
| Constants                | `SCREAMING_SNAKE` | `MAX_RETRIES`       |
| Enum variants            | `PascalCase`     | `Command::Review`    |
| Local variables          | `snake_case`     | `pr_number`          |
| Type parameters          | Single uppercase | `T`, `E`             |

### Error Handling

- Use `Result<T, E>` for fallible operations -- do not panic in library code.
- Prefer `?` for error propagation over explicit `match` on `Result`.
- Use `anyhow` or `thiserror` for application vs library error types when
  the project grows beyond a single file.
- Reserve `unwrap()` / `expect()` for cases where failure is truly impossible
  and add a message to `expect()` explaining why.
- Log errors at the appropriate level (`log::error!`, `log::warn!`) before
  returning them when context would otherwise be lost.

### Logging

- Use the `log` crate macros: `log::info!`, `log::warn!`, `log::error!`,
  `log::debug!`, `log::trace!`.
- Default log level is `info` (set via `env_logger` in `main`).
- Override at runtime with `RUST_LOG=debug cargo run`.

### CLI Design

- Use `clap` derive macros (`#[derive(Parser)]`, `#[derive(Subcommand)]`) for
  all CLI argument definitions.
- Add doc comments (`///`) on struct fields and enum variants -- clap uses
  these as help text.

### Testing

- Place unit tests in the same file as the code they test, inside a
  `#[cfg(test)] mod tests { ... }` block.
- Place integration tests in a top-level `tests/` directory.
- Name test functions descriptively: `test_review_valid_pr_number`.
- Use `assert_eq!`, `assert_ne!`, `assert!` with descriptive messages.

### Documentation

- Add `///` doc comments on all public items.
- Use `//` line comments for implementation notes; avoid `/* */` block comments.

### Git Practices

- Do not commit `target/`, `*.pdb`, `*.rs.bk`, or `mutants.out*/` (covered
  by `.gitignore`).
- Write concise commit messages focused on "why" not "what".

---

## Running the Project

```bash
# Quick start
cd prism-cli
cargo run -- review 42

# Review a GitHub PR URL
cargo run -- review https://github.com/owner/repo/pull/42

# Review a commit
cargo run -- review a1b2c3d

# With debug logging
RUST_LOG=debug cargo run -- review 42
```

---

## CI / Linting Checklist

No CI pipeline exists yet. Before pushing, verify locally:

```bash
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
cargo build
```
