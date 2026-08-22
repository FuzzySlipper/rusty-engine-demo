use std::collections::BTreeMap;

use rusty_engine::core_ids::EntityId;
use rusty_engine::entity_state::{
    ComponentRegistry, EntityAuthoringService, EntityDefinition, EntityState, RelationshipCommand,
};
use rusty_engine::gameplay_mechanics::{
    register_gameplay_components, ActiveEffectsComponent, CatalogVersion, DamageKindDefinition,
    DamageKindId, DamageKindSelector, DamageResponseDefinition, EffectDefinition,
    EffectDefinitionId, EffectInstanceId, EffectStackingPolicy, EquipmentAssignment,
    EquipmentComponent, EquipmentExclusivityId, EquipmentSlotDefinition, EquipmentSlotId,
    InventoryComponent, ItemClassificationId, ItemComponent, ItemDefinition as MechanicsItem,
    ItemEquipmentPolicy, ItemKind as MechanicsItemKind, ItemStack as MechanicsStack,
    MechanicsCatalog, MechanicsCatalogDefinition, MechanicsScalar, SourceDefinition,
    SourceDefinitionId, StackingGroupId, StatDefinition, StatId, StatValue, StatsComponent,
    TrackDefinition, TrackId, TrackMaximum, TrackValue, TracksComponent,
};
use rusty_engine::gameplay_standard::{
    compose_action_actor_and_destructible_resource_catalog, ActionActorPreset,
    ActionActorPresetConfig, DestructibleResourcePreset, DestructibleResourcePresetConfig,
};

use crate::inventory::{InventoryConfig, ItemDefinition, ItemDefinitionId, ItemKind};
use crate::vitality::HealthConfig;

pub(crate) const CATALOG_VERSION: &str = "loading-bay-v1";
pub(crate) const ARMOR_STAT: &str = "max-armor";
pub(crate) const ARMOR_TRACK: &str = "armor";
pub(crate) const DIRECT_DAMAGE: &str = "direct";
pub(crate) const ARMOR_ELIGIBLE_DAMAGE: &str = "armor-eligible";
pub(crate) const WEAPON_SLOT: &str = "weapon";
const WEAPON_CLASSIFICATION: &str = "weapon";
const WEAPON_EXCLUSIVITY: &str = "weapon";
const ARMOR_STACKING_GROUP: &str = "armor";
pub(crate) const ARMOR_EFFECT_INSTANCE: &str = "armor";

#[derive(Debug, Clone)]
pub(crate) struct ArmorMechanics {
    pub effect: EffectDefinitionId,
}

#[derive(Debug, Clone)]
pub(crate) struct MechanicsRuntime {
    pub catalog: MechanicsCatalog,
    pub armor: BTreeMap<ItemDefinitionId, ArmorMechanics>,
}

/// Which public standard preset supplies the authoritative vitality track for
/// one Demo entity. Doom still owns the semantic translation around that
/// track (damage source, armor, hitbox, and consequences).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VitalityPreset {
    ActionActor,
    DestructibleObject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryRuntime {
    pub capacity_slots: usize,
    pub stack_order: Vec<ItemDefinitionId>,
    pub weapon_slots: Vec<ItemDefinitionId>,
    pub weapon_entities: BTreeMap<ItemDefinitionId, EntityId>,
    pub weapon_ready_at: BTreeMap<ItemDefinitionId, rusty_engine::core_time::Tick>,
    pub last_applied_command_sequence: Option<u64>,
}

pub(crate) type WeaponEntityMappings = BTreeMap<EntityId, BTreeMap<ItemDefinitionId, EntityId>>;

pub(crate) fn mechanics_registry() -> Result<ComponentRegistry, String> {
    let mut registry = ComponentRegistry::default();
    register_gameplay_components(&mut registry).map_err(|error| error.to_string())?;
    Ok(registry)
}

