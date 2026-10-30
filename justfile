default:
    just --list

test:
    cargo test

dev:
    cargo run

lint:
    cargo clippy

fmt:
    cargo fmt
