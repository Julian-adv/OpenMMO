use onlinerpg_shared::{
    dungeon,
    furniture::{self, FurniturePlacement},
    housing::HouseData,
    pathfinding::{self, PassabilityCache, PathResult, PathWaypoint},
};
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::{error::Error, fs, path::Path};

pub const GROUPS: [&str; 7] = [
    "open",
    "village",
    "dungeon",
    "stairs",
    "closed_doors",
    "unreachable",
    "long_route",
];

#[derive(Clone, Serialize)]
pub struct Query {
    pub group: usize,
    pub map: String,
    pub from: PathWaypoint,
    pub to: PathWaypoint,
}

pub struct Fixtures {
    pub open: PassabilityCache,
    pub closed: PassabilityCache,
    pub queries: [Vec<Query>; 7],
    pub manifest: serde_json::Value,
}

impl Fixtures {
    pub fn run(&self, query: &Query) -> PathResult {
        let cache = if query.group == 4 {
            &self.closed
        } else {
            &self.open
        };
        pathfinding::find_and_smooth_path(
            query.from.x,
            query.from.z,
            query.from.floor,
            query.to.x,
            query.to.z,
            query.to.floor,
            cache,
            dungeon::path_max_nodes(query.from.floor, query.to.floor),
        )
    }
}

