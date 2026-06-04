set dotenv-load := false

fmt:
    cargo fmt --all --check

test:
    cargo test --workspace

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

check: fmt test clippy

xml-example:
    cargo run -p croma-cli -- xml examples/basic.abc
