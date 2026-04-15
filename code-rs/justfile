set working-directory := "code-rs"
set positional-arguments

# Display help
help:
    just -l

# `code`
alias c := code
code *args:
    cargo run --bin code -- "$@"

# `code exec`
exec *args:
    cargo run --bin code -- exec "$@"

# `code tui`
tui *args:
    cargo run --bin code -- tui "$@"

# Run the CLI version of the file-search crate.
file-search *args:
    cargo run --bin code-file-search -- "$@"

# format code
fmt:
    cargo fmt -- --config imports_granularity=Item

fix *args:
    cargo clippy --fix --all-features --tests --allow-dirty "$@"

clippy:
    cargo clippy --all-features --tests "$@"

install:
    rustup show active-toolchain
    cargo fetch

# Run `cargo nextest` since it's faster than `cargo test`, though including
# --no-fail-fast is important to ensure all tests are run.
#
# Run `cargo install cargo-nextest` if you don't have it installed.
test:
    cargo nextest run --no-fail-fast

# Run the MCP server
mcp-server-run *args:
    cargo run -p code-mcp-server -- "$@"