fn json_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(json_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn fingerprint(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
}

fn point(x: f32, z: f32, floor: u8) -> PathWaypoint {
    PathWaypoint { x, z, floor }
}

fn dungeon_point(def: &dungeon::DungeonEntranceDef, depth: u8, cell: (i32, i32)) -> PathWaypoint {
    let p = dungeon::cell_center(&def.position(), depth, cell);
    point(p.x, p.z, dungeon::passability_floor_for_depth(depth))
}

fn clear(cache: &PassabilityCache, p: &PathWaypoint) -> bool {
    !pathfinding::is_circle_blocked_on_floor(cache, p.x, p.z, 0.3, p.floor, None)
        && !pathfinding::is_cell_sealed(cache, p.x, p.z, p.floor, None)
}

#[derive(Deserialize)]
struct Objects {
    #[serde(default)]
    placements: Vec<FurniturePlacement>,
}

pub fn load(root: &Path, seed: u64) -> Result<Fixtures, Box<dyn Error>> {
    let mut cache = PassabilityCache::new();
    let mut hash = 0xcbf29ce484222325;
    let mut queries: [Vec<Query>; 7] = std::array::from_fn(|_| Vec::new());
    let house_files = json_files(&root.join("housing"))?;
    if house_files.is_empty() {
        return Err("no houses found in data root".into());
    }
    for path in &house_files {
        let bytes = fs::read(path)?;
        fingerprint(&mut hash, &bytes);
        let house: HouseData = serde_json::from_slice(&bytes)?;
        let rp = pathfinding::build_runtime_passability(&house);
        let corners = [
            point(rp.min_x - 3.5, rp.min_z - 3.5, 0),
            point(rp.max_x + 3.5, rp.min_z - 3.5, 0),
            point(rp.max_x + 3.5, rp.max_z + 3.5, 0),
            point(rp.min_x - 3.5, rp.max_z + 3.5, 0),
        ];
        for i in 0..4 {
            queries[1].push(Query {
                group: 1,
                map: house.id.clone(),
                from: corners[i].clone(),
                to: corners[(i + 2) % 4].clone(),
            });
        }
        cache.insert(house.id.clone(), rp);
        pathfinding::apply_door_overlays(&mut cache, &house);
    }
    let object_files = json_files(&root.join("terrain/objects"))?;
    let mut furniture_regions = 0;
    for path in &object_files {
        let bytes = fs::read(path)?;
        fingerprint(&mut hash, &bytes);
        let objects: Objects = serde_json::from_slice(&bytes)?;
        if let Some(rp) = furniture::build_furniture_passability_for_placements(&objects.placements)
        {
            cache.insert(
                format!("furniture:{}", path.file_stem().unwrap().to_string_lossy()),
                rp,
            );
            furniture_regions += 1;
        }
    }
    let layouts: Vec<_> = dungeon::entrances()
        .iter()
        .map(|def| (def, dungeon::generate_dungeon_for(&def.id)))
        .collect();
    for (def, floors) in &layouts {
        cache.insert(
            dungeon::dungeon_cache_key(&def.id),
            dungeon::dungeon_passability(&def.position(), floors),
        );
    }
    let closed = cache.clone();
    for (def, floors) in &layouts {
        for floor in floors {
            dungeon::set_floor_cells(
                &mut cache,
                &def.id,
                floor.depth,
                dungeon::floor_passability_cells_full(floor, &[], &[]),
            );
        }
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    for _ in 0..64 {
        let x = rng.gen_range(-1550.0..-1300.0);
        let z = rng.gen_range(4450.0..4500.0);
        queries[0].push(Query {
            group: 0,
            map: "aldermark_outskirts".into(),
            from: point(x, z, 0),
            to: point(
                x + rng.gen_range(5.0..50.0),
                z + rng.gen_range(-20.0..20.0),
                0,
            ),
        });
    }
    for (def, floors) in &layouts {
        let first = &floors[0];
        let last = floors.last().unwrap();
        let from = dungeon_point(def, first.depth, first.up_shaft.exit_cell());
        let to = dungeon_point(def, last.depth, last.stand_cell(last.chest.unwrap()));
        queries[6].push(Query {
            group: 6,
            map: def.id.clone(),
            from: from.clone(),
            to: to.clone(),
        });
        queries[6].push(Query {
            group: 6,
            map: def.id.clone(),
            from: to,
            to: from,
        });
        for floor in floors {
            let points: Vec<_> = floor
                .rooms
                .iter()
                .map(|room| dungeon_point(def, floor.depth, room.center()))
                .filter(|p| clear(&cache, p))
                .collect();
            if let Some(prop) = floor.props.iter().find(|prop| prop.kind.is_solid()) {
                let to = dungeon_point(def, floor.depth, (prop.x, prop.z));
                if let Some(from) = points.iter().find(|p| (p.x - to.x).hypot(p.z - to.z) > 4.0) {
                    queries[5].push(Query {
                        group: 5,
                        map: def.id.clone(),
                        from: from.clone(),
                        to,
                    });
                }
            }
            for from in &points {
                if let Some(to) = points.choose(&mut rng) {
                    let distance = (from.x - to.x).hypot(from.z - to.z);
                    if (4.0..=60.0).contains(&distance) {
                        queries[2].push(Query {
                            group: 2,
                            map: def.id.clone(),
                            from: from.clone(),
                            to: to.clone(),
                        });
                    }
                }
            }
            if let Some(next) = floors.get(floor.depth as usize) {
                if let Some(from) = points.choose(&mut rng) {
                    let to = dungeon_point(def, next.depth, next.rooms[0].center());
                    if clear(&cache, &to) {
                        queries[3].push(Query {
                            group: 3,
                            map: def.id.clone(),
                            from: from.clone(),
                            to,
                        });
                    }
                }
            }
            for door in dungeon::interior_doors(floor)
                .into_iter()
                .filter(|door| door.locked)
            {
                let (a, b) = if door.spans_x() {
                    ((door.lat0, door.wall_line - 1), (door.lat0, door.wall_line))
                } else {
                    ((door.wall_line - 1, door.lat0), (door.wall_line, door.lat0))
                };
                let from = dungeon_point(def, floor.depth, a);
                let to = dungeon_point(def, floor.depth, b);
                if clear(&closed, &from) && clear(&closed, &to) {
                    queries[4].push(Query {
                        group: 4,
                        map: def.id.clone(),
                        from: from.clone(),
                        to: to.clone(),
                    });
                    queries[4].push(Query {
                        group: 4,
                        map: def.id.clone(),
                        from: to,
                        to: from,
                    });
                }
            }
        }
    }
    for group in &mut queries {
        group.shuffle(&mut rng);
        group.truncate(64);
    }
    if queries.iter().any(Vec::is_empty) {
        return Err("one or more fixture groups are empty".into());
    }
    let serialized = serde_json::to_vec(&queries)?;
    let mut corpus_hash = 0xcbf29ce484222325;
    fingerprint(&mut corpus_hash, &serialized);
    let manifest = serde_json::json!({
        "kind": "fixtures", "seed": seed, "houses": house_files.len(),
        "object_files": object_files.len(), "furniture_regions": furniture_regions,
        "dungeons": layouts.len(), "dungeon_floors": layouts.iter().map(|(_, f)| f.len()).sum::<usize>(),
        "cache_entries": cache.len(), "input_fnv64": format!("{hash:016x}"),
        "corpus_fnv64": format!("{corpus_hash:016x}"),
        "groups": GROUPS.iter().zip(&queries).map(|(name, q)| (*name, q.len())).collect::<std::collections::BTreeMap<_, _>>(),
        "dungeon_state": "all doors open, props intact; closed_doors uses all doors closed",
        "excluded": ["runtime fences and estate storage", "terrain height/slope/water", "live player and monster state"]
    });
    Ok(Fixtures {
        open: cache,
        closed,
        queries,
        manifest,
    })
}
