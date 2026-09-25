use onlinerpg_terrain::io::TerrainIO;
use std::{io, path::PathBuf};

fn main() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let dir = args.next().map(PathBuf::from).filter(|dir| dir.is_dir());
    let Some(dir) = dir.filter(|_| args.next().is_none()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Usage: terrain-grass-migrate TERRAIN_DIR",
        ));
    };
    let stats = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(TerrainIO::new(dir).migrate_grass_files())?;
    println!(
        "Converted {} grass files: {} -> {} bytes",
        stats.converted, stats.before_bytes, stats.after_bytes
    );
    Ok(())
}
