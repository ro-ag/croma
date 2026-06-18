# Development Environment

Croma supports two interchangeable development environments. Both provide the
same pinned toolchain (Rust 1.96.0 + `uv`-managed Python) so corpus tooling and
validation behave identically:

1. **Linux cloud sandbox** (Claude Code on the web, CI, or any ephemeral Linux
   container). The toolchain is provisioned directly with `rustup` and `uv`.
2. **Local Nix flake** (any OS, including macOS). A `flake.nix` pins the same
   tools for reproducible local work.

The Rust version is pinned by `rust-toolchain.toml` at the repository root
(`channel = "1.96.0"`, with `clippy` and `rustfmt`). Because `cargo`/`rustc`
honor that file automatically, you never need to reference an absolute toolchain
path — plain `cargo build` selects 1.96.0 regardless of host architecture
(`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, etc.).

## Linux Cloud Sandbox

This is the default environment for Claude Code on the web. The container is
ephemeral: the repo is cloned fresh on start and reclaimed on inactivity, so
anything worth keeping must be committed and pushed.

### Bootstrap

`rustup` and `uv` are typically pre-installed. If a fresh container is missing
either, install them:

```sh
# Rust toolchain manager (skip if rustup already present)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"

# uv (skip if already present)
curl -LsSf https://astral.sh/uv/install.sh | sh
```

Provision the pinned toolchain and Python environment from the repository root:

```sh
# Installs Rust 1.96.0 + clippy + rustfmt as declared in rust-toolchain.toml.
rustup show

# Resolves and installs Python deps from pyproject.toml / uv.lock.
uv sync
```

### Build And Validate

```sh
cargo build -p croma-cli          # produces target/debug/croma
cargo test --workspace            # Rust unit + integration tests
uv run pytest                     # Python tooling tests, if present
```

No absolute paths are required; run everything from the repository root.

## Local Nix Flake

Use Nix flakes for project-local tooling on a developer machine. This gives the
repository its own compiler, formatter, language server, Python tooling, and
native build inputs without relying on global package-manager state.

### Recommended Stack

1. Determinate Nix — installs Nix cleanly (macOS and Linux) with flakes enabled.
2. direnv plus nix-direnv — automatically enters the project shell on `cd`.
3. Optional nix-darwin — only for machine-level macOS configuration.
4. Optional devenv.sh — useful later if the project needs managed services.

### Bootstrap

Install Nix:

```sh
curl -fsSL https://install.determinate.systems/nix | sh -s -- install
```

Install direnv and nix-direnv (use your platform's package manager, e.g.
`brew install direnv nix-direnv` on macOS or `nix profile install` on Linux),
then add the direnv shell hook to your shell rc (`~/.zshrc` or `~/.bashrc`):

```sh
eval "$(direnv hook zsh)"   # or: eval "$(direnv hook bash)"
```

Enter the repository (`.envrc` contains `use flake`):

```sh
cd /path/to/croma
direnv allow
```

Manual entry without direnv:

```sh
nix develop
```

### Flake Toolchain

`flake.nix` provides:

- Rust 1.96.0 (clippy, rustfmt, rust-analyzer, rust-src)
- cargo-nextest
- Python 3.12 and uv
- just, taplo
- pkg-config and OpenSSL

## Python Tooling

Use `uv` for Python environments and Python command execution in this project in
**both** environments. This matters most for corpus tooling and music21-based
MusicXML comparison.

Preferred patterns:

```sh
uv sync                                       # install pinned deps (uv.lock)
uv run python tools/music21_compare.py --help
uv run pytest
```

Do not rely on a global or system Python for project validation. Keep local
Python caches, virtual environments, and generated comparison reports out of
tracked source paths.

Private research notes and generated scratch files belong in `docs/untracked/`.
That directory is intentionally ignored by Git.
