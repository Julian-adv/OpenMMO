use onlinerpg_shared::dungeon::{
    cell_center, door_position, interior_doors, FloorLayout, PropKind,
};
use onlinerpg_shared::interest::{Bounds, Space, SubjectArea};
use onlinerpg_shared::{Position, ServerMessage};

use super::interest::Interest;

fn area(id: &str, depth: u8, position: Position) -> SubjectArea {
    SubjectArea {
        bounds: Bounds::Point(position),
        space: if depth == 0 {
            Space::Surface
        } else {
            Space::Dungeon {
                id: id.to_owned(),
                depth,
            }
        },
    }
}

impl Interest {
    pub(super) fn reset_dungeon_props(&mut self, id: &str, layouts: &[FloorLayout]) {
        for layout in layouts {
            for (index, prop) in layout.props.iter().enumerate() {
                if prop.kind == PropKind::TorchWall {
                    continue;
                }
                let state = ServerMessage::DungeonPropState {
                    entrance_id: id.to_owned(),
                    depth: layout.depth,
                    prop_id: index as u32,
                    active: true,
                    broken: false,
                    opened: false,
                };
                self.update_snapshot(
                    &format!("prop:{id}:{}:{index}", layout.depth),
                    state.clone(),
                    |snapshot| *snapshot = vec![state],
                );
            }
        }
    }

    pub(super) fn seed_dungeon(&mut self, id: &str, entrance: &Position, layouts: &[FloorLayout]) {
        let doors = std::iter::once((0, 0)).chain(layouts.iter().flat_map(|layout| {
            interior_doors(layout)
                .into_iter()
                .map(move |door| (layout.depth, door.door_id))
        }));
        for (depth, door_id) in doors {
            let key = format!("door:{id}:{depth}:{door_id}");
            if self.has_subject(&key) {
                continue;
            }
            let Some(position) = door_position(entrance, layouts, depth, door_id) else {
                continue;
            };
            let state = ServerMessage::DungeonDoorState {
                entrance_id: id.to_owned(),
                depth,
                door_id,
                is_open: Some(false),
            };
            self.publish(
                key,
                vec![area(id, depth, position)],
                vec![state.clone()],
                vec![ServerMessage::DungeonDoorState {
                    entrance_id: id.to_owned(),
                    depth,
                    door_id,
                    is_open: None,
                }],
                vec![state],
            );
        }
        for layout in layouts {
            for (index, prop) in layout
                .props
                .iter()
                .enumerate()
                .filter(|(_, prop)| prop.kind != PropKind::TorchWall)
            {
                let prop_id = index as u32;
                let depth = layout.depth;
                let key = format!("prop:{id}:{depth}:{prop_id}");
                if self.has_subject(&key) {
                    continue;
                }
                let state = ServerMessage::DungeonPropState {
                    entrance_id: id.to_owned(),
                    depth,
                    prop_id,
                    active: true,
                    broken: false,
                    opened: false,
                };
                self.publish(
                    key,
                    vec![area(
                        id,
                        depth,
                        cell_center(entrance, depth, (prop.x, prop.z)),
                    )],
                    vec![state.clone()],
                    vec![ServerMessage::DungeonPropState {
                        entrance_id: id.to_owned(),
                        depth,
                        prop_id,
                        active: false,
                        broken: false,
                        opened: false,
                    }],
                    vec![state],
                );
            }
        }
    }

    pub(super) fn update_dungeon(&mut self, message: &ServerMessage) -> bool {
        match message {
            ServerMessage::DungeonDoorToggled {
                entrance_id,
                depth,
                door_id,
                is_open,
            } => self.update_snapshot(
                &format!("door:{entrance_id}:{depth}:{door_id}"),
                message.clone(),
                |snapshot| {
                    *snapshot = vec![ServerMessage::DungeonDoorState {
                        entrance_id: entrance_id.clone(),
                        depth: *depth,
                        door_id: *door_id,
                        is_open: Some(*is_open),
                    }];
                },
            ),
            ServerMessage::DungeonPropBroken {
                entrance_id,
                depth,
                prop_id,
            }
            | ServerMessage::DungeonPropOpened {
                entrance_id,
                depth,
                prop_id,
            } => self.update_snapshot(
                &format!("prop:{entrance_id}:{depth}:{prop_id}"),
                message.clone(),
                |snapshot| {
                    for entry in snapshot {
                        if let ServerMessage::DungeonPropState { broken, opened, .. } = entry {
                            if matches!(message, ServerMessage::DungeonPropBroken { .. }) {
                                *broken = true;
                            } else {
                                *opened = true;
                            }
                        }
                    }
                },
            ),
            _ => return false,
        }
        true
    }
}
