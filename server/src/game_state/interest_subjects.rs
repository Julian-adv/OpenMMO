use onlinerpg_shared::interest::{Bounds, Space, SubjectArea};
use onlinerpg_shared::ServerMessage;

use super::interest::Interest;

impl Interest {
    pub(super) fn publish_state(&mut self, msg: &ServerMessage) -> bool {
        if self.update_dungeon(msg) {
            return true;
        }
        match msg {
            ServerMessage::PlayerInteractionChanged { object_type, .. }
                if object_type.as_deref() == Some("pickup") =>
            {
                return false
            }
            ServerMessage::GroundItemSpawned { item }
            | ServerMessage::GroundItemAppeared { item } => {
                self.publish(
                    format!("item:{}", item.instance_id),
                    vec![SubjectArea::point(item.position, item.floor_level)],
                    vec![ServerMessage::GroundItemAppeared { item: item.clone() }],
                    vec![ServerMessage::GroundItemRemoved {
                        instance_id: item.instance_id,
                        picked_up_by: None,
                    }],
                    vec![msg.clone()],
                );
            }
            ServerMessage::CampfireSpawned { campfire }
            | ServerMessage::CampfireAppeared { campfire } => {
                self.publish(
                    format!("campfire:{}", campfire.id),
                    vec![SubjectArea::point(campfire.position, campfire.floor_level)],
                    vec![ServerMessage::CampfireAppeared {
                        campfire: campfire.clone(),
                    }],
                    vec![ServerMessage::CampfireRemoved {
                        campfire_id: campfire.id,
                    }],
                    vec![msg.clone()],
                );
            }
            ServerMessage::StallPlaced { stall } | ServerMessage::StallAppeared { stall } => {
                self.stall_owners.insert(stall.id, stall.owner_name.clone());
                self.publish(
                    format!("stall:{}", stall.id),
                    vec![SubjectArea::point(stall.position, stall.floor_level)],
                    vec![ServerMessage::StallAppeared {
                        stall: stall.clone(),
                    }],
                    vec![ServerMessage::StallRemoved { stall_id: stall.id }],
                    vec![msg.clone()],
                );
            }
            ServerMessage::TipHatPlaced { tip_hat } | ServerMessage::TipHatAppeared { tip_hat } => {
                self.publish(
                    format!("hat:{}", tip_hat.id),
                    vec![SubjectArea::point(tip_hat.position, tip_hat.floor_level)],
                    vec![ServerMessage::TipHatAppeared {
                        tip_hat: tip_hat.clone(),
                    }],
                    vec![ServerMessage::TipHatRemoved {
                        tip_hat_id: tip_hat.id,
                    }],
                    vec![msg.clone()],
                );
            }
            ServerMessage::MealPlaced { meal } | ServerMessage::MealAppeared { meal } => {
                self.publish(
                    format!("meal:{}", meal.id),
                    vec![SubjectArea::point(meal.position, meal.floor_level)],
                    vec![ServerMessage::MealAppeared { meal: meal.clone() }],
                    vec![ServerMessage::MealRemoved { meal_id: meal.id }],
                    vec![msg.clone()],
                );
            }
            ServerMessage::PlayerJoined { player }
            | ServerMessage::PlayerAppeared { player }
            | ServerMessage::PlayerRespawned { player } => {
                self.publish(
                    format!("player:{}", player.id),
                    vec![SubjectArea::point(player.position, player.floor_level)],
                    vec![ServerMessage::PlayerAppeared {
                        player: player.clone(),
                    }],
                    vec![ServerMessage::PlayerDisappeared {
                        player_id: player.id,
                    }],
                    vec![msg.clone()],
                );
            }
            ServerMessage::MonsterSpawned { monster } => {
                self.publish(
                    format!("monster:{}", monster.id),
                    vec![SubjectArea::point(monster.position, monster.floor_level)],
                    vec![ServerMessage::MonsterSpawned {
                        monster: monster.clone(),
                    }],
                    vec![ServerMessage::MonsterRemoved {
                        monster_id: monster.id.clone(),
                    }],
                    vec![msg.clone()],
                );
            }
            ServerMessage::PlayerLeft { player_id } => {
                self.remove(&format!("player:{}", player_id), vec![msg.clone()])
            }
            ServerMessage::MonsterRemoved { monster_id } => {
                self.remove(&format!("monster:{}", monster_id), vec![msg.clone()])
            }
            ServerMessage::GroundItemRemoved { instance_id, .. } => {
                self.remove(&format!("item:{}", instance_id), vec![msg.clone()])
            }
            ServerMessage::CampfireRemoved { campfire_id } => {
                self.remove(&format!("campfire:{}", campfire_id), vec![msg.clone()])
            }
            ServerMessage::StallRemoved { stall_id } => {
                self.remove(&format!("stall:{}", stall_id), vec![msg.clone()])
            }
            ServerMessage::TipHatRemoved { tip_hat_id } => {
                self.remove(&format!("hat:{}", tip_hat_id), vec![msg.clone()])
            }
            ServerMessage::MealRemoved { meal_id } => {
                self.remove(&format!("meal:{}", meal_id), vec![msg.clone()])
            }
            ServerMessage::FenceVisibility { added, removed } => {
                for edge in removed {
                    self.remove(
                        &format!("fence:{},{},{:?}", edge.x, edge.z, edge.axis),
                        vec![ServerMessage::FenceVisibility {
                            added: vec![],
                            removed: vec![*edge],
                        }],
                    );
                }
                for fence in added {
                    let mut a = fence.edge.center(fence.y);
                    let mut b = a;
                    match fence.edge.axis {
                        onlinerpg_shared::fence::FenceAxis::X => {
                            a.x -= 0.5;
                            b.x += 0.5;
                        }
                        onlinerpg_shared::fence::FenceAxis::Z => {
                            a.z -= 0.5;
                            b.z += 0.5;
                        }
                    }
                    let snapshot = ServerMessage::FenceVisibility {
                        added: vec![fence.clone()],
                        removed: vec![],
                    };
                    self.publish(
                        format!(
                            "fence:{},{},{:?}",
                            fence.edge.x, fence.edge.z, fence.edge.axis
                        ),
                        vec![SubjectArea {
                            bounds: Bounds::Segment(a, b),
                            space: Space::Floor(0),
                        }],
                        vec![snapshot.clone()],
                        vec![ServerMessage::FenceVisibility {
                            added: vec![],
                            removed: vec![fence.edge],
                        }],
                        vec![snapshot],
                    );
                }
            }
            ServerMessage::EstateChestVisibility { added, removed } => {
                for id in removed {
                    self.remove(
                        &format!("chest:{id}"),
                        vec![ServerMessage::EstateChestVisibility {
                            added: vec![],
                            removed: vec![*id],
                        }],
                    );
                }
                for chest in added {
                    let snapshot = ServerMessage::EstateChestVisibility {
                        added: vec![chest.clone()],
                        removed: vec![],
                    };
                    self.publish(
                        format!("chest:{}", chest.id),
                        vec![SubjectArea::point(chest.position, chest.floor_level)],
                        vec![snapshot.clone()],
                        vec![ServerMessage::EstateChestVisibility {
                            added: vec![],
                            removed: vec![chest.id],
                        }],
                        vec![snapshot],
                    );
                }
            }
            ServerMessage::PlayerTorchToggled { player_id, .. }
            | ServerMessage::PlayerRadianceToggled { player_id, .. }
            | ServerMessage::PlayerWetToggled { player_id, .. }
            | ServerMessage::PlayerMountChanged { player_id, .. }
            | ServerMessage::PlayerTitleChanged { player_id, .. }
            | ServerMessage::PlayerMainHandChanged { player_id, .. }
            | ServerMessage::PlayerBackChanged { player_id, .. }
            | ServerMessage::PlayerInteractionChanged { player_id, .. }
            | ServerMessage::PlayerHealthUpdate { player_id, .. }
            | ServerMessage::PlayerDead { player_id, .. } => {
                self.update_snapshot(&format!("player:{player_id}"), msg.clone(), |snapshot| {
                    for entry in snapshot.iter_mut() {
                        if let ServerMessage::PlayerAppeared { player } = entry {
                            match msg {
                                ServerMessage::PlayerTorchToggled { enabled, .. } => { player.torch_on = *enabled; },
                                ServerMessage::PlayerRadianceToggled { enabled, .. } => { player.radiance_on = *enabled; },
                                ServerMessage::PlayerWetToggled { wet, .. } => { player.wet = *wet; },
                                ServerMessage::PlayerMountChanged { mount, .. } => { player.mount = *mount; },
                                ServerMessage::PlayerTitleChanged { title, .. } => { player.title = title.clone(); },
                                ServerMessage::PlayerMainHandChanged { item_def_id, .. } => { player.main_hand = item_def_id.clone(); },
                                ServerMessage::PlayerBackChanged { item_def_id, cape_color, cape_texture, .. } => { player.back = item_def_id.clone(); player.back_color = cape_color.clone(); player.back_texture = cape_texture.clone(); },
                                ServerMessage::PlayerInteractionChanged { object_type, object_id, .. } => { player.object_type = object_type.clone(); player.object_id = *object_id; },
                                ServerMessage::PlayerHealthUpdate { health, max_health, .. } => { player.health = *health; player.max_health = *max_health; },
                                ServerMessage::PlayerDead { .. } => { player.health = 0; },
                                _ => {}
                            }
                        }
                    }
                    if matches!(msg, ServerMessage::PlayerInteractionChanged { object_type, .. } if object_type.as_deref() != Some(onlinerpg_shared::messages::MUSIC_EMOTE)) {
                        snapshot.retain(|entry| !matches!(entry, ServerMessage::PlayerMusicStarted { .. } | ServerMessage::PlayerInstrumentStarted { .. }));
                    }
                });
            }
            ServerMessage::PlayerMusicStarted { player_id, .. }
            | ServerMessage::PlayerInstrumentStarted { player_id } => {
                self.update_snapshot(&format!("player:{player_id}"), msg.clone(), |snapshot| {
                    snapshot.retain(|entry| {
                        !matches!(
                            entry,
                            ServerMessage::PlayerMusicStarted { .. }
                                | ServerMessage::PlayerInstrumentStarted { .. }
                        )
                    });
                    snapshot.push(msg.clone());
                });
            }
            ServerMessage::GroundItemQuantityChanged {
                instance_id,
                quantity,
                ..
            } => self.update_snapshot(&format!("item:{instance_id}"), msg.clone(), |snapshot| {
                for entry in snapshot {
                    if let ServerMessage::GroundItemAppeared { item } = entry {
                        item.quantity = *quantity;
                    }
                }
            }),
            ServerMessage::MealEaten { meal_id, .. } => {
                self.update_snapshot(&format!("meal:{meal_id}"), msg.clone(), |snapshot| {
                    for entry in snapshot {
                        if let ServerMessage::MealAppeared { meal } = entry {
                            meal.eaten = true;
                        }
                    }
                })
            }
            ServerMessage::StallSignChanged { stall_id, sign } => {
                self.update_snapshot(&format!("stall:{stall_id}"), msg.clone(), |snapshot| {
                    for entry in snapshot {
                        if let ServerMessage::StallAppeared { stall } = entry {
                            stall.sign = sign.clone();
                        }
                    }
                })
            }
            ServerMessage::MonsterDead { monster_id, .. } => {
                self.stop_movement(&format!("monster:{monster_id}"));
                self.update_snapshot(&format!("monster:{monster_id}"), msg.clone(), |snapshot| {
                    for entry in snapshot {
                        if let ServerMessage::MonsterSpawned { monster } = entry {
                            monster.health = 0;
                            monster.state = onlinerpg_shared::MonsterState::Dead;
                        }
                    }
                });
            }
            _ => return false,
        }
        true
    }
}
