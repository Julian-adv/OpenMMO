use onlinerpg_terrain::io::TerrainIO;
use std::{io, path::PathBuf};

fn main() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let dir = args.next().map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Usage: terrain-manifests TERRAIN_DIR",
        )
    })?;
    if args.next().is_some() || !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Expected one existing terrain directory",
        ));
    }
    let terrain = TerrainIO::new(dir);
    let count = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(terrain.prepare_manifests())?;
    println!(
        "Prepared {count} terrain manifests in {}",
        terrain.manifest_dir().display()
    );
    Ok(())
}
