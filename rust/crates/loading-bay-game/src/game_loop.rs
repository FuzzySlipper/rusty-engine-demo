use std::collections::VecDeque;
use std::time::Duration;

use core_ids::EntityId;
use serde::{Deserialize, Serialize};

use crate::{
    CombatFact, CombatRejectionReason, DamageService, EnemyCombatFact, ExtractionBeaconFact,
    GameEvent, GameRuntime, HazardFact, InventoryFact, InventoryRejection, InventoryService,
    ItemDefinitionId, NavigationFact, PickupFact, PickupReceipt, PickupRejection,
    PlayerControlFact, ResolvedAttackAction, RuntimeError, SaveSlotId, VitalityFact,
    VitalityRejection,
};

pub const FIXED_SIMULATION_HZ: u32 = 60;
pub const FIXED_STEP_SECONDS: f32 = 1.0 / FIXED_SIMULATION_HZ as f32;
pub const FIXED_STEP_DURATION: Duration = Duration::from_nanos(16_666_667);
pub const MAX_CATCH_UP_TICKS: usize = 5;
pub const MAX_EDGE_COMMANDS: usize = 32;
pub const MAX_RETAINED_COMMAND_SEQUENCES: usize = 64;
pub const MAX_PENDING_GAME_LOOP_FACTS: usize = 256;
pub const MAX_INPUT_AGE_TICKS: u64 = 2;
pub const MAX_ACCUMULATED_LOOK_UNITS: f32 = 1.0;

