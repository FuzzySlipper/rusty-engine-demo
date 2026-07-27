use std::collections::BTreeMap;

use core_ids::EntityId;
use entity_state::{
    ComponentRegistry, EntityAuthoringService, EntityDefinition, EntityState, RelationshipCommand,
};
use gameplay_mechanics::{
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

use crate::inventory::{InventoryConfig, ItemDefinition, ItemDefinitionId, ItemKind};
use crate::vitality::HealthConfig;

pub(crate) const CATALOG_VERSION: &str = "loading-bay-v1";
pub(crate) const HEALTH_STAT: &str = "max-health";
pub(crate) const ARMOR_STAT: &str = "max-armor";
pub(crate) const HEALTH_TRACK: &str = "health";
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryRuntime {
    pub capacity_slots: usize,
    pub stack_order: Vec<ItemDefinitionId>,
    pub weapon_slots: Vec<ItemDefinitionId>,
    pub weapon_entities: BTreeMap<ItemDefinitionId, EntityId>,
    pub weapon_ready_at: BTreeMap<ItemDefinitionId, core_time::Tick>,
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
) -> Result<MechanicsRuntime, String> {
    let version = catalog_version();
    let maximum = MechanicsScalar::new(i64::from(crate::vitality::MAX_HEALTH))
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
    let catalog = MechanicsCatalog::admit(MechanicsCatalogDefinition {
        version,
        stats: vec![
            StatDefinition {
                id: health_stat(),
                minimum: MechanicsScalar::zero(),
                maximum,
            },
            StatDefinition {
                id: armor_stat(),
                minimum: MechanicsScalar::zero(),
                maximum,
            },
        ],
        tracks: vec![
            TrackDefinition {
                id: health_track(),
                minimum: MechanicsScalar::zero(),
                maximum: TrackMaximum::Stat {
                    stat: health_stat(),
                },
            },
            TrackDefinition {
                id: armor_track(),
                minimum: MechanicsScalar::zero(),
                maximum: TrackMaximum::Stat { stat: armor_stat() },
            },
        ],
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
    })
    .map_err(|error| error.to_string())?;
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
) -> Result<(), String> {
    let version = catalog_version();
    attach(
        state,
        entity,
        StatsComponent::new(
            version.clone(),
            vec![
                StatValue::new(health_stat(), scalar(config.max)?),
                StatValue::new(armor_stat(), scalar(config.max_armor)?),
            ],
        )
        .map_err(|error| error.to_string())?,
    )?;
    attach(
        state,
        entity,
        TracksComponent::new(
            version.clone(),
            vec![
                TrackValue::new(health_track(), scalar(config.max)?),
                TrackValue::new(armor_track(), MechanicsScalar::zero()),
            ],
        )
        .map_err(|error| error.to_string())?,
    )?;
    attach(
        state,
        entity,
        ActiveEffectsComponent::new(version, Vec::new()).map_err(|error| error.to_string())?,
    )
}

pub(crate) fn attach_restored_health(
    state: &mut EntityState,
    runtime: &MechanicsRuntime,
    entity: EntityId,
    config: HealthConfig,
    current: u32,
    armor: u32,
    armor_item: Option<&ItemDefinitionId>,
) -> Result<(), String> {
    let version = catalog_version();
    attach(
        state,
        entity,
        StatsComponent::new(
            version.clone(),
            vec![
                StatValue::new(health_stat(), scalar(config.max)?),
                StatValue::new(armor_stat(), scalar(config.max_armor)?),
            ],
        )
        .map_err(|error| error.to_string())?,
    )?;
    attach(
        state,
        entity,
        TracksComponent::new(
            version.clone(),
            vec![
                TrackValue::new(health_track(), scalar(current)?),
                TrackValue::new(armor_track(), scalar(armor)?),
            ],
        )
        .map_err(|error| error.to_string())?,
    )?;
    let effects = armor_item
        .map(|item| {
            let binding = runtime
                .armor
                .get(item)
                .ok_or_else(|| format!("missing admitted armor effect for {item}"))?;
            gameplay_mechanics::ActiveEffectInstance::new(
                armor_effect_instance(),
                binding.effect.clone(),
                gameplay_mechanics::SourceInstanceIdentity::Request {
                    operation: gameplay_mechanics::OperationId::parse("snapshot-migration")
                        .expect("fixed mechanics identity"),
                    instance: gameplay_mechanics::SourceInstanceId::parse("armor")
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
            .map(|item| (item, core_time::Tick::ZERO))
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
) -> Result<gameplay_mechanics::ItemDefinitionId, String> {
    gameplay_mechanics::ItemDefinitionId::parse(item.as_str().replace('/', "."))
        .map_err(|error| error.to_string())
}

pub(crate) fn health_stat() -> StatId {
    StatId::parse(HEALTH_STAT).expect("fixed mechanics identity")
}

pub(crate) fn armor_stat() -> StatId {
    StatId::parse(ARMOR_STAT).expect("fixed mechanics identity")
}

pub(crate) fn health_track() -> TrackId {
    TrackId::parse(HEALTH_TRACK).expect("fixed mechanics identity")
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

fn attach<T: entity_state::EntityComponent>(
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
