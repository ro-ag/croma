# Development Environment

Use Nix flakes for project-local tooling. This gives each repository its own
compiler, formatter, language server, Python tooling, and native build inputs
without relying on global Homebrew state.

## Recommended Stack

1. Determinate Nix
   - Installs Nix cleanly on macOS.
   - Enables flakes by default.
   - Provides a practical uninstall path.

2. direnv plus nix-direnv
   - Automatically enters the project shell on `cd`.
   - Keeps tooling scoped to this repository.

3. Optional nix-darwin
   - Use only for machine-level macOS configuration.
   - Not required for this repository.

4. Optional devenv.sh
   - Useful later if the project needs managed services or task orchestration.
   - The current `flake.nix` is enough for the initial Rust/Python stack.

## Bootstrap

Install Nix on macOS:

```sh
curl -fsSL https://install.determinate.systems/nix | sh -s -- install
```

Install direnv and nix-direnv:

```sh
brew install direnv nix-direnv
```

Add the direnv shell hook to `~/.zshrc`:

```sh
eval "$(direnv hook zsh)"
```

Enter the repository:

```sh
cd /Users/rodox/dev/rs/croma
direnv allow
```

Manual entry without direnv:

```sh
nix develop
```

## Project Toolchain

The flake currently provides:

- Rust 1.96.0
- rustfmt
- clippy
- rust-analyzer
- cargo-nextest
- Python 3.12
- uv
- just
- taplo
- pkg-config and OpenSSL

## Python Tooling

Use `uv` for Python environments and Python command execution in this project.
This matters most for corpus tooling and music21-based MusicXML comparison.

Preferred patterns:

```sh
uv venv
uv pip install music21
uv run python tools/music21_compare.py --help
```

Do not rely on a global Homebrew or system Python for project validation. Keep
local Python caches, virtual environments, and generated comparison reports out
of tracked source paths.

Private research notes and generated scratch files belong in `docs/untracked/`.
That directory is intentionally ignored by Git.
