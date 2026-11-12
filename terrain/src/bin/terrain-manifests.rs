use onlinerpg_terrain::io::TerrainIO;
use std::{io, path::PathBuf};

const USAGE: &str = "Usage: terrain-manifests TERRAIN_DIR [X,Z ...]";

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn parse_tile(arg: &str) -> Option<(i32, i32)> {
    let (x, z) = arg.split_once(',')?;
    Some((x.parse().ok()?, z.parse().ok()?))
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| invalid(USAGE))?;
    if !dir.is_dir() {
        return Err(invalid("Expected an existing terrain directory"));
    }
    let tiles = args
        .map(|arg| parse_tile(&arg).ok_or_else(|| invalid(USAGE)))
        .collect::<io::Result<Vec<_>>>()?;
    let terrain = TerrainIO::new(dir);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // Listed tiles are rebuilt alone, so a patch applied during downtime skips the full pass.
    let count = if tiles.is_empty() {
        runtime.block_on(terrain.prepare_manifests())?
    } else {
        for &(x, z) in &tiles {
            runtime.block_on(terrain.rebuild_manifest(x, z))?;
        }
        tiles.len()
    };
    println!(
        "Prepared {count} terrain manifests in {}",
        terrain.manifest_dir().display()
    );
    Ok(())
}
