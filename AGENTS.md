# AGENTS.md - Coding Guidelines for chaindash

## Project Overview

**chaindash** is a terminal UI dashboard for PlatON blockchain nodes, built with Rust.

- Language: Rust (Edition 2021)
- Build Tool: Cargo
- UI Framework: tui-rs with crossterm backend
- Async Runtime: Tokio

## Build Commands

```bash
# Build the project
cargo build

# Build optimized release
cargo build --release

# Check without building
cargo check

# Run the application
cargo run -- --url mainnet@wss://openapi2.platon.network/rpc

# Run with debug logging
cargo run -- --url $URL --debug
```

## Test Commands

```bash
# Run all tests
cargo test

# Run a specific test by name
cargo test test_name

# Run tests in a specific module
cargo test module_name::

# Run tests with output
cargo test -- --nocapture

# Run tests matching a pattern
cargo test pattern
```

## Lint & Format

```bash
# Format code (uses rustfmt.toml)
cargo fmt

# Check formatting without modifying
cargo fmt -- --check

# Run Clippy lints
cargo clippy

# Run Clippy with all features
cargo clippy --all-features
```

## Code Style Guidelines

### Naming Conventions
- **Types/Structs/Enums**: PascalCase (`App`, `Opts`, `NodeWidget`)
- **Functions/Variables**: snake_case (`setup_app`, `handle_tab_key`)
- **Constants**: UPPER_SNAKE_CASE (`PROGRAM_NAME`)
- **Modules**: snake_case (`widgets`, `disk_list`)

### Imports
Imports are organized in three groups (enforced by rustfmt):
1. Standard library (`std::`, `core::`)
2. External crates (`tokio::`, `clap::`, `tui::`)
3. Internal modules (`crate::`, `super::`)

Use vertical layout for imports:
```rust
use std::{
    fs,
    io::Write,
    path::Path,
};
use clap::Parser;
use crate::widgets::*;
```

### Formatting Rules (from rustfmt.toml)
- Indent: 4 spaces (no tabs)
- Max width: 100 characters
- Newline: Unix style (`\n`)
- Trailing commas: Vertical style
- Brace style: Same line where
- Import granularity: Crate-level

### Error Handling
- Use `?` operator for propagation
- Prefer `Result<T, Box<dyn std::error::Error>>` for main errors
- Use `unwrap()` only in setup/unrecoverable situations
- Log errors via `log` crate before panicking

### Platform-Specific Code
Use conditional compilation for platform-specific features:
```rust
#[cfg(target_family = "unix")]
pub struct SystemSummaryWidget;
```

### Async Patterns
- Main function uses `#[tokio::main]`
- Spawn collectors with `tokio::spawn()`
- Use `crossbeam_channel` for sync/async communication

### Code Organization
- Keep functions focused (single responsibility)
- Prefer early returns to reduce nesting
- Document public APIs with `///` comments
- Group related constants at module level

## File Structure

```
src/
├── main.rs           # Entry point, event loop
├── app.rs            # App state and widget management
├── opts.rs           # CLI argument definitions
├── draw.rs           # Rendering logic
├── update.rs         # Widget update logic
├── collect/          # Data collection module
│   ├── mod.rs
│   ├── collector.rs
│   └── types.rs
└── widgets/          # UI widgets
    ├── mod.rs
    ├── block.rs
    ├── node.rs
    └── ...
```

## Dependencies Notes

Key external crates:
- `clap`: CLI parsing with derive macros
- `tokio`: Async runtime (v0.2 - older version)
- `tui`: Terminal UI framework (v0.9)
- `crossterm`: Cross-platform terminal control
- `web3`: Custom fork for PlatON support
- `sysinfo`: System statistics collection

## Running in Docker

```bash
./run.sh mainnet@wss://openapi2.platon.network/rpc
```
