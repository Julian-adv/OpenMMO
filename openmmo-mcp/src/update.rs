//! Updates from the agent-client releases, which carry an openmmo-mcp binary
//! per platform. Newer builds are downloaded into the cache in the background
//! and the installed copy hands over to the newest one on the next start, so
//! the folder Claude Desktop installed the extension into is never written to.

use agent_client::update::{download, fetch_releases, parse_version, TAG_PREFIX};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Set on the handed-over copy so it serves instead of looking again.
const HANDED_OVER: &str = "OPENMMO_MCP_HANDED_OVER";
/// Unauthenticated GitHub allows 60 API calls an hour per address.
const CHECK_INTERVAL: Duration = Duration::from_secs(3600);
const STAMP: &str = "last-check";

type Version = (u64, u64, u64);

fn own_version() -> Version {
    parse_version(agent_client::VERSION).expect("agent-client version is x.y.z")
}

/// `x86_64-pc-windows-msvc.exe` or `x86_64-unknown-linux-gnu`: the release
/// asset is `openmmo-mcp-v<version>-<this>`.
fn platform_suffix() -> String {
    let arch = std::env::consts::ARCH;
    if cfg!(windows) {
        format!("{arch}-pc-windows-msvc.exe")
    } else {
        format!("{arch}-unknown-linux-gnu")
    }
}

fn asset_name(version: &str) -> String {
    format!("openmmo-mcp-v{version}-{}", platform_suffix())
}

fn bin_dir() -> PathBuf {
    crate::game::cache_dir().join("bin")
}

/// The newest downloaded build newer than this one.
fn newest_cached(dir: &Path) -> Option<PathBuf> {
    let own = own_version();
    let suffix = format!("-{}", platform_suffix());
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let version =
                parse_version(name.strip_prefix("openmmo-mcp-v")?.strip_suffix(&suffix)?)?;
            (version > own).then_some((version, path))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, path)| path)
}

/// Downloads the newest release newer than this build, if any, and returns
/// its tag. Older downloads are removed.
pub async fn download_newer() -> anyhow::Result<Option<String>> {
    let dir = bin_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(STAMP), b"")?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("openmmo-mcp/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let own = own_version();
    let releases = fetch_releases(&client).await?;
    let newer = releases.iter().find_map(|release| {
        let version = release.tag_name.strip_prefix(TAG_PREFIX)?;
        if parse_version(version)? <= own {
            return None;
        }
        let name = asset_name(version);
        let asset = release.assets.iter().find(|a| a.name == name)?;
        Some((release.tag_name.clone(), asset))
    });
    let Some((tag, asset)) = newer else {
        return Ok(None);
    };
    let dest = dir.join(&asset.name);
    if !dest.is_file() {
        let partial = dir.join(format!("{}.part", asset.name));
        std::fs::write(&partial, download(&client, asset).await?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&partial, &dest)?;
    }
    for entry in std::fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path != dest && path.file_name().is_some_and(|n| n != STAMP) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(Some(tag))
}

fn check_due(dir: &Path) -> bool {
    std::fs::metadata(dir.join(STAMP))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.elapsed().ok())
        .is_none_or(|age| age >= CHECK_INTERVAL)
}

/// Before serving: run the newest downloaded build in this process's place,
/// or else look for a newer release in the background for the next start.
/// `Some(exit code)` when another build served.
pub fn hand_over() -> Option<i32> {
    if std::env::var_os(HANDED_OVER).is_some() {
        return None;
    }
    let dir = bin_dir();
    let Some(newer) = newest_cached(&dir) else {
        if check_due(&dir) {
            tokio::spawn(async {
                if let Err(e) = download_newer().await {
                    tracing::warn!("Update check skipped: {e:#}");
                }
            });
        }
        return None;
    };
    tracing::info!("Handing over to {}", newer.display());
    let mut command = std::process::Command::new(&newer);
    command.env(HANDED_OVER, "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = command.exec();
        tracing::warn!("Running {} failed: {e}", newer.display());
        None
    }
    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => Some(status.code().unwrap_or(1)),
        Err(e) => {
            tracing::warn!("Running {} failed: {e}", newer.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_newer_cached_builds_for_this_platform_count() {
        let dir = std::env::temp_dir().join(format!("openmmo-mcp-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b, c) = own_version();
        for version in [
            format!("{a}.{b}.{c}"),
            format!("{a}.{}.0", b + 1),
            format!("{}.0.0", a + 1),
        ] {
            std::fs::write(dir.join(asset_name(&version)), b"").unwrap();
        }
        std::fs::write(dir.join(format!("openmmo-mcp-v{}.0.0-other", a + 2)), b"").unwrap();
        let newest = newest_cached(&dir).unwrap();
        assert_eq!(
            newest.file_name().unwrap(),
            asset_name(&format!("{}.0.0", a + 1)).as_str()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
