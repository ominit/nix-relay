help:
    @just --list

update-rust-deps:
    cargo upgrade
    cargo update
    just generate-cargo-nix

update-rust-deps-full:
    cargo upgrade --incompatible
    cargo update
    just generate-cargo-nix

generate-cargo-nix:
    crate2nix generate

test:
    cargo nextest run --all-features --no-tests=pass

format:
    nix fmt -- -c

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

lint-fix:
    cargo clippy --workspace --all-targets --all-features --fix -- -D warnings

check: generate-cargo-nix format lint test
