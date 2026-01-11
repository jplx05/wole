# Repository Guidelines

## Project Structure & Module Organization
- `src/`: Rust library and CLI implementation. Key areas include `src/cli.rs` (CLI orchestration), `src/cli/commands/` (per-command handlers), `src/cli/interactive_menu.rs` (interactive menu), `src/cleaner.rs` (cleaning orchestration), `src/cleaner/` (deletion features), `src/optimize.rs` (optimization orchestration), `src/optimize/` (optimization features), `src/categories/` (scan/clean per category), and `src/tui/` (terminal UI).
- `tests/`: Integration tests.
- `Cargo.toml`: Dependencies, features, and build settings.
- `build/`, `artifacts/`, `target/`: Build outputs and generated artifacts (do not edit by hand).

## Build, Test, and Development Commands
- `cargo build`: Compile the project.
- `cargo run -- --help`: Run the CLI and show options.
- `cargo test`: Run unit and integration tests.
- `cargo fmt`: Format code with rustfmt (standard Rust style).
- `cargo clippy`: Lint for common Rust issues.

## Coding Style & Naming Conventions
- Use Rust 2021 edition conventions with `rustfmt` formatting.
- Naming: `snake_case` for functions/variables/modules, `CamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for constants.
- Prefer small, focused helpers; reuse deletion helpers in `src/cleaner.rs` to keep behavior consistent.

## Testing Guidelines
- Unit tests live alongside code in `src/` using `#[cfg(test)]`.
- Integration tests live in `tests/`.
- Keep tests deterministic; avoid relying on system temp contents.
- Run `cargo test` before changes that affect deletion or scanning.

## Commit & Pull Request Guidelines
- Commit messages are descriptive, sentence case, and action-oriented (e.g., “Add safeguards to prevent system freeze…”).
- PRs should include:
  - A short summary of changes.
  - Testing performed (`cargo test`, manual validation, etc.).
  - Notes on Windows-specific behavior if applicable.

## Security & Configuration Tips
- System paths are intentionally protected from deletion. If adding new delete paths, ensure they are screened via existing safety checks.
- Temp and cache deletes should route through shared helpers to avoid false failures and to respect locks.
