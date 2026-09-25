use serde::{Deserialize, Serialize};

use crate::{housing::HouseData, Position, ServerMessage, EVENT_DELIVERY_RADIUS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Space {
    Surface,
    Floor(i8),
    Dungeon { id: String, depth: u8 },
    Unknown,
}

impl Space {
    pub fn at(position: &Position, floor: i8) -> Self {
        if floor >= 0 {
            Self::Floor(floor)
        } else if let Some(entrance) = crate::dungeon::entrance_at(position.x, position.z) {
            Self::Dungeon {
                id: entrance.id.clone(),
                depth: floor.unsigned_abs(),
            }
        } else {
            Self::Unknown
        }
    }

    pub fn includes(&self, viewer: &Self) -> bool {
        match (self, viewer) {
            (Self::Unknown, _) | (_, Self::Unknown) => false,
            (Self::Surface, Self::Floor(floor)) => *floor >= 0,
            _ => self == viewer,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Bounds {
    Point(Position),
    Segment(Position, Position),
    Rect { min: Position, max: Position },
    Union(Vec<Bounds>),
}

impl Bounds {
    pub fn house(house: &HouseData) -> Self {
        Self::Union(
            house
                .rooms
                .iter()
                .map(|room| {
                    let min = Position {
                        x: house.origin.x + room.local_x as f32,
                        y: house.origin.y,
                        z: house.origin.z + room.local_z as f32,
                    };
                    Self::Rect {
                        min,
                        max: Position {
                            x: min.x + room.size_x as f32,
                            z: min.z + room.size_z as f32,
                            ..min
                        },
                    }
                })
                .collect(),
        )
    }

    pub fn tile(x: i32, z: i32, size: f32) -> Self {
        Self::Rect {
            min: Position {
                x: x as f32 * size - size * 0.5,
                y: 0.0,
                z: z as f32 * size - size * 0.5,
            },
            max: Position {
                x: x as f32 * size + size * 0.5,
                y: 0.0,
                z: z as f32 * size + size * 0.5,
            },
        }
    }

    pub fn distance_sq(&self, viewer: &Position) -> f32 {
        match self {
            Self::Point(point) => viewer.dist_xz_sq(point),
            Self::Segment(from, to) => {
                let dx = crate::shortest_world_delta_x(from.x, to.x);
                let dz = to.z - from.z;
                let px = crate::shortest_world_delta_x(from.x, viewer.x);
                let pz = viewer.z - from.z;
                let len = dx * dx + dz * dz;
                let t = if len == 0.0 {
                    0.0
                } else {
                    ((px * dx + pz * dz) / len).clamp(0.0, 1.0)
                };
                (px - t * dx).powi(2) + (pz - t * dz).powi(2)
            }
            Self::Rect { min, max } => {
                let center = (min.x + max.x) * 0.5;
                let dx = (crate::shortest_world_delta_x(center, viewer.x).abs()
                    - (max.x - min.x) * 0.5)
                    .max(0.0);
                let dz = (min.z - viewer.z).max(viewer.z - max.z).max(0.0);
                dx * dx + dz * dz
            }
            Self::Union(parts) => parts
                .iter()
                .map(|part| part.distance_sq(viewer))
                .fold(f32::INFINITY, f32::min),
        }
    }

    pub fn rectangles(&self, output: &mut Vec<(Position, Position)>) {
        match self {
            Self::Point(p) => output.push((*p, *p)),
            Self::Rect { min, max } => output.push((*min, *max)),
            Self::Segment(a, b) => {
                let bx = a.x + crate::shortest_world_delta_x(a.x, b.x);
                output.push((
                    Position {
                        x: a.x.min(bx),
                        z: a.z.min(b.z),
                        ..*a
                    },
                    Position {
                        x: a.x.max(bx),
                        z: a.z.max(b.z),
                        ..*b
                    },
                ));
            }
            Self::Union(parts) => parts.iter().for_each(|part| part.rectangles(output)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubjectArea {
    pub bounds: Bounds,
    pub space: Space,
}

impl SubjectArea {
    pub fn point(position: Position, floor: i8) -> Self {
        Self {
            bounds: Bounds::Point(position),
            space: Space::at(&position, floor),
        }
    }

    pub fn contains(&self, position: &Position, space: &Space) -> bool {
        self.space.includes(space)
            && self.bounds.distance_sq(position) <= EVENT_DELIVERY_RADIUS.powi(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InterestChange {
    Enter,
    Update,
    Leave,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    pub subject: String,
    pub revision: u64,
    pub change: InterestChange,
    pub messages: Vec<ServerMessage>,
}

#[derive(Debug, Clone, Default)]
pub struct WorldView {
    pub world_epoch: String,
    pub generation: u64,
    pub sequence: u64,
    pub synchronized: bool,
    pub subjects: std::collections::HashMap<String, u64>,
    pub position: Option<Position>,
    pub floor_level: i8,
    retired_epochs: std::collections::HashSet<String>,
}

impl WorldView {
    pub fn covers(&self, position: &Position) -> bool {
        self.synchronized
            && self.position.is_some_and(|center| {
                center.dist_xz_sq(position) <= (EVENT_DELIVERY_RADIUS - 1.0).powi(2)
            })
    }

    pub fn accept(
        &mut self,
        epoch: &str,
        generation: u64,
        sequence: u64,
        reset: bool,
        events: &[WorldEvent],
    ) -> bool {
        if self.retired_epochs.contains(epoch)
            || (self.world_epoch == epoch
                && (generation < self.generation
                    || (generation == self.generation && sequence <= self.sequence)))
        {
            return false;
        }
        if reset {
            if sequence != 1 || (self.world_epoch == epoch && generation <= self.generation) {
                self.synchronized = false;
                return false;
            }
            if !self.world_epoch.is_empty() && self.world_epoch != epoch {
                self.retired_epochs.insert(self.world_epoch.clone());
            }
            self.subjects.clear();
            self.world_epoch = epoch.to_owned();
            self.generation = generation;
            self.sequence = 0;
            self.synchronized = true;
        }
        if !self.synchronized
            || self.world_epoch != epoch
            || self.generation != generation
            || sequence != self.sequence + 1
        {
            self.synchronized = false;
            return false;
        }
        for event in events {
            if self
                .subjects
                .get(&event.subject)
                .is_some_and(|revision| *revision > event.revision)
            {
                self.synchronized = false;
                return false;
            }
        }
        for event in events {
            match event.change {
                InterestChange::Enter | InterestChange::Update => {
                    self.subjects.insert(event.subject.clone(), event.revision);
                }
                InterestChange::Leave | InterestChange::Delete => {
                    self.subjects.remove(&event.subject);
                }
            }
        }
        self.sequence = sequence;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, z: f32) -> Position {
        Position { x, y: 0.0, z }
    }

    #[test]
    fn boundary_wrap_and_tile_corners() {
        let area = SubjectArea::point(p(crate::WORLD_MAX_X - 1.0, 0.0), 0);
        for (offset, expected) in [
            (EVENT_DELIVERY_RADIUS - 0.01, true),
            (EVENT_DELIVERY_RADIUS, true),
            (EVENT_DELIVERY_RADIUS + 0.01, false),
        ] {
            assert_eq!(
                area.contains(
                    &p(crate::wrap_world_x(crate::WORLD_MAX_X - 1.0 + offset), 0.0),
                    &Space::Floor(0)
                ),
                expected
            );
        }
        let tile = SubjectArea {
            bounds: Bounds::tile(0, 0, 64.0),
            space: Space::Surface,
        };
        assert!(tile.contains(&p(60.0, 0.0), &Space::Floor(2)));
        assert!(!tile.contains(&p(60.0, 60.0), &Space::Floor(0)));
        assert!(!tile.contains(
            &p(0.0, 0.0),
            &Space::Dungeon {
                id: "a".into(),
                depth: 1
            }
        ));
    }

    #[test]
    fn separate_rooms_and_movement_endpoints() {
        let area = SubjectArea {
            bounds: Bounds::Union(vec![
                Bounds::Point(p(-40.0, 0.0)),
                Bounds::Point(p(40.0, 0.0)),
            ]),
            space: Space::Floor(0),
        };
        assert!(!area.contains(&p(0.0, 0.0), &Space::Floor(0)));
        assert!(area.contains(&p(8.0, 0.0), &Space::Floor(0)));
        assert!(!area.contains(&p(8.0, 0.0), &Space::Floor(1)));
    }

    #[test]
    fn missing_sequence_requires_new_snapshot() {
        let mut view = WorldView::default();
        assert!(view.accept("a", 1, 1, true, &[]));
        assert!(!view.accept("a", 1, 3, false, &[]));
        assert!(!view.accept("a", 1, 2, false, &[]));
        assert!(view.accept("a", 2, 1, true, &[]));
        assert!(!view.accept("a", 1, 4, false, &[]));
    }
}
