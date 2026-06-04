{
  description = "Croma project development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rust = pkgs.rust-bin.stable."1.96.0".default.override {
          extensions = [
            "clippy"
            "rust-analyzer"
            "rust-src"
            "rustfmt"
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.cargo-nextest
            pkgs.git
            pkgs.just
            pkgs.openssl
            pkgs.pkg-config
            pkgs.python312
            pkgs.taplo
            pkgs.uv
          ];

          RUST_BACKTRACE = "1";

          shellHook = ''
            echo "croma dev shell: Rust $(rustc --version)"
          '';
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
