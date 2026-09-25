//! Generates game data and fingerprints deterministic dungeon layouts.

include!("src/fnv.rs");
include!("../tools/cargo-build-data.rs");

/// Excluded so a test-only edit does not reload the whole fleet.
const SKIP: &str = "tests.rs";

fn main() {
    generate_data_json().expect("failed to generate data JSON from CSV");
    let mut inputs = vec![
        std::path::PathBuf::from("../data-src/dungeons.csv"),
        std::path::PathBuf::from("../data-src/monsters.csv"),
    ];
    println!("cargo:rerun-if-changed=src/dungeon");
    let dir = std::fs::read_dir("src/dungeon").expect("shared/src/dungeon");
    inputs.extend(
        dir.map(|e| e.expect("dungeon dir entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .filter(|p| p.file_name().is_some_and(|n| n != SKIP)),
    );
    inputs.sort();

    let mut hash = FNV_OFFSET;
    for path in &inputs {
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("layout input {path:?}: {e}"));
        // Normalize separators so Windows and Unix builds agree.
        hash = fnv1a64(hash, path.to_string_lossy().replace('\\', "/").bytes());
        // CR dropped: the Windows agent-client build must hash the same source.
        hash = fnv1a64(hash, bytes.into_iter().filter(|b| *b != b'\r'));
    }
    println!("cargo:rustc-env=LAYOUT_VERSION={hash:016x}");
}