pub(crate) fn build_runtime(
    definitions: &BTreeMap<ItemDefinitionId, ItemDefinition>,
    vitality_policy: crate::DoomVitalityPolicy,
    destructible_integrity_maximum: u32,
) -> Result<MechanicsRuntime, String> {
    let version = catalog_version();
    let maximum_armor = MechanicsScalar::new(i64::from(vitality_policy.maximum_armor))
        .map_err(|error| error.to_string())?;
    let mut sources = Vec::new();
    let mut effects = Vec::new();
    let mut armor = BTreeMap::new();
    let mut items = Vec::new();
    for (index, definition) in definitions.values().enumerate() {
        let id = mechanics_item_id(&definition.id)?;
        let (kind, classifications, equipment) = match definition.kind {
            ItemKind::Weapon(_) => (
                MechanicsItemKind::Unique,
                vec![weapon_classification()],
                Some(ItemEquipmentPolicy {
                    required_slots: 1,
                    exclusive_group: Some(
                        EquipmentExclusivityId::parse(WEAPON_EXCLUSIVITY)
                            .expect("fixed mechanics identity"),
                    ),
                }),
            ),
            _ => (MechanicsItemKind::Fungible, Vec::new(), None),
        };
        items.push(MechanicsItem {
            id,
            kind,
            maximum_quantity: u64::from(definition.max_quantity),
            classifications,
            capacity_costs: Vec::new(),
            equipment,
            sources: Vec::new(),
        });
        if matches!(definition.kind, ItemKind::Armor { .. }) {
            let source = SourceDefinitionId::parse(format!("armor-source-{index}"))
                .map_err(|error| error.to_string())?;
            let effect = EffectDefinitionId::parse(format!("armor-effect-{index}"))
                .map_err(|error| error.to_string())?;
            sources.push(SourceDefinition {
                id: source.clone(),
                priority: 0,
                stat_contributions: Vec::new(),
                damage_responses: vec![DamageResponseDefinition::Absorb {
                    selector: DamageKindSelector::Exact {
                        damage_kind: armor_eligible_damage_kind(),
                    },
                    track: armor_track(),
                }],
            });
            effects.push(EffectDefinition {
                id: effect.clone(),
                stacking_group: StackingGroupId::parse(ARMOR_STACKING_GROUP)
                    .expect("fixed mechanics identity"),
                stacking: EffectStackingPolicy::Replace,
                maximum_stacks: 1,
                sources: vec![source.clone()],
            });
            armor.insert(definition.id.clone(), ArmorMechanics { effect });
        }
    }
    // The standard actor and object presets remain ordinary catalog fragments.
    // Loading Bay supplies its admitted vitality bound and the largest authored
    // explosive prop, then deliberately composes them with its Doom armor/items.
    // There is no product registry, generic merge, or second evaluation route.
    let action_actor = action_actor_preset_config(
        vitality_policy,
        vitality_policy.maximum_health,
        vitality_policy.maximum_health,
    )?;
    let destructible_resource = destructible_resource_preset_config(
        destructible_integrity_maximum,
        destructible_integrity_maximum,
    )?;
    let definition = compose_action_actor_and_destructible_resource_catalog(
        version.clone(),
        MechanicsCatalogDefinition {
            version: version.clone(),
            stats: vec![StatDefinition {
                id: armor_stat(),
                minimum: MechanicsScalar::zero(),
                maximum: maximum_armor,
            }],
            tracks: vec![TrackDefinition {
                id: armor_track(),
                minimum: MechanicsScalar::zero(),
                maximum: TrackMaximum::Stat { stat: armor_stat() },
            }],
            sources,
            damage_kinds: vec![
                DamageKindDefinition {
                    id: direct_damage_kind(),
                },
                DamageKindDefinition {
                    id: armor_eligible_damage_kind(),
                },
            ],
            effects,
            capacity_metrics: Vec::new(),
            items,
            equipment_slots: vec![EquipmentSlotDefinition {
                id: weapon_slot(),
                allowed_classifications: vec![weapon_classification()],
            }],
        },
        Some(&action_actor),
        Some(&destructible_resource),
    )
    .map_err(|error| error.to_string())?;
    let catalog = MechanicsCatalog::admit(definition).map_err(|error| error.to_string())?;
    Ok(MechanicsRuntime { catalog, armor })
}

pub(crate) fn allocate_weapon_entities(
    definitions: &[crate::GameEntityDefinition],
    inventories: &BTreeMap<EntityId, InventoryConfig>,
) -> Result<(Vec<EntityDefinition>, WeaponEntityMappings), String> {
    let mut next = definitions
        .iter()
        .map(|definition| definition.entity.id.raw())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "weapon item entity identity overflow".to_string())?;
    let mut hidden = Vec::new();
    let mut mappings = BTreeMap::new();
    for (owner, config) in inventories {
        let mut owner_mappings = BTreeMap::new();
        for item in &config.weapon_slots {
            let entity = EntityId::new(next);
            next = next
                .checked_add(1)
                .ok_or_else(|| "weapon item entity identity overflow".to_string())?;
            let owned = config
                .starting_stacks
                .iter()
                .any(|stack| stack.item == *item && stack.quantity > 0);
            let mut definition =
                EntityDefinition::new(entity, format!("Inventory weapon {}", item.as_str()));
            if owned {
                definition = definition.with_containment(*owner);
            }
            hidden.push(definition);
            owner_mappings.insert(item.clone(), entity);
        }
        mappings.insert(*owner, owner_mappings);
    }
    Ok((hidden, mappings))
}

