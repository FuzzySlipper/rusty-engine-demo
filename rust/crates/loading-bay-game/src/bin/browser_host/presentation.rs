//! Disposable browser presentation projected from accepted gameplay outcomes.
//!
//! The browser host owns this border. Rebuildable animation posture is read
//! from current authoritative state, while cues live only in one HTTP response.
//! Nothing here can mutate a session or enter a gameplay snapshot.

use core_ids::EntityId;
use loading_bay_game::{
    CombatFact, DoorState, EnemyCombatFact, EnemyState, ExtractionBeaconFact,
    ExtractionBeaconState, GameEvent, GameRuntime, NavigationState, PickupFact, PlayerControlFact,
    ProgressionFact, VitalityFact,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BrowserPresentation {
    animation_states: Vec<BrowserAnimationState>,
    cues: Vec<BrowserFeedbackCue>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserAnimationState {
    entity: u64,
    posture: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum BrowserFeedbackCue {
    Movement {
        entity: u64,
        from: [f32; 3],
        to: [f32; 3],
    },
    MovementBlocked {
        entity: u64,
    },
    Attack {
        attacker: u64,
        weapon: String,
        presentation: String,
        attack_mode: &'static str,
        ray_count: u8,
        origin: [f32; 3],
        direction: [f32; 3],
    },
    DryFire {
        attacker: u64,
        weapon: String,
        presentation: String,
    },
    Damage {
        attacker: u64,
        target: u64,
        amount: u32,
        remaining: u32,
    },
    EnemyAlert {
        entity: u64,
        target: u64,
        cause: &'static str,
    },
    EnemyAttack {
        attacker: u64,
        target: u64,
        attack_kind: &'static str,
        presentation: String,
        origin: [f32; 3],
        target_position: [f32; 3],
    },
    EnemyAttackMissed {
        attacker: u64,
        target: u64,
        reason: &'static str,
    },
    Defeat {
        attacker: Option<u64>,
        entity: u64,
    },
    EnemyDropMaterialized {
        enemy: u64,
        pickup: u64,
        item: String,
        quantity: u32,
        position: [f32; 3],
    },
    EncounterActivated {
        entity: u64,
        player: u64,
    },
    DoorChanged {
        entity: u64,
        state: &'static str,
    },
    ExtractionBeaconActivated {
        entity: u64,
        actor: u64,
    },
    PickupCollected {
        entity: u64,
        actor: u64,
        item: String,
        quantity: u32,
    },
    DoorAccessGranted {
        entity: u64,
        actor: u64,
        required_key: String,
        key_consumed: bool,
    },
    DoorAccessDenied {
        entity: u64,
        required_key: String,
        presentation: String,
    },
    SecretDiscovered {
        entity: u64,
        actor: u64,
        presentation: String,
    },
    LevelCompleted {
        entity: u64,
        actor: u64,
        presentation: String,
    },
}

/// Response-local projection accumulator. Repeated movement collapses to one
/// cue per entity so bounded multi-step phases cannot flood the browser.
#[derive(Debug, Default)]
pub(super) struct BrowserFeedbackProjection {
    cues: Vec<BrowserFeedbackCue>,
}

impl BrowserFeedbackProjection {
    pub(super) fn extend_player_control(&mut self, facts: &[PlayerControlFact]) {
        for fact in facts {
            match fact {
                PlayerControlFact::Moved {
                    entity,
                    before,
                    after,
                } => self.push_movement(*entity, before.to_array(), after.to_array()),
                PlayerControlFact::Blocked { entity, .. } => self.push_blocked(*entity),
                PlayerControlFact::LookChanged { .. } => {}
            }
        }
    }

    pub(super) fn extend_combat(&mut self, facts: &[CombatFact]) {
        for fact in facts {
            match fact {
                CombatFact::AttackFired {
                    attacker,
                    weapon,
                    presentation,
                    attack_mode,
                    ray_count,
                    origin,
                    direction,
                    ..
                } => self.cues.push(BrowserFeedbackCue::Attack {
                    attacker: attacker.raw(),
                    weapon: weapon.as_str().to_owned(),
                    presentation: presentation.clone(),
                    attack_mode: match attack_mode {
                        loading_bay_game::WeaponAttackMode::Hitscan => "hitscan",
                        loading_bay_game::WeaponAttackMode::Spread { .. } => "spread",
                        loading_bay_game::WeaponAttackMode::Automatic => "automatic",
                    },
                    ray_count: *ray_count,
                    origin: origin.to_array(),
                    direction: direction.to_array(),
                }),
                CombatFact::Vitality(VitalityFact::DamageApplied {
                    source,
                    target,
                    health_damage,
                    health_after,
                    ..
                }) => self.cues.push(BrowserFeedbackCue::Damage {
                    attacker: source.entity().raw(),
                    target: target.raw(),
                    amount: *health_damage,
                    remaining: *health_after,
                }),
                CombatFact::EnemyDefeated {
                    attacker, enemy, ..
                } => self.push_defeat(Some(*attacker), *enemy),
                CombatFact::EnemyDrop(drop) => {
                    self.cues.push(BrowserFeedbackCue::EnemyDropMaterialized {
                        enemy: drop.enemy.raw(),
                        pickup: drop.pickup.raw(),
                        item: drop.item.as_str().to_owned(),
                        quantity: drop.quantity,
                        position: drop.position.to_array(),
                    });
                }
                CombatFact::Inventory(_)
                | CombatFact::Vitality(_)
                | CombatFact::AttackHit { .. }
                | CombatFact::AttackMissed { .. } => {}
            }
        }
    }

    pub(super) fn extend_enemy_combat(&mut self, facts: &[EnemyCombatFact]) {
        for fact in facts {
            match fact {
                EnemyCombatFact::Alerted {
                    enemy,
                    target,
                    cause,
                } => self.cues.push(BrowserFeedbackCue::EnemyAlert {
                    entity: enemy.raw(),
                    target: target.raw(),
                    cause: match cause {
                        loading_bay_game::EnemyPerceptionCause::Sight => "sight",
                        loading_bay_game::EnemyPerceptionCause::Hearing => "hearing",
                    },
                }),
                EnemyCombatFact::AttackFired {
                    enemy,
                    target,
                    kind,
                    presentation,
                    origin,
                    target_position,
                    ..
                } => self.cues.push(BrowserFeedbackCue::EnemyAttack {
                    attacker: enemy.raw(),
                    target: target.raw(),
                    attack_kind: match kind {
                        loading_bay_game::EnemyAttackKind::Melee => "melee",
                        loading_bay_game::EnemyAttackKind::RangedHitscan => "rangedHitscan",
                    },
                    presentation: presentation.clone(),
                    origin: origin.to_array(),
                    target_position: target_position.to_array(),
                }),
                EnemyCombatFact::AttackMissed {
                    enemy,
                    target,
                    reason,
                    ..
                } => self.cues.push(BrowserFeedbackCue::EnemyAttackMissed {
                    attacker: enemy.raw(),
                    target: target.raw(),
                    reason: match reason {
                        loading_bay_game::EnemyAttackMissReason::WorldBlocked => "worldBlocked",
                        loading_bay_game::EnemyAttackMissReason::TargetOutOfRange => {
                            "targetOutOfRange"
                        }
                        loading_bay_game::EnemyAttackMissReason::TargetDead => "targetDead",
                    },
                }),
                EnemyCombatFact::Vitality(VitalityFact::DamageApplied {
                    source,
                    target,
                    health_damage,
                    health_after,
                    ..
                }) => self.cues.push(BrowserFeedbackCue::Damage {
                    attacker: source.entity().raw(),
                    target: target.raw(),
                    amount: *health_damage,
                    remaining: *health_after,
                }),
                EnemyCombatFact::PostureChanged { .. }
                | EnemyCombatFact::AttackHit { .. }
                | EnemyCombatFact::Vitality(_) => {}
            }
        }
    }

    pub(super) fn extend_vitality(&mut self, facts: &[VitalityFact]) {
        for fact in facts {
            if let VitalityFact::DamageApplied {
                source,
                target,
                health_damage,
                health_after,
                ..
            } = fact
            {
                self.cues.push(BrowserFeedbackCue::Damage {
                    attacker: source.entity().raw(),
                    target: target.raw(),
                    amount: *health_damage,
                    remaining: *health_after,
                });
            }
        }
    }

    pub(super) fn extend_events(&mut self, events: &[GameEvent]) {
        for event in events {
            match event {
                GameEvent::DoorOpened { door, .. } => self.push_door(*door, "open"),
                GameEvent::DoorClosed { door, .. } => self.push_door(*door, "closed"),
                GameEvent::EnemyDefeated { enemy, actor, .. } => {
                    self.push_defeat(Some(*actor), *enemy);
                }
                GameEvent::PlayerDied { player, source, .. } => {
                    self.push_defeat(Some(*source), *player);
                }
                GameEvent::EncounterActivated { encounter, player } => {
                    self.cues.push(BrowserFeedbackCue::EncounterActivated {
                        entity: encounter.raw(),
                        player: player.raw(),
                    });
                }
                GameEvent::SwitchActivated { .. } | GameEvent::EncounterCleared { .. } => {}
            }
        }
    }

    pub(super) fn extend_extraction_beacon(&mut self, fact: ExtractionBeaconFact) {
        let ExtractionBeaconFact::Activated { beacon, actor, .. } = fact;
        self.cues
            .push(BrowserFeedbackCue::ExtractionBeaconActivated {
                entity: beacon.raw(),
                actor: actor.raw(),
            });
    }

    pub(super) fn extend_dry_fire(
        &mut self,
        attacker: EntityId,
        weapon: &loading_bay_game::ItemDefinitionId,
        presentation: &str,
    ) {
        self.cues.push(BrowserFeedbackCue::DryFire {
            attacker: attacker.raw(),
            weapon: weapon.as_str().to_owned(),
            presentation: presentation.to_owned(),
        });
    }

    pub(super) fn extend_pickup(&mut self, fact: &PickupFact) {
        let PickupFact::Collected {
            pickup,
            actor,
            item,
            quantity,
            ..
        } = fact;
        self.cues.push(BrowserFeedbackCue::PickupCollected {
            entity: pickup.raw(),
            actor: actor.raw(),
            item: item.as_str().to_owned(),
            quantity: *quantity,
        });
    }

    pub(super) fn extend_progression(&mut self, fact: &ProgressionFact) {
        match fact {
            ProgressionFact::DoorAccessGranted {
                door,
                actor,
                required_key,
                key_policy,
                ..
            } => self.cues.push(BrowserFeedbackCue::DoorAccessGranted {
                entity: door.raw(),
                actor: actor.raw(),
                required_key: required_key.as_str().to_owned(),
                key_consumed: *key_policy == loading_bay_game::RequiredKeyPolicy::Consume,
            }),
            ProgressionFact::SecretDiscovered {
                secret,
                actor,
                presentation,
                ..
            } => self.cues.push(BrowserFeedbackCue::SecretDiscovered {
                entity: secret.raw(),
                actor: actor.raw(),
                presentation: presentation.clone(),
            }),
            ProgressionFact::LevelCompleted {
                exit,
                actor,
                presentation,
                ..
            } => self.cues.push(BrowserFeedbackCue::LevelCompleted {
                entity: exit.raw(),
                actor: actor.raw(),
                presentation: presentation.clone(),
            }),
        }
    }

    pub(super) fn extend_door_access_denied(
        &mut self,
        door: EntityId,
        required_key: &loading_bay_game::ItemDefinitionId,
        presentation: &str,
    ) {
        self.cues.push(BrowserFeedbackCue::DoorAccessDenied {
            entity: door.raw(),
            required_key: required_key.as_str().to_owned(),
            presentation: presentation.to_owned(),
        });
    }

    fn push_movement(&mut self, entity: EntityId, from: [f32; 3], to: [f32; 3]) {
        if let Some(BrowserFeedbackCue::Movement {
            to: previous_to, ..
        }) = self.cues.iter_mut().find(|cue| {
            matches!(cue, BrowserFeedbackCue::Movement { entity: existing, .. } if *existing == entity.raw())
        }) {
            *previous_to = to;
            return;
        }
        self.cues.push(BrowserFeedbackCue::Movement {
            entity: entity.raw(),
            from,
            to,
        });
    }

    fn push_blocked(&mut self, entity: EntityId) {
        let cue = BrowserFeedbackCue::MovementBlocked {
            entity: entity.raw(),
        };
        if !self.cues.contains(&cue) {
            self.cues.push(cue);
        }
    }

    fn push_defeat(&mut self, attacker: Option<EntityId>, entity: EntityId) {
        if self.cues.iter().any(
            |cue| matches!(cue, BrowserFeedbackCue::Defeat { entity: existing, .. } if *existing == entity.raw()),
        ) {
            return;
        }
        self.cues.push(BrowserFeedbackCue::Defeat {
            attacker: attacker.map(EntityId::raw),
            entity: entity.raw(),
        });
    }

    fn push_door(&mut self, entity: EntityId, state: &'static str) {
        self.cues.retain(
            |cue| !matches!(cue, BrowserFeedbackCue::DoorChanged { entity: existing, .. } if *existing == entity.raw()),
        );
        self.cues.push(BrowserFeedbackCue::DoorChanged {
            entity: entity.raw(),
            state,
        });
    }
}

pub(super) fn project_presentation(
    runtime: &GameRuntime,
    player: EntityId,
    enemies: &[EntityId],
    door: EntityId,
    beacon: EntityId,
    feedback: BrowserFeedbackProjection,
) -> BrowserPresentation {
    let mut animation_states = Vec::with_capacity(enemies.len() + 3);
    animation_states.push(BrowserAnimationState {
        entity: player.raw(),
        posture: if runtime
            .session()
            .health(player)
            .is_some_and(|health| health.state == loading_bay_game::VitalityState::Dead)
        {
            "defeated"
        } else {
            "idle"
        },
    });
    if let Some(beacon) = runtime.session().extraction_beacon(beacon) {
        animation_states.push(BrowserAnimationState {
            entity: beacon.entity.raw(),
            posture: match beacon.state {
                ExtractionBeaconState::Standby => "standby",
                ExtractionBeaconState::Active { .. } => "active",
            },
        });
    }
    animation_states.extend(enemies.iter().map(|entity| {
        let enemy = runtime
            .session()
            .enemy(*entity)
            .expect("presentation enemy");
        let posture = match runtime.session().enemy_combat(*entity) {
            Some(combat) => match combat.state.posture {
                loading_bay_game::EnemyCombatPosture::Sleeping => "idle",
                loading_bay_game::EnemyCombatPosture::Alert => "alert",
                loading_bay_game::EnemyCombatPosture::Pursuing => "moving",
                loading_bay_game::EnemyCombatPosture::Attacking => "attacking",
                loading_bay_game::EnemyCombatPosture::Dead => "defeated",
            },
            None if enemy.state == EnemyState::Defeated => "defeated",
            None if runtime
                .session()
                .navigation(*entity)
                .is_some_and(|navigation| navigation.state == NavigationState::Following) =>
            {
                "moving"
            }
            None => "idle",
        };
        BrowserAnimationState {
            entity: entity.raw(),
            posture,
        }
    }));
    let door_state = runtime
        .session()
        .door(door)
        .expect("presentation door")
        .state;
    animation_states.push(BrowserAnimationState {
        entity: door.raw(),
        posture: match door_state {
            DoorState::Closed => "closed",
            DoorState::Open => "open",
        },
    });
    BrowserPresentation {
        animation_states,
        cues: feedback.cues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_math::Vec3;

    #[test]
    fn typed_facts_keep_payloads_and_collapse_repeated_disposable_cues() {
        let actor = EntityId::new(1);
        let enemy = EntityId::new(4);
        let door = EntityId::new(3);
        let weapon = loading_bay_game::ItemDefinitionId::parse("weapon/arc-pistol").unwrap();
        let ammunition = loading_bay_game::ItemDefinitionId::parse("ammo/energy-cell").unwrap();
        let mut projection = BrowserFeedbackProjection::default();
        projection.extend_player_control(&[
            PlayerControlFact::Moved {
                entity: actor,
                before: Vec3::ZERO,
                after: Vec3::new(1.0, 0.0, 0.0),
            },
            PlayerControlFact::Moved {
                entity: actor,
                before: Vec3::new(1.0, 0.0, 0.0),
                after: Vec3::new(2.0, 0.0, 0.0),
            },
        ]);
        projection.extend_combat(&[
            CombatFact::AttackFired {
                attacker: actor,
                weapon: weapon.clone(),
                presentation: "arc-pistol".to_owned(),
                attack_mode: loading_bay_game::WeaponAttackMode::Hitscan,
                ammunition,
                origin: Vec3::new(2.0, 1.0, 0.0),
                direction: Vec3::new(0.0, 0.0, -1.0),
                ray_count: 1,
                spread_seed: 17,
                ammo_before: 8,
                ammo_after: 7,
                ready_at_tick: core_time::Tick::new(2),
            },
            CombatFact::Vitality(VitalityFact::DamageApplied {
                source: loading_bay_game::DamageSource::Weapon {
                    attacker: actor,
                    weapon: loading_bay_game::ItemDefinitionId::parse("weapon/arc-pistol").unwrap(),
                },
                target: enemy,
                incoming: 60,
                armor_absorbed: 0,
                health_damage: 60,
                health_before: 100,
                health_after: 40,
                armor_before: 0,
                armor_after: 0,
            }),
            CombatFact::EnemyDefeated {
                attacker: actor,
                enemy,
            },
        ]);
        projection.extend_dry_fire(actor, &weapon, "arc-pistol");
        projection.extend_events(&[
            GameEvent::EnemyDefeated {
                enemy,
                actor,
                entity_facts: Vec::new(),
            },
            GameEvent::DoorOpened {
                door,
                entity_facts: Vec::new(),
            },
        ]);
        projection.extend_extraction_beacon(ExtractionBeaconFact::Activated {
            beacon: EntityId::new(7),
            actor,
            tick: core_time::Tick::new(2),
        });

        assert_eq!(projection.cues.len(), 7);
        assert_eq!(
            projection.cues[0],
            BrowserFeedbackCue::Movement {
                entity: 1,
                from: [0.0, 0.0, 0.0],
                to: [2.0, 0.0, 0.0],
            }
        );
        assert!(matches!(
            projection.cues.as_slice(),
            [
                BrowserFeedbackCue::Movement { .. },
                BrowserFeedbackCue::Attack {
                    origin: [2.0, 1.0, 0.0],
                    ..
                },
                BrowserFeedbackCue::Damage {
                    amount: 60,
                    remaining: 40,
                    ..
                },
                BrowserFeedbackCue::Defeat { entity: 4, .. },
                BrowserFeedbackCue::DryFire {
                    attacker: 1,
                    presentation,
                    ..
                },
                BrowserFeedbackCue::DoorChanged {
                    entity: 3,
                    state: "open"
                },
                BrowserFeedbackCue::ExtractionBeaconActivated {
                    entity: 7,
                    actor: 1
                },
            ] if presentation == "arc-pistol"
        ));
    }

    #[test]
    fn hazard_damage_produces_disposable_feedback_without_owning_vitality() {
        let player = EntityId::new(1);
        let hazard = EntityId::new(27);
        let mut projection = BrowserFeedbackProjection::default();

        projection.extend_vitality(&[VitalityFact::DamageApplied {
            source: loading_bay_game::DamageSource::Hazard { hazard },
            target: player,
            incoming: 20,
            armor_absorbed: 5,
            health_damage: 15,
            health_before: 100,
            health_after: 85,
            armor_before: 10,
            armor_after: 5,
        }]);

        assert_eq!(
            projection.cues,
            [BrowserFeedbackCue::Damage {
                attacker: hazard.raw(),
                target: player.raw(),
                amount: 15,
                remaining: 85,
            }]
        );
    }
}