pub const FIXED_TICK_PHASE_ORDER: [GameLoopPhase; 8] = [
    GameLoopPhase::InputConsumption,
    GameLoopPhase::PlayerMotion,
    GameLoopPhase::EnemyIntentAndMotion,
    GameLoopPhase::Hazards,
    GameLoopPhase::Combat,
    GameLoopPhase::InteractionsAndPickups,
    GameLoopPhase::ScheduledConsequences,
    GameLoopPhase::ProjectionAndFactDrain,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameLoopPhase {
    InputConsumption,
    PlayerMotion,
    EnemyIntentAndMotion,
    Hazards,
    Combat,
    InteractionsAndPickups,
    ScheduledConsequences,
    ProjectionAndFactDrain,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerInputIntent {
    /// `[forward, right]`, with each axis in `[-1, 1]`.
    pub movement: [f32; 2],
    /// `[yaw, pitch]`, accumulated only until the next fixed tick.
    pub look_delta: [f32; 2],
    pub primary_fire_held: bool,
}

impl PlayerInputIntent {
    pub const NEUTRAL: Self = Self {
        movement: [0.0, 0.0],
        look_delta: [0.0, 0.0],
        primary_fire_held: false,
    };

    fn is_valid(self) -> bool {
        self.movement
            .into_iter()
            .chain(self.look_delta)
            .all(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerInputCommand {
    pub connection_generation: u64,
    pub sequence: u64,
    pub intent: PlayerInputIntent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GameLoopEdgeCommandKind {
    Interact { target: u64 },
    SelectWeaponSlot { slot: u8 },
    UseItem { item: String },
    SetPaused { paused: bool },
    RestartAuthoredBaseline,
    RestartCheckpoint,
    SaveGame { slot: SaveSlotId },
    LoadGame { slot: SaveSlotId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GameLoopEdgeCommand {
    pub connection_generation: u64,
    pub sequence: u64,
    pub command: GameLoopEdgeCommandKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputCommandDisposition {
    Accepted,
    Repeated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputCommandReceipt {
    pub connection_generation: u64,
    pub acknowledged_sequence: u64,
    pub consumed_sequence: u64,
    pub disposition: InputCommandDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInputSessionView {
    pub connection_generation: u64,
    pub connected: bool,
    pub paused: bool,
    pub acknowledged_sequence: u64,
    pub consumed_sequence: u64,
    pub queued_edge_commands: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputCommandRejection {
    SessionDisconnected,
    WrongConnectionGeneration { expected: u64, actual: u64 },
    StaleSequence { acknowledged: u64, actual: u64 },
    InvalidInput,
    EdgeQueueSaturated { capacity: usize },
    PlayerDefeated,
}

impl std::fmt::Display for InputCommandRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for InputCommandRejection {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeCommandRejection {
    Paused,
    UnknownTarget,
    NotInteractable,
    PickupRejected,
    InvalidWeaponSlot,
    WeaponNotOwned,
    WeaponAlreadySelected,
    PlayerDefeated,
    InventoryRejected,
    ItemNotOwned,
    ItemNotUsable,
    HealthFull,
    CheckpointUnavailable,
    DoorLocked,
    LevelExitUnavailable,
    LevelComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameRestartMode {
    AuthoredBaseline,
    Checkpoint,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GameLoopFact {
    PlayerControl(PlayerControlFact),
    Navigation(NavigationFact),
    EnemyCombat(EnemyCombatFact),
    Combat(CombatFact),
    ExtractionBeacon(ExtractionBeaconFact),
    Pickup(PickupFact),
    Inventory(InventoryFact),
    Vitality(VitalityFact),
    Hazard(HazardFact),
    Progression(crate::ProgressionFact),
    DoorAccessRejected {
        sequence: u64,
        door: EntityId,
        required_key: ItemDefinitionId,
        presentation: String,
    },
    PickupRejected {
        pickup: EntityId,
        reason: PickupRejection,
    },
    Event(GameEvent),
    CombatRejected {
        attacker: EntityId,
        weapon: Option<ItemDefinitionId>,
        presentation: Option<String>,
        reason: CombatRejectionReason,
    },
    EdgeCommandRejected {
        sequence: u64,
        reason: EdgeCommandRejection,
    },
    RestartRequested {
        sequence: u64,
        mode: GameRestartMode,
    },
    SaveRequested {
        sequence: u64,
        slot: SaveSlotId,
    },
    LoadRequested {
        sequence: u64,
        slot: SaveSlotId,
    },
    InputExpired {
        sequence: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameLoopTickReceipt {
    pub driver_tick: u64,
    pub simulation_tick: u64,
    pub simulation_advanced: bool,
    pub phases: [GameLoopPhase; 8],
    pub acknowledged_sequence: u64,
    pub consumed_sequence: u64,
    pub facts: Vec<GameLoopFact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameLoopAdvanceReceipt {
    pub fixed_ticks: Vec<GameLoopTickReceipt>,
    pub dropped_ticks: u64,
}

#[derive(Debug, Clone)]
struct QueuedEdgeCommand {
    sequence: u64,
    command: GameLoopEdgeCommandKind,
}

#[derive(Debug)]
struct PlayerInputSession {
    connection_generation: u64,
    connected: bool,
    paused: bool,
    acknowledged_sequence: u64,
    consumed_sequence: u64,
    retained_sequences: VecDeque<u64>,
    movement: [f32; 2],
    accumulated_look: [f32; 2],
    primary_fire_held: bool,
    primary_fire_pressed: bool,
    last_input_driver_tick: Option<u64>,
    edge_commands: VecDeque<QueuedEdgeCommand>,
}

impl Default for PlayerInputSession {
    fn default() -> Self {
        Self {
            connection_generation: 0,
            connected: false,
            paused: false,
            acknowledged_sequence: 0,
            consumed_sequence: 0,
            retained_sequences: VecDeque::new(),
            movement: PlayerInputIntent::NEUTRAL.movement,
            accumulated_look: PlayerInputIntent::NEUTRAL.look_delta,
            primary_fire_held: false,
            primary_fire_pressed: false,
            last_input_driver_tick: None,
            edge_commands: VecDeque::new(),
        }
    }
}

impl PlayerInputSession {
    fn view(&self) -> PlayerInputSessionView {
        PlayerInputSessionView {
            connection_generation: self.connection_generation,
            connected: self.connected,
            paused: self.paused,
            acknowledged_sequence: self.acknowledged_sequence,
            consumed_sequence: self.consumed_sequence,
            queued_edge_commands: self.edge_commands.len(),
        }
    }

    fn clear_intent(&mut self) {
        self.movement = PlayerInputIntent::NEUTRAL.movement;
        self.accumulated_look = PlayerInputIntent::NEUTRAL.look_delta;
        self.primary_fire_held = false;
        self.primary_fire_pressed = false;
        self.last_input_driver_tick = None;
    }

    fn clear_generation(&mut self) {
        self.clear_intent();
        self.acknowledged_sequence = 0;
        self.consumed_sequence = 0;
        self.retained_sequences.clear();
        self.edge_commands.clear();
        self.paused = false;
    }

    fn validate_envelope(
        &self,
        connection_generation: u64,
        sequence: u64,
    ) -> Result<InputCommandDisposition, InputCommandRejection> {
        if !self.connected {
            return Err(InputCommandRejection::SessionDisconnected);
        }
        if connection_generation != self.connection_generation {
            return Err(InputCommandRejection::WrongConnectionGeneration {
                expected: self.connection_generation,
                actual: connection_generation,
            });
        }
        if self.retained_sequences.contains(&sequence) {
            return Ok(InputCommandDisposition::Repeated);
        }
        if sequence == 0 || sequence <= self.acknowledged_sequence {
            return Err(InputCommandRejection::StaleSequence {
                acknowledged: self.acknowledged_sequence,
                actual: sequence,
            });
        }
        Ok(InputCommandDisposition::Accepted)
    }

    fn accept_sequence(&mut self, sequence: u64) {
        self.acknowledged_sequence = sequence;
        self.retained_sequences.push_back(sequence);
        while self.retained_sequences.len() > MAX_RETAINED_COMMAND_SEQUENCES {
            self.retained_sequences.pop_front();
        }
    }

    fn receipt(&self, disposition: InputCommandDisposition) -> InputCommandReceipt {
        InputCommandReceipt {
            connection_generation: self.connection_generation,
            acknowledged_sequence: self.acknowledged_sequence,
            consumed_sequence: self.consumed_sequence,
            disposition,
        }
    }
}

/// Downstream Loading Bay authority for transient input, fixed-step scheduling,
/// and the game's explicit phase order. `GameRuntime` remains the durable
/// world/session owner and Rusty Engine remains a mechanism provider.
#[derive(Debug)]
pub struct LoadingBayGameLoop {
    runtime: GameRuntime,
    player: EntityId,
    input: PlayerInputSession,
    accumulator: Duration,
    driver_tick: u64,
    pending_facts: VecDeque<GameLoopFact>,
    dropped_fact_count: u64,
}

impl LoadingBayGameLoop {
    pub fn validate_runtime(runtime: &GameRuntime, player: EntityId) -> Result<(), RuntimeError> {
        if runtime.session().player_controller(player).is_none() {
            return Err(RuntimeError::UnknownPlayerController { player });
        }
        if runtime.session().health(player).is_none()
            && runtime.session().hazards().next().is_some()
        {
            return Err(RuntimeError::HazardPlayerMissingVitality { player });
        }
        if runtime.session().health(player).is_none()
            && runtime.session().enemy_combatants().next().is_some()
        {
            return Err(RuntimeError::EnemyCombatPlayerMissingVitality { player });
        }
        Ok(())
    }

    pub fn new(runtime: GameRuntime, player: EntityId) -> Result<Self, RuntimeError> {
        Self::validate_runtime(&runtime, player)?;
        Ok(Self {
            runtime,
            player,
            input: PlayerInputSession::default(),
            accumulator: Duration::ZERO,
            driver_tick: 0,
            pending_facts: VecDeque::new(),
            dropped_fact_count: 0,
        })
    }

    pub fn runtime(&self) -> &GameRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut GameRuntime {
        &mut self.runtime
    }

    pub fn into_runtime(self) -> GameRuntime {
        self.runtime
    }

    pub fn input_session(&self) -> PlayerInputSessionView {
        self.input.view()
    }

    pub fn start_connection(&mut self) -> PlayerInputSessionView {
        self.input.connection_generation =
            self.input.connection_generation.saturating_add(1).max(1);
        self.input.clear_generation();
        self.input.connected = true;
        self.input.view()
    }

    pub fn start_connection_after(&mut self, previous_generation: u64) -> PlayerInputSessionView {
        self.input.connection_generation = previous_generation;
        self.start_connection()
    }

    pub fn disconnect(&mut self, connection_generation: u64) -> bool {
        if !self.input.connected || self.input.connection_generation != connection_generation {
            return false;
        }
        self.input.connected = false;
        self.input.clear_intent();
        self.input.edge_commands.clear();
        true
    }

    pub fn submit_input(
        &mut self,
        command: PlayerInputCommand,
    ) -> Result<InputCommandReceipt, InputCommandRejection> {
        if !command.intent.is_valid() {
            return Err(InputCommandRejection::InvalidInput);
        }
        if DamageService::is_dead(self.runtime.session(), self.player) {
            return Err(InputCommandRejection::PlayerDefeated);
        }
        let disposition = self
            .input
            .validate_envelope(command.connection_generation, command.sequence)?;
        if disposition == InputCommandDisposition::Repeated {
            return Ok(self.input.receipt(disposition));
        }
        self.input.accept_sequence(command.sequence);
        self.input.movement = command.intent.movement;
        for (accumulated, delta) in self
            .input
            .accumulated_look
            .iter_mut()
            .zip(command.intent.look_delta)
        {
            *accumulated = (*accumulated + delta)
                .clamp(-MAX_ACCUMULATED_LOOK_UNITS, MAX_ACCUMULATED_LOOK_UNITS);
        }
        if !self.input.primary_fire_held && command.intent.primary_fire_held {
            self.input.primary_fire_pressed = true;
        }
        self.input.primary_fire_held = command.intent.primary_fire_held;
        self.input.last_input_driver_tick = Some(self.driver_tick);
        Ok(self.input.receipt(disposition))
    }

    pub fn submit_edge_command(
        &mut self,
        command: GameLoopEdgeCommand,
    ) -> Result<InputCommandReceipt, InputCommandRejection> {
        if DamageService::is_dead(self.runtime.session(), self.player)
            && !matches!(
                &command.command,
                GameLoopEdgeCommandKind::SetPaused { .. }
                    | GameLoopEdgeCommandKind::RestartAuthoredBaseline
                    | GameLoopEdgeCommandKind::RestartCheckpoint
                    | GameLoopEdgeCommandKind::SaveGame { .. }
                    | GameLoopEdgeCommandKind::LoadGame { .. }
            )
        {
            return Err(InputCommandRejection::PlayerDefeated);
        }
        let disposition = self
            .input
            .validate_envelope(command.connection_generation, command.sequence)?;
        if disposition == InputCommandDisposition::Repeated {
            return Ok(self.input.receipt(disposition));
        }
        if self.input.edge_commands.len() == MAX_EDGE_COMMANDS {
            return Err(InputCommandRejection::EdgeQueueSaturated {
                capacity: MAX_EDGE_COMMANDS,
            });
        }
        self.input.accept_sequence(command.sequence);
        self.input.edge_commands.push_back(QueuedEdgeCommand {
            sequence: command.sequence,
            command: command.command,
        });
        Ok(self.input.receipt(disposition))
    }

    pub fn advance_elapsed(
        &mut self,
        elapsed: Duration,
    ) -> Result<GameLoopAdvanceReceipt, RuntimeError> {
        self.accumulator = self.accumulator.saturating_add(elapsed);
        let fixed_nanos = FIXED_STEP_DURATION.as_nanos();
        let due = self.accumulator.as_nanos() / fixed_nanos;
        let steps = due.min(MAX_CATCH_UP_TICKS as u128) as usize;
        let dropped_ticks = due.saturating_sub(steps as u128).min(u64::MAX as u128) as u64;
        let remainder = self.accumulator.as_nanos() % fixed_nanos;
        if dropped_ticks > 0 {
            self.accumulator = FIXED_STEP_DURATION
                .saturating_mul(steps as u32)
                .saturating_add(Duration::from_nanos(remainder as u64));
        }
        let mut fixed_ticks = Vec::with_capacity(steps);
        for _ in 0..steps {
            self.accumulator = self.accumulator.saturating_sub(FIXED_STEP_DURATION);
            let tick = self.run_fixed_tick()?;
            let requires_host_interleave = tick.facts.iter().any(|fact| {
                matches!(
                    fact,
                    GameLoopFact::RestartRequested { .. }
                        | GameLoopFact::SaveRequested { .. }
                        | GameLoopFact::LoadRequested { .. }
                )
            });
            fixed_ticks.push(tick);
            // Host-owned persistence/session replacement must observe the
            // command-consumption tick before any retained catch-up debt runs.
            if requires_host_interleave {
                break;
            }
        }
        Ok(GameLoopAdvanceReceipt {
            fixed_ticks,
            dropped_ticks,
        })
    }

    pub fn run_fixed_tick(&mut self) -> Result<GameLoopTickReceipt, RuntimeError> {
        self.driver_tick = self.driver_tick.saturating_add(1);
        let mut facts = Vec::new();
        let mut interactions = Vec::new();
        self.consume_input_phase(&mut facts, &mut interactions)?;

        let level_complete = self.runtime.is_level_complete();
        let simulation_advanced = !self.input.paused && !level_complete;
        if simulation_advanced {
            self.runtime.begin_fixed_tick();
            self.run_player_motion_phase(&mut facts)?;
            self.run_enemy_phase(&mut facts)?;
            self.run_hazard_phase(&mut facts)?;
            self.run_combat_phase(&mut facts)?;
            self.run_interaction_and_pickup_phase(interactions, &mut facts)?;
            let events = self.runtime.run_scheduled_consequence_phase()?;
            facts.extend(events.into_iter().map(GameLoopFact::Event));
        } else if level_complete {
            for command in interactions {
                if !extend_session_operation_fact(&command, &mut facts) {
                    facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::LevelComplete,
                    });
                }
            }
        } else {
            for command in interactions {
                if !extend_session_operation_fact(&command, &mut facts) {
                    facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::Paused,
                    });
                }
            }
        }

        for fact in &facts {
            self.push_pending_fact(fact.clone());
        }
        Ok(GameLoopTickReceipt {
            driver_tick: self.driver_tick,
            simulation_tick: self.runtime.tick().raw(),
            simulation_advanced,
            phases: FIXED_TICK_PHASE_ORDER,
            acknowledged_sequence: self.input.acknowledged_sequence,
            consumed_sequence: self.input.consumed_sequence,
            facts,
        })
    }

    pub fn drain_pending_facts(&mut self) -> Vec<GameLoopFact> {
        self.pending_facts.drain(..).collect()
    }

    pub fn dropped_fact_count(&self) -> u64 {
        self.dropped_fact_count
    }

    fn consume_input_phase(
        &mut self,
        facts: &mut Vec<GameLoopFact>,
        interactions: &mut Vec<QueuedEdgeCommand>,
    ) -> Result<(), RuntimeError> {
        if self.input.connected
            && self
                .input
                .last_input_driver_tick
                .is_some_and(|accepted_at| {
                    self.driver_tick.saturating_sub(accepted_at) > MAX_INPUT_AGE_TICKS
                })
        {
            let sequence = self.input.acknowledged_sequence;
            self.input.clear_intent();
            facts.push(GameLoopFact::InputExpired { sequence });
        }

        while let Some(command) = self.input.edge_commands.pop_front() {
            self.input.consumed_sequence = self.input.consumed_sequence.max(command.sequence);
            match &command.command {
                GameLoopEdgeCommandKind::SetPaused { paused } => {
                    self.input.paused = *paused;
                    self.input.clear_intent();
                }
                GameLoopEdgeCommandKind::RestartAuthoredBaseline
                | GameLoopEdgeCommandKind::RestartCheckpoint
                | GameLoopEdgeCommandKind::SaveGame { .. }
                | GameLoopEdgeCommandKind::LoadGame { .. }
                | GameLoopEdgeCommandKind::Interact { .. }
                | GameLoopEdgeCommandKind::SelectWeaponSlot { .. }
                | GameLoopEdgeCommandKind::UseItem { .. } => interactions.push(command),
            }
        }
        self.input.consumed_sequence = self
            .input
            .consumed_sequence
            .max(self.input.acknowledged_sequence);

        if self.input.paused || !self.input.connected || self.runtime.is_level_complete() {
            self.input.accumulated_look = PlayerInputIntent::NEUTRAL.look_delta;
            return Ok(());
        }
        let [yaw_delta, pitch_delta] = std::mem::replace(
            &mut self.input.accumulated_look,
            PlayerInputIntent::NEUTRAL.look_delta,
        );
        if yaw_delta != 0.0 || pitch_delta != 0.0 {
            let receipt = self.runtime.apply_player_action(
                self.player,
                crate::ResolvedPlayerAction::Look {
                    yaw_delta,
                    pitch_delta,
                },
            )?;
            facts.extend(receipt.facts.into_iter().map(GameLoopFact::PlayerControl));
        }
        Ok(())
    }

    fn run_player_motion_phase(
        &mut self,
        facts: &mut Vec<GameLoopFact>,
    ) -> Result<(), RuntimeError> {
        if !self.input.connected {
            return Ok(());
        }
        let [forward, right] = self.input.movement;
        if forward == 0.0 && right == 0.0 {
            return Ok(());
        }
        let receipt = self.runtime.integrate_player_motion(
            self.player,
            forward,
            right,
            FIXED_STEP_SECONDS,
        )?;
        facts.extend(receipt.facts.into_iter().map(GameLoopFact::PlayerControl));
        Ok(())
    }

    fn run_enemy_phase(&mut self, facts: &mut Vec<GameLoopFact>) -> Result<(), RuntimeError> {
        let activation_events = self.runtime.run_encounter_activation_phase(self.player)?;
        facts.extend(activation_events.into_iter().map(GameLoopFact::Event));
        let receipt = self
            .runtime
            .run_enemy_intent_and_motion_phase(self.player, FIXED_STEP_SECONDS)?;
        facts.extend(receipt.facts.into_iter().map(GameLoopFact::EnemyCombat));
        facts.extend(
            receipt
                .navigation
                .facts
                .into_iter()
                .map(GameLoopFact::Navigation),
        );
        Ok(())
    }

    fn run_combat_phase(&mut self, facts: &mut Vec<GameLoopFact>) -> Result<(), RuntimeError> {
        let enemy_attacks = self.runtime.run_enemy_attack_phase(self.player)?;
        facts.extend(
            enemy_attacks
                .facts
                .into_iter()
                .map(GameLoopFact::EnemyCombat),
        );
        facts.extend(enemy_attacks.events.into_iter().map(GameLoopFact::Event));
        let projectiles = self.runtime.run_projectile_phase(FIXED_STEP_SECONDS)?;
        facts.extend(
            projectiles
                .facts
                .into_iter()
                .map(|fact| GameLoopFact::Combat(projectile_fact_to_combat_fact(fact))),
        );
        facts.extend(projectiles.combat.into_iter().map(GameLoopFact::Combat));
        facts.extend(projectiles.events.into_iter().map(GameLoopFact::Event));
        if DamageService::is_dead(self.runtime.session(), self.player) {
            self.input.clear_intent();
            return Ok(());
        }
        let primary_fire_pressed = std::mem::take(&mut self.input.primary_fire_pressed);
        if !self.input.connected || DamageService::is_dead(self.runtime.session(), self.player) {
            return Ok(());
        }
        let automatic_held = self.input.primary_fire_held
            && self
                .runtime
                .session()
                .weapon(self.player)
                .is_some_and(|weapon| weapon.definition.attack_mode.is_automatic());
        if !primary_fire_pressed && !automatic_held {
            return Ok(());
        }
        let equipped = self.runtime.session().weapon(self.player);
        let weapon = equipped.as_ref().map(|weapon| weapon.item.clone());
        let presentation = equipped.map(|weapon| weapon.definition.presentation);
        match self
            .runtime
            .attack(self.player, ResolvedAttackAction::Attack)
        {
            Ok(receipt) => {
                facts.extend(receipt.facts.into_iter().map(GameLoopFact::Combat));
                facts.extend(receipt.events.into_iter().map(GameLoopFact::Event));
                Ok(())
            }
            Err(RuntimeError::CombatRejected {
                entity: attacker,
                reason,
            }) => {
                facts.push(GameLoopFact::CombatRejected {
                    attacker,
                    weapon,
                    presentation,
                    reason,
                });
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn run_hazard_phase(&mut self, facts: &mut Vec<GameLoopFact>) -> Result<(), RuntimeError> {
        let receipt = self.runtime.run_hazard_phase(self.player)?;
        facts.extend(receipt.facts.into_iter().map(GameLoopFact::Hazard));
        facts.extend(receipt.events.into_iter().map(GameLoopFact::Event));
        if DamageService::is_dead(self.runtime.session(), self.player) {
            self.input.clear_intent();
        }
        Ok(())
    }

    fn run_interaction_and_pickup_phase(
        &mut self,
        interactions: Vec<QueuedEdgeCommand>,
        facts: &mut Vec<GameLoopFact>,
    ) -> Result<(), RuntimeError> {
        if DamageService::is_dead(self.runtime.session(), self.player) {
            self.input.clear_intent();
            for command in interactions {
                if !extend_session_operation_fact(&command, facts) {
                    facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::PlayerDefeated,
                    });
                }
            }
            return Ok(());
        }
        let pickup_phase = self.runtime.run_pickup_phase(self.player)?;
        for receipt in pickup_phase.collected {
            extend_pickup_facts(receipt, facts);
        }
        facts.extend(pickup_phase.rejected.into_iter().map(|attempt| {
            GameLoopFact::PickupRejected {
                pickup: attempt.pickup,
                reason: attempt.reason,
            }
        }));
        let secret_phase = self.runtime.run_secret_phase(self.player)?;
        facts.extend(
            secret_phase
                .facts
                .into_iter()
                .map(GameLoopFact::Progression),
        );
        for command in interactions {
            if extend_session_operation_fact(&command, facts) {
                continue;
            }
            if let GameLoopEdgeCommandKind::SelectWeaponSlot { slot } = &command.command {
                match InventoryService::select_weapon_slot(
                    &mut self.runtime.session,
                    self.player,
                    usize::from(*slot),
                ) {
                    Ok(receipt) => {
                        facts.extend(receipt.facts.into_iter().map(GameLoopFact::Inventory));
                    }
                    Err(rejection) => {
                        let reason = match rejection {
                            InventoryRejection::InvalidWeaponSlot { .. }
                            | InventoryRejection::MissingDefinition { .. }
                            | InventoryRejection::IncompatibleSelection { .. } => {
                                EdgeCommandRejection::InvalidWeaponSlot
                            }
                            InventoryRejection::WeaponNotOwned { .. } => {
                                EdgeCommandRejection::WeaponNotOwned
                            }
                            InventoryRejection::AlreadySelected { .. } => {
                                EdgeCommandRejection::WeaponAlreadySelected
                            }
                            InventoryRejection::OwnerDefeated { .. } => {
                                EdgeCommandRejection::PlayerDefeated
                            }
                            _ => EdgeCommandRejection::InventoryRejected,
                        };
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason,
                        });
                    }
                }
                continue;
            }
            if let GameLoopEdgeCommandKind::UseItem { item } = &command.command {
                let item = match ItemDefinitionId::parse(item.clone()) {
                    Ok(item) => item,
                    Err(_) => {
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::ItemNotUsable,
                        });
                        continue;
                    }
                };
                match DamageService::use_health_supply(&mut self.runtime.session, self.player, item)
                {
                    Ok(receipt) => {
                        facts.extend(receipt.facts.into_iter().map(GameLoopFact::Vitality));
                        facts.extend(
                            receipt
                                .inventory
                                .into_iter()
                                .flat_map(|receipt| receipt.facts)
                                .map(GameLoopFact::Inventory),
                        );
                    }
                    Err(rejection) => {
                        let reason = match rejection {
                            VitalityRejection::PlayerDead { .. } => {
                                EdgeCommandRejection::PlayerDefeated
                            }
                            VitalityRejection::HealthFull { .. } => {
                                EdgeCommandRejection::HealthFull
                            }
                            VitalityRejection::ItemNotOwned { .. }
                            | VitalityRejection::Inventory(
                                InventoryRejection::QuantityUnderflow { .. },
                            ) => EdgeCommandRejection::ItemNotOwned,
                            _ => EdgeCommandRejection::ItemNotUsable,
                        };
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason,
                        });
                    }
                }
                continue;
            }
            let GameLoopEdgeCommandKind::Interact { target } = command.command else {
                continue;
            };
            let target = EntityId::new(target);
            if self.runtime.session().pickup(target).is_some() {
                match self.runtime.collect_pickup(
                    self.player,
                    target,
                    self.input.connection_generation,
                    command.sequence,
                ) {
                    Ok(receipt) => extend_pickup_facts(receipt, facts),
                    Err(RuntimeError::Pickup(reason)) => {
                        facts.push(GameLoopFact::PickupRejected {
                            pickup: target,
                            reason,
                        });
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::PickupRejected,
                        });
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            if self.runtime.session().extraction_beacon(target).is_some() {
                match self.runtime.activate_extraction_beacon(self.player, target) {
                    Ok(receipt) => {
                        facts.push(GameLoopFact::ExtractionBeacon(receipt.fact));
                    }
                    Err(RuntimeError::ExtractionBeaconAlreadyActive { .. })
                    | Err(RuntimeError::ExtractionBeaconOutOfRange { .. }) => {
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::NotInteractable,
                        });
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            if self.runtime.session().door_access(target).is_some() {
                match self.runtime.open_keyed_door(self.player, target) {
                    Ok((receipt, events)) => {
                        if let Some(inventory) = receipt.inventory {
                            facts.extend(inventory.facts.into_iter().map(GameLoopFact::Inventory));
                        }
                        if let Some(fact) = receipt.fact {
                            facts.push(GameLoopFact::Progression(fact));
                        }
                        facts.extend(events.into_iter().map(GameLoopFact::Event));
                    }
                    Err(RuntimeError::DoorAccess(
                        crate::DoorAccessRejection::MissingRequiredKey {
                            door,
                            required_key,
                            presentation,
                        },
                    )) => {
                        facts.push(GameLoopFact::DoorAccessRejected {
                            sequence: command.sequence,
                            door,
                            required_key,
                            presentation,
                        });
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::DoorLocked,
                        });
                    }
                    Err(RuntimeError::DoorAccess(crate::DoorAccessRejection::OutOfRange {
                        ..
                    })) => {
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::NotInteractable,
                        });
                    }
                    Err(RuntimeError::DoorAccess(crate::DoorAccessRejection::PlayerDefeated {
                        ..
                    })) => {
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::PlayerDefeated,
                        });
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            if self
                .runtime
                .session()
                .loading_bay_interlock(target)
                .is_some()
            {
                match self
                    .runtime
                    .activate_loading_bay_interlock(self.player, target)
                {
                    Ok(receipt) => {
                        facts.extend(receipt.events.into_iter().map(GameLoopFact::Event));
                    }
                    Err(RuntimeError::LoadingBayInterlock(
                        crate::LoadingBayInterlockRejection::OutOfRange { .. },
                    )) => facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::NotInteractable,
                    }),
                    Err(RuntimeError::LoadingBayInterlock(
                        crate::LoadingBayInterlockRejection::PlayerDefeated { .. },
                    )) => facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::PlayerDefeated,
                    }),
                    Err(RuntimeError::LoadingBayInterlock(_)) => {
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::NotInteractable,
                        })
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            if self.runtime.session().level_exit(target).is_some() {
                match self.runtime.complete_level(self.player, target) {
                    Ok(Some(fact)) => facts.push(GameLoopFact::Progression(fact)),
                    Ok(None) => facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::LevelComplete,
                    }),
                    Err(RuntimeError::LevelExit(crate::LevelExitRejection::OutOfRange {
                        ..
                    })) => facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::NotInteractable,
                    }),
                    Err(RuntimeError::LevelExit(crate::LevelExitRejection::PlayerDefeated {
                        ..
                    })) => facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::PlayerDefeated,
                    }),
                    Err(RuntimeError::LevelExit(_)) => {
                        facts.push(GameLoopFact::EdgeCommandRejected {
                            sequence: command.sequence,
                            reason: EdgeCommandRejection::LevelExitUnavailable,
                        })
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            match self.runtime.interact(self.player, target) {
                Ok(receipt) => {
                    facts.extend(receipt.events.into_iter().map(GameLoopFact::Event));
                }
                Err(RuntimeError::NotInteractable { .. }) => {
                    facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::NotInteractable,
                    });
                }
                Err(RuntimeError::UnknownDoor { .. })
                | Err(RuntimeError::UnknownEnemy { .. })
                | Err(RuntimeError::UnknownActor { .. }) => {
                    facts.push(GameLoopFact::EdgeCommandRejected {
                        sequence: command.sequence,
                        reason: EdgeCommandRejection::UnknownTarget,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn push_pending_fact(&mut self, fact: GameLoopFact) {
        if self.pending_facts.len() == MAX_PENDING_GAME_LOOP_FACTS {
            self.pending_facts.pop_front();
            self.dropped_fact_count = self.dropped_fact_count.saturating_add(1);
        }
        self.pending_facts.push_back(fact);
    }
}

fn projectile_fact_to_combat_fact(fact: crate::ProjectileFact) -> CombatFact {
    match fact {
        crate::ProjectileFact::Spawned {
            entity,
            owner,
            weapon,
            origin,
            impulse,
            expires_at,
        } => CombatFact::ProjectileSpawned {
            entity,
            owner,
            weapon,
            origin,
            impulse,
            expires_at,
        },
        crate::ProjectileFact::Impacted {
            entity,
            owner,
            target,
            position,
            damage,
        } => CombatFact::ProjectileImpacted {
            entity,
            owner,
            target,
            position,
            damage,
        },
        crate::ProjectileFact::Expired {
            entity,
            owner,
            position,
        } => CombatFact::ProjectileExpired {
            entity,
            owner,
            position,
        },
    }
}

fn extend_session_operation_fact(
    command: &QueuedEdgeCommand,
    facts: &mut Vec<GameLoopFact>,
) -> bool {
    let fact = match command.command {
        GameLoopEdgeCommandKind::RestartAuthoredBaseline => GameLoopFact::RestartRequested {
            sequence: command.sequence,
            mode: GameRestartMode::AuthoredBaseline,
        },
        GameLoopEdgeCommandKind::RestartCheckpoint => GameLoopFact::EdgeCommandRejected {
            sequence: command.sequence,
            reason: EdgeCommandRejection::CheckpointUnavailable,
        },
        GameLoopEdgeCommandKind::SaveGame { slot } => GameLoopFact::SaveRequested {
            sequence: command.sequence,
            slot,
        },
        GameLoopEdgeCommandKind::LoadGame { slot } => GameLoopFact::LoadRequested {
            sequence: command.sequence,
            slot,
        },
        _ => return false,
    };
    facts.push(fact);
    true
}

fn extend_pickup_facts(receipt: PickupReceipt, facts: &mut Vec<GameLoopFact>) {
    facts.extend(receipt.facts.into_iter().map(GameLoopFact::Pickup));
    facts.extend(
        receipt
            .vitality_facts
            .into_iter()
            .map(GameLoopFact::Vitality),
    );
}
