//! croma ABC — Zed extension.
//!
//! Registers the `ABC` language with Zed and launches the `croma-lsp` stdio
//! language server. The server binary is resolved with a **download-or-PATH**
//! strategy (see [`asset_name`] and `language_server_command`):
//!
//! 1. `worktree.which("croma-lsp")` — works today via
//!    `cargo install --path crates/croma-lsp`.
//! 2. A GitHub release auto-download for the current platform (lights up once
//!    the release epic cuts binaries).
//! 3. Otherwise a clear error pointing at `cargo install`.
//!
//! The platform → release-asset-name mapping is factored into the pure
//! [`asset_name`] function so it is unit-testable on the host target,
//! independent of the wasm `zed` runtime.

use zed_extension_api::{Architecture, Os};

/// Name of the language server binary, as installed by
/// `cargo install --path crates/croma-lsp` and (eventually) shipped in releases.
const LSP_BINARY: &str = "croma-lsp";

/// Maps a target platform to the expected `croma-lsp` release-asset file name.
///
/// Pure and host-testable: it depends only on the `Os`/`Architecture` value
/// types from `zed_extension_api`, not on the wasm-only `zed` host runtime.
///
/// The scheme is `croma-lsp-<os>-<arch>[.exe]`:
/// `croma-lsp-macos-aarch64`, `croma-lsp-linux-x86_64`,
/// `croma-lsp-windows-x86_64.exe`, etc.
///
// TODO(epic-C): reconcile this scheme with the names the release workflow uploads.
pub fn asset_name(platform: (Os, Architecture)) -> String {
    let (os, arch) = platform;
    let (os, suffix) = match os {
        Os::Mac => ("macos", ""),
        Os::Linux => ("linux", ""),
        Os::Windows => ("windows", ".exe"),
    };
    let arch = match arch {
        Architecture::Aarch64 => "aarch64",
        Architecture::X86 => "x86",
        Architecture::X8664 => "x86_64",
    };
    format!("{LSP_BINARY}-{os}-{arch}{suffix}")
}

#[cfg(target_arch = "wasm32")]
mod wasm_ext {
    use super::{asset_name, LSP_BINARY};
    use zed_extension_api::{
        self as zed, DownloadedFileType, GithubReleaseOptions, LanguageServerId, Result,
    };

    /// GitHub `owner/repo` slug used for the release auto-download.
    const GITHUB_REPO: &str = "ro-ag/croma";

    /// Directory (relative to the extension work dir) into which a downloaded
    /// server binary is cached, keyed by release version.
    fn version_dir(version: &str) -> String {
        format!("croma-lsp-{version}")
    }

    pub struct CromaAbcExtension;

    impl zed::Extension for CromaAbcExtension {
        fn new() -> Self {
            Self
        }

        fn language_server_command(
            &mut self,
            _language_server_id: &LanguageServerId,
            worktree: &zed::Worktree,
        ) -> Result<zed::Command> {
            // 1) Prefer a `croma-lsp` already on PATH / in the worktree env.
            //    This is the path that works *today* via
            //    `cargo install --path crates/croma-lsp`.
            if let Some(path) = worktree.which(LSP_BINARY) {
                return Ok(zed::Command {
                    command: path,
                    args: Vec::new(),
                    env: worktree.shell_env(),
                });
            }

            // 2) Otherwise try a GitHub release auto-download for this platform.
            //    Functional once the release epic (C) publishes binaries; until
            //    then `latest_github_release` simply errors and we fall through.
            if let Ok(command) = download_from_release() {
                return Ok(command);
            }

            // 3) No server available — actionable error.
            Err(format!(
                "`{LSP_BINARY}` not found. Install it with \
                 `cargo install --path crates/croma-lsp` (from a croma checkout), \
                 or `cargo install croma-lsp` once it is published to crates.io. \
                 Automatic download from GitHub releases will work once \
                 release binaries are published."
            ))
        }
    }

    /// Attempts to download the latest `croma-lsp` release binary for the current
    /// platform and returns a [`zed::Command`] pointing at the cached path.
    fn download_from_release() -> Result<zed::Command> {
        let platform = zed::current_platform();
        let asset_name = asset_name(platform);

        let release = zed::latest_github_release(
            GITHUB_REPO,
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                format!(
                    "no `{asset_name}` asset in croma release {}",
                    release.version
                )
            })?;

        let dir = version_dir(&release.version);
        let bin_path = format!("{dir}/{LSP_BINARY}");

        // Re-download only if not already cached for this version.
        if !std::path::Path::new(&bin_path).exists() {
            zed::download_file(
                &asset.download_url,
                &bin_path,
                DownloadedFileType::Uncompressed,
            )?;
            zed::make_file_executable(&bin_path)?;
        }

        Ok(zed::Command {
            command: bin_path,
            args: Vec::new(),
            env: Vec::new(),
        })
    }

    zed::register_extension!(CromaAbcExtension);
}

#[cfg(test)]
mod tests {
    use super::*;
    use zed_extension_api::{Architecture, Os};

    #[test]
    fn macos_aarch64_asset() {
        assert_eq!(
            asset_name((Os::Mac, Architecture::Aarch64)),
            "croma-lsp-macos-aarch64"
        );
    }

    #[test]
    fn macos_x86_64_asset() {
        assert_eq!(
            asset_name((Os::Mac, Architecture::X8664)),
            "croma-lsp-macos-x86_64"
        );
    }

    #[test]
    fn linux_aarch64_asset() {
        assert_eq!(
            asset_name((Os::Linux, Architecture::Aarch64)),
            "croma-lsp-linux-aarch64"
        );
    }

    #[test]
    fn linux_x86_64_asset() {
        assert_eq!(
            asset_name((Os::Linux, Architecture::X8664)),
            "croma-lsp-linux-x86_64"
        );
    }

    #[test]
    fn windows_x86_64_gets_exe_suffix() {
        assert_eq!(
            asset_name((Os::Windows, Architecture::X8664)),
            "croma-lsp-windows-x86_64.exe"
        );
    }

    #[test]
    fn windows_aarch64_gets_exe_suffix() {
        assert_eq!(
            asset_name((Os::Windows, Architecture::Aarch64)),
            "croma-lsp-windows-aarch64.exe"
        );
    }

    #[test]
    fn x86_32bit_arch_maps_to_x86() {
        assert_eq!(
            asset_name((Os::Linux, Architecture::X86)),
            "croma-lsp-linux-x86"
        );
    }

    #[test]
    fn only_windows_gets_exe_suffix() {
        // Sanity: no non-windows asset name should end in `.exe`.
        for os in [Os::Mac, Os::Linux] {
            for arch in [
                Architecture::Aarch64,
                Architecture::X86,
                Architecture::X8664,
            ] {
                let name = asset_name((os, arch));
                assert!(
                    !name.ends_with(".exe"),
                    "{name} should not have an .exe suffix"
                );
                assert!(name.starts_with("croma-lsp-"), "{name} bad prefix");
            }
        }
    }
}