pub(crate) fn attach_health(
    state: &mut EntityState,
    entity: EntityId,
    config: HealthConfig,
    preset: VitalityPreset,
    vitality_policy: crate::DoomVitalityPolicy,
) -> Result<(), String> {
    let version = catalog_version();
    let (mut stats, mut tracks) = vitality_preset_values(
        version.clone(),
        config,
        config.starting,
        preset,
        vitality_policy,
    )?;
    stats.push(StatValue::new(armor_stat(), scalar(config.max_armor)?));
    attach(
        state,
        entity,
        StatsComponent::new(version.clone(), stats).map_err(|error| error.to_string())?,
    )?;
    tracks.push(TrackValue::new(armor_track(), MechanicsScalar::zero()));
    attach(
        state,
        entity,
        TracksComponent::new(version.clone(), tracks).map_err(|error| error.to_string())?,
    )?;
    attach(
        state,
        entity,
        ActiveEffectsComponent::new(version, Vec::new()).map_err(|error| error.to_string())?,
    )
}

#[allow(clippy::too_many_arguments)] // Snapshot restoration supplies both authored bounds and saved values.
pub(crate) fn attach_restored_health(
    state: &mut EntityState,
    runtime: &MechanicsRuntime,
    entity: EntityId,
    config: HealthConfig,
    current: u32,
    armor: u32,
    armor_item: Option<&ItemDefinitionId>,
    preset: VitalityPreset,
    vitality_policy: crate::DoomVitalityPolicy,
) -> Result<(), String> {
    let version = catalog_version();
    let (mut stats, mut tracks) =
        vitality_preset_values(version.clone(), config, current, preset, vitality_policy)?;
    stats.push(StatValue::new(armor_stat(), scalar(config.max_armor)?));
    attach(
        state,
        entity,
        StatsComponent::new(version.clone(), stats).map_err(|error| error.to_string())?,
    )?;
    tracks.push(TrackValue::new(armor_track(), scalar(armor)?));
    attach(
        state,
        entity,
        TracksComponent::new(version.clone(), tracks).map_err(|error| error.to_string())?,
    )?;
    let effects = armor_item
        .map(|item| {
            let binding = runtime
                .armor
                .get(item)
                .ok_or_else(|| format!("missing admitted armor effect for {item}"))?;
            rusty_engine::gameplay_mechanics::ActiveEffectInstance::new(
                armor_effect_instance(),
                binding.effect.clone(),
                rusty_engine::gameplay_mechanics::SourceInstanceIdentity::Request {
                    operation: rusty_engine::gameplay_mechanics::OperationId::parse(
                        "snapshot-migration",
                    )
                    .expect("fixed mechanics identity"),
                    instance: rusty_engine::gameplay_mechanics::SourceInstanceId::parse("armor")
                        .expect("fixed mechanics identity"),
                },
                1,
            )
            .map_err(|error| error.to_string())
        })
        .transpose()?
        .into_iter()
        .collect();
    attach(
        state,
        entity,
        ActiveEffectsComponent::new(version, effects).map_err(|error| error.to_string())?,
    )
}

