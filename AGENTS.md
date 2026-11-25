# AGENTS.md

nix-relay is a distributed build pool for Nix. Workers dial out to a relay, `nixr` sends builds to the relay, and the relay keeps a binary cache of the outputs.

## Priorities

1. Correctness and reliability, including reconnects, workers that drop mid-build, and partial transfers.
2. Performance. Most of a build's wall time should be spent building, not evaluating or moving store paths.

## Scope

- Don't add flags, subcommands, config files, or just recipes the task didn't call for. A command earns its place only if someone would actually run it.
- Fix clear bugs and perf problems without asking. If a behavior might be intentional, ask before changing it.
- Don't restrict a user's own choices to protect them from themselves.
- Linux only for now.
- No legacy code. When you replace something, delete the old path in the same change instead of keeping it behind a fallback or compat shim. Migrations and version bumps are fine when the data or protocol actually changes.

## Code

- Break up god files. Shared logic goes in a shared module, not copied into each caller.
- Types shared by the relay, worker, and client go in `nix-relay-protocol`.
- Fix warnings instead of silencing them.
- Delete tests that don't test anything real.
- Unit tests for `foo.rs` go in `foo/tests.rs`, declared as `#[cfg(test)] mod tests;`. Never inline a test module. Split large ones into `foo/tests/*.rs`.
- Comments only for code so unusual that a reader can't infer why it must be written that way. Never comment to explain a change.
- Rust dependencies go in `[workspace.dependencies]` with a full version triple.

## Checks

- `just check` and `nix flake check` are expensive. Run `just generate-cargo-nix` after changing dependencies and `just format` before finishing. Run narrow checks for what you touched: `just lint`, `cargo nextest run -p <crate> --all-features`, or the matching recipe in the `justfile`.
- Fix flaky tests, don't rerun them.
- Profiles live in `Cargo.toml`. Rustflags live in `.cargo/config.toml`.
