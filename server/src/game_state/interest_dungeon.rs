use onlinerpg_shared::dungeon::{
    cart_rail_run, cell_center, door_position, interior_doors, FloorLayout, PropKind,
};
use onlinerpg_shared::interest::{Bounds, Space, SubjectArea};
use onlinerpg_shared::{Position, ServerMessage};

use super::interest::Interest;

fn area(id: &str, depth: u8, bounds: Bounds) -> SubjectArea {
    SubjectArea {
        bounds,
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

fn door_gone(id: &str, depth: u8, door_id: u32) -> ServerMessage {
    ServerMessage::DungeonDoorState {
        entrance_id: id.to_owned(),
        depth,
        door_id,
        is_open: None,
    }
}

fn prop_gone(id: &str, depth: u8, prop_id: u32) -> ServerMessage {
    ServerMessage::DungeonPropState {
        entrance_id: id.to_owned(),
        depth,
        prop_id,
        active: false,
        broken: false,
        opened: false,
        offset: 0,
    }
}

impl Interest {
    pub(super) fn reset_dungeon_props(
        &mut self,
        id: &str,
        layouts: &[FloorLayout],
        kinds: impl Fn(PropKind) -> bool,
    ) {
        for layout in layouts {
            for (index, prop) in layout.props.iter().enumerate() {
                if !kinds(prop.kind) {
                    continue;
                }
                let state = ServerMessage::DungeonPropState {
                    entrance_id: id.to_owned(),
                    depth: layout.depth,
                    prop_id: index as u32,
                    active: true,
                    broken: false,
                    opened: false,
                    offset: 0,
                };
                self.update_snapshot(
                    &format!("prop:{id}:{}:{index}", layout.depth),
                    state.clone(),
                    |snapshot| *snapshot = vec![state],
                );
            }
        }
    }

    /// Drop the interior door and prop subjects of a maze being replaced.
    pub(super) fn remove_dungeon_floors(&mut self, id: &str, layouts: &[FloorLayout]) {
        for layout in layouts {
            let depth = layout.depth;
            for door in interior_doors(layout) {
                let door_id = door.door_id;
                self.remove(
                    &format!("door:{id}:{depth}:{door_id}"),
                    vec![door_gone(id, depth, door_id)],
                );
            }
            for (index, _) in layout
                .props
                .iter()
                .enumerate()
                .filter(|(_, prop)| prop.kind.is_interactive())
            {
                self.remove(
                    &format!("prop:{id}:{depth}:{index}"),
                    vec![prop_gone(id, depth, index as u32)],
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
                vec![area(id, depth, Bounds::Point(position))],
                vec![state.clone()],
                vec![door_gone(id, depth, door_id)],
                vec![state],
            );
        }
        for layout in layouts {
            for (index, prop) in layout
                .props
                .iter()
                .enumerate()
                .filter(|(_, prop)| prop.kind.is_interactive())
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
                    offset: 0,
                };
                // A cart is heard anywhere along the rail it slides on.
                let run = (prop.kind == PropKind::MineRailCart)
                    .then(|| cart_rail_run(layout, prop))
                    .flatten();
                let bounds = match run {
                    Some(run) => Bounds::Segment(
                        cell_center(entrance, depth, run.cell(run.start)),
                        cell_center(entrance, depth, run.cell(run.end - 1)),
                    ),
                    None => Bounds::Point(cell_center(entrance, depth, (prop.x, prop.z))),
                };
                self.publish(
                    key,
                    vec![area(id, depth, bounds)],
                    vec![state.clone()],
                    vec![prop_gone(id, depth, prop_id)],
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
            }
            | ServerMessage::DungeonCartPushed {
                entrance_id,
                depth,
                prop_id,
                ..
            } => self.update_snapshot(
                &format!("prop:{entrance_id}:{depth}:{prop_id}"),
                message.clone(),
                |snapshot| {
                    for entry in snapshot {
                        if let ServerMessage::DungeonPropState {
                            broken,
                            opened,
                            offset,
                            ..
                        } = entry
                        {
                            match message {
                                ServerMessage::DungeonPropBroken { .. } => *broken = true,
                                ServerMessage::DungeonCartPushed { offset: to, .. } => {
                                    *offset = *to
                                }
                                _ => *opened = true,
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