pub(crate) fn attach_inventory(
    state: &mut EntityState,
    owner: EntityId,
    config: &InventoryConfig,
    weapon_entities: &BTreeMap<ItemDefinitionId, EntityId>,
) -> Result<InventoryRuntime, String> {
    let version = catalog_version();
    let stacks = config
        .starting_stacks
        .iter()
        .filter(|stack| !weapon_entities.contains_key(&stack.item))
        .map(|stack| {
            Ok(MechanicsStack {
                definition: mechanics_item_id(&stack.item)?,
                quantity: u64::from(stack.quantity),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    attach(
        state,
        owner,
        InventoryComponent::new(version.clone(), stacks).map_err(|error| error.to_string())?,
    )?;
    let assignments = config
        .initially_equipped_weapon
        .as_ref()
        .map(|item| {
            weapon_entities
                .get(item)
                .copied()
                .map(|item| EquipmentAssignment {
                    slot: weapon_slot(),
                    item,
                })
                .ok_or_else(|| format!("missing unique weapon entity for {item}"))
        })
        .transpose()?
        .into_iter()
        .collect();
    attach(
        state,
        owner,
        EquipmentComponent::new(version.clone(), assignments).map_err(|error| error.to_string())?,
    )?;
    for (item, entity) in weapon_entities {
        attach(
            state,
            *entity,
            ItemComponent::new(version.clone(), mechanics_item_id(item)?),
        )?;
    }
    Ok(InventoryRuntime {
        capacity_slots: config.capacity_slots,
        stack_order: config
            .starting_stacks
            .iter()
            .map(|stack| stack.item.clone())
            .collect(),
        weapon_slots: config.weapon_slots.clone(),
        weapon_entities: weapon_entities.clone(),
        weapon_ready_at: config
            .weapon_slots
            .iter()
            .cloned()
            .map(|item| (item, rusty_engine::core_time::Tick::ZERO))
            .collect(),
        last_applied_command_sequence: None,
    })
}

pub(crate) fn set_weapon_containment(
    state: &mut EntityState,
    weapon: EntityId,
    owner: EntityId,
) -> Result<(), String> {
    state
        .apply_relationship(
            state.revision(),
            RelationshipCommand::SetContainment {
                child: weapon,
                container: owner,
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn clear_weapon_containment(
    state: &mut EntityState,
    weapon: EntityId,
) -> Result<(), String> {
    state
        .apply_relationship(
            state.revision(),
            RelationshipCommand::ClearContainment { child: weapon },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn scalar(value: u32) -> Result<MechanicsScalar, String> {
    MechanicsScalar::new(i64::from(value)).map_err(|error| error.to_string())
}

pub(crate) fn mechanics_item_id(
    item: &ItemDefinitionId,
) -> Result<rusty_engine::gameplay_mechanics::ItemDefinitionId, String> {
    rusty_engine::gameplay_mechanics::ItemDefinitionId::parse(item.as_str().replace('/', "."))
        .map_err(|error| error.to_string())
}

pub(crate) fn armor_stat() -> StatId {
    StatId::parse(ARMOR_STAT).expect("fixed mechanics identity")
}

pub(crate) fn health_track() -> TrackId {
    TrackId::parse(ActionActorPreset::VITALITY_TRACK).expect("fixed standard preset identity")
}

pub(crate) fn destructible_integrity_track() -> TrackId {
    TrackId::parse(DestructibleResourcePreset::INTEGRITY_TRACK)
        .expect("fixed standard preset identity")
}

/// Keep established authored content on the compatibility capacity while a
/// future valid prop raises the ordinary configured catalog bound.
pub(crate) fn destructible_integrity_capacity(
    authored_maxima: impl IntoIterator<Item = u32>,
) -> u32 {
    let compatibility_capacity = u32::try_from(
        DestructibleResourcePreset::default_config()
            .integrity_maximum()
            .get(),
    )
    .expect("standard destructible integrity capacity is non-negative u32");
    authored_maxima
        .into_iter()
        .max()
        .unwrap_or(compatibility_capacity)
        .max(compatibility_capacity)
}

fn action_actor_preset_config(
    vitality_policy: crate::DoomVitalityPolicy,
    vitality_maximum: u32,
    vitality_initial: u32,
) -> Result<ActionActorPresetConfig, String> {
    let default = ActionActorPreset::default_config();
    ActionActorPresetConfig::try_new(
        scalar(vitality_policy.maximum_health)?,
        scalar(vitality_maximum)?,
        scalar(vitality_initial)?,
        default.resource_maximum(),
        default.resource_initial(),
    )
    .map_err(|error| error.to_string())
}

fn destructible_resource_preset_config(
    integrity_maximum: u32,
    integrity_initial: u32,
) -> Result<DestructibleResourcePresetConfig, String> {
    let default = DestructibleResourcePreset::default_config();
    DestructibleResourcePresetConfig::try_new(
        scalar(integrity_maximum)?,
        scalar(integrity_initial)?,
        default.resource_maximum(),
        default.resource_initial(),
    )
    .map_err(|error| error.to_string())
}

fn vitality_preset_values(
    version: CatalogVersion,
    config: HealthConfig,
    current: u32,
    preset: VitalityPreset,
    vitality_policy: crate::DoomVitalityPolicy,
) -> Result<(Vec<StatValue>, Vec<TrackValue>), String> {
    let actor = ActionActorPreset::components_with_config(
        version.clone(),
        &action_actor_preset_config(vitality_policy, config.max, current)?,
    );
    match preset {
        VitalityPreset::ActionActor => Ok((
            actor.stats.values().to_vec(),
            actor.tracks.values().to_vec(),
        )),
        VitalityPreset::DestructibleObject => {
            let destructible = DestructibleResourcePreset::components_with_config(
                version.clone(),
                &destructible_resource_preset_config(config.max, current)?,
            );
            Ok((
                actor.stats.values().to_vec(),
                destructible.tracks.values().to_vec(),
            ))
        }
    }
}

pub(crate) fn vitality_track(preset: VitalityPreset) -> TrackId {
    match preset {
        VitalityPreset::ActionActor => health_track(),
        VitalityPreset::DestructibleObject => destructible_integrity_track(),
    }
}

pub(crate) fn armor_track() -> TrackId {
    TrackId::parse(ARMOR_TRACK).expect("fixed mechanics identity")
}

pub(crate) fn direct_damage_kind() -> DamageKindId {
    DamageKindId::parse(DIRECT_DAMAGE).expect("fixed mechanics identity")
}

pub(crate) fn armor_eligible_damage_kind() -> DamageKindId {
    DamageKindId::parse(ARMOR_ELIGIBLE_DAMAGE).expect("fixed mechanics identity")
}

pub(crate) fn weapon_slot() -> EquipmentSlotId {
    EquipmentSlotId::parse(WEAPON_SLOT).expect("fixed mechanics identity")
}

pub(crate) fn armor_effect_instance() -> EffectInstanceId {
    EffectInstanceId::parse(ARMOR_EFFECT_INSTANCE).expect("fixed mechanics identity")
}

pub(crate) fn catalog_version() -> CatalogVersion {
    CatalogVersion::parse(CATALOG_VERSION).expect("fixed mechanics identity")
}

fn weapon_classification() -> ItemClassificationId {
    ItemClassificationId::parse(WEAPON_CLASSIFICATION).expect("fixed mechanics identity")
}

fn attach<T: rusty_engine::entity_state::EntityComponent>(
    state: &mut EntityState,
    entity: EntityId,
    component: T,
) -> Result<(), String> {
    let revision = state
        .component_revision::<T>(entity)
        .map_err(|error| error.to_string())?;
    EntityAuthoringService
        .attach_component(state, revision, entity, component)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_preset_fragments_are_visible_in_the_admitted_doom_catalog() {
        let runtime = build_runtime(
            &BTreeMap::new(),
            crate::DoomVitalityPolicy::doom_compatibility(),
            destructible_integrity_capacity(std::iter::empty()),
        )
        .expect("compose standard fragments");
        assert!(runtime.catalog.track(&health_track()).is_some());
        assert!(runtime
            .catalog
            .track(&TrackId::parse(ActionActorPreset::RESOURCE_TRACK).unwrap())
            .is_some());
        assert!(runtime
            .catalog
            .track(&TrackId::parse(DestructibleResourcePreset::INTEGRITY_TRACK).unwrap())
            .is_some());
        assert!(runtime
            .catalog
            .damage_kind(&DamageKindId::parse(DestructibleResourcePreset::DAMAGE_KIND).unwrap())
            .is_some());
    }

    #[test]
    fn configured_prop_capacity_changes_only_the_admitted_standard_catalog_bound() {
        let compatibility = build_runtime(
            &BTreeMap::new(),
            crate::DoomVitalityPolicy::doom_compatibility(),
            destructible_integrity_capacity(std::iter::empty()),
        )
        .expect("compose compatibility capacity");
        let raised = build_runtime(
            &BTreeMap::new(),
            crate::DoomVitalityPolicy::doom_compatibility(),
            destructible_integrity_capacity([51]),
        )
        .expect("compose raised prop capacity");
        let raised_again = build_runtime(
            &BTreeMap::new(),
            crate::DoomVitalityPolicy::doom_compatibility(),
            destructible_integrity_capacity([51]),
        )
        .expect("compose deterministic raised prop capacity");

        assert_eq!(
            compatibility
                .catalog
                .track(&destructible_integrity_track())
                .expect("compatibility integrity track")
                .maximum,
            TrackMaximum::Fixed {
                value: scalar(50).unwrap()
            }
        );
        assert_eq!(
            raised
                .catalog
                .track(&destructible_integrity_track())
                .expect("raised integrity track")
                .maximum,
            TrackMaximum::Fixed {
                value: scalar(51).unwrap()
            }
        );
        assert_ne!(
            compatibility.catalog.fingerprint(),
            raised.catalog.fingerprint()
        );
        assert_eq!(
            raised.catalog.fingerprint(),
            raised_again.catalog.fingerprint()
        );
    }
}
