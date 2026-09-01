#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const projectPath = resolve(root, 'content/projects/doom-e1m1.project.json');
const outputPath = resolve(root, 'csharp/LoadingBay.Game/E1M1SemanticCatalog.g.cs');
const expectedSha256 = '08d069726cdeaf1fddf1181eb3e75d63bad11e5262a4a5b46b4cf9a3bf5ae31b';
const check = process.argv.includes('--check');
const source = await readFile(projectPath);
const sha256 = createHash('sha256').update(source).digest('hex');
if (sha256 !== expectedSha256) throw new Error(`doom-e1m1 project SHA drifted: expected ${expectedSha256}, got ${sha256}`);
const project = JSON.parse(source);
const scene = requireOne(project.scenes, 'scene');
const nodes = scene.authoredScene?.nodes;
const entities = scene.entities;
if (!Array.isArray(nodes) || !Array.isArray(entities)) throw new Error('canonical scene must retain authored nodes and semantic entities');
assertDistinct(project.assets, 'asset', value => value.id);
assertDistinct(project.itemDefinitions, 'item definition', value => value.id);
assertDistinct(nodes, 'authored node', value => value.id);
assertDistinct(entities, 'semantic entity', value => value.id);
if (nodes.length !== entities.length) throw new Error(`authored/semantic cardinality drift: ${nodes.length}/${entities.length}`);
for (const node of nodes) if (!entities.some(entity => entity.id === node.id)) throw new Error(`authored node ${node.id} lacks a semantic entity`);
for (const entity of entities) if (!nodes.some(node => node.id === entity.id)) throw new Error(`semantic entity ${entity.id} lacks an authored node`);

const families = [
  ['Gameplay', 'gameplayPrograms', 'OperationProgram'], ['PlayerSetup', 'playerSetupPrograms', 'PlayerSetupBindings'],
  ['Pickup', 'pickupPrograms', 'OperationProgram'], ['EnemyAttack', 'enemyAttackPrograms', 'OperationProgram'],
  ['EnemyDefeat', 'enemyDefeatPrograms', 'OperationProgram'], ['Encounter', 'encounterPrograms', 'EncounterActivationAndClear'],
  ['Hazard', 'hazardPrograms', 'OperationProgram'], ['ExplosiveProp', 'explosivePropPrograms', 'OperationProgram'],
  ['FloorAction', 'floorActionPrograms', 'OperationProgram'], ['Lift', 'liftPrograms', 'OperationProgram'],
  ['Secret', 'secretPrograms', 'OperationProgram'], ['Switch', 'switchPrograms', 'OperationProgram'], ['LevelExit', 'levelExitPrograms', 'OperationProgram'],
];
for (const [, key] of families) assertDistinct(project[key], key, value => value.id);
validateReferenceClosure(project, nodes, families);
const generated = appendWorldDefinitions(render(project, nodes, entities, families), nodes, entities);
if (check) {
  if (await readFile(outputPath, 'utf8') !== generated) throw new Error('E1M1 semantic catalog is stale; run node scripts/generate-e1m1-semantic-catalog.mjs');
} else {
  await writeFile(outputPath, generated);
}

function requireOne(values, name) { if (!Array.isArray(values) || values.length !== 1) throw new Error(`expected one ${name}`); return values[0]; }
function assertDistinct(values, name, key) {
  if (!Array.isArray(values)) throw new Error(`${name} collection is missing`);
  const seen = new Set();
  for (const value of values) { const id = key(value); if (id === undefined || id === null || id === '') throw new Error(`${name} has no id`); if (seen.has(id)) throw new Error(`duplicate ${name} id ${id}`); seen.add(id); }
}
function validateReferenceClosure(project, nodes, families) {
  const itemIds = new Set(project.itemDefinitions.map(value => value.id));
  const assetIds = new Set(project.assets.map(value => value.id));
  const programIds = new Set(families.flatMap(([, key]) => project[key].map(value => value.id)));
  for (const node of nodes) {
    const asset = node.kind?.asset?.id;
    if (asset && !assetIds.has(asset)) throw new Error(`authored node ${node.id} references missing asset '${asset}'`);
  }
  const visit = (value, path = 'project') => {
    if (Array.isArray(value)) return value.forEach((entry, index) => visit(entry, `${path}[${index}]`));
    if (!value || typeof value !== 'object') return;
    for (const [key, entry] of Object.entries(value)) {
      if ((key === 'program' || key.endsWith('Program')) && typeof entry === 'string' && !programIds.has(entry)) throw new Error(`${path}.${key} references missing program '${entry}'`);
      if (key === 'item' && typeof entry === 'string' && !itemIds.has(entry)) throw new Error(`${path}.item references missing item '${entry}'`);
      visit(entry, `${path}.${key}`);
    }
  };
  visit(project);
}
function q(value) { return JSON.stringify(String(value)); }
function enumName(value) { return value === 'rangedHitscan' ? 'Hitscan' : String(value).replace(/[^a-zA-Z0-9]/g, '_').replace(/^./, char => char.toUpperCase()); }
function appendWorldDefinitions(catalog, nodes, entities) {
  const point = value => `new System.Numerics.Vector3(${value[0]}f, ${value[1]}f, ${value[2]}f)`;
  const nodeFor = entity => nodes.find(value => value.id === entity.id) ?? (() => { throw new Error(`world entity ${entity.id} lacks an authored node`); })();
  const rows = (field, render) => entities.filter(entity => entity[field]).map(render).join(',\n        ');
  const hazards = rows('hazard', (entity, index) => { const node = nodeFor(entity); return `new LoadingBayE1M1HazardDefinition(${entity.id}UL, ${index}, ${q(node.label)}, ${q(entity.hazard.program)}, ${entity.hazard.damage}, ${entity.hazard.cooldownTicks}, ${point(node.transform.translation)}, ${point(entity.bounds.min)}, ${point(entity.bounds.max)})`; });
  const barrels = rows('explosiveProp', (entity, index) => { const node = nodeFor(entity); return `new LoadingBayE1M1BarrelDefinition(${entity.id}UL, ${index}, ${q(node.label)}, ${q(entity.explosiveProp.program)}, ${entity.health.max}, ${entity.explosiveProp.damage}, ${entity.explosiveProp.radius}d, ${point(node.transform.translation)}, ${point(entity.health.hitboxHalfExtents)})`; });
  const doors = rows('door', (entity, index) => { const node = nodeFor(entity); return `new LoadingBayE1M1DoorDefinition(${entity.id}UL, ${index}, ${q(node.label)}, ${q(entity.switch.program)}, ${entity.switch.activationRadius}d, ${point(node.transform.translation)}, ${point(entity.door.openTranslation)}, ${entity.door.motionDurationTicks}, ${entity.door.autoCloseAfterTicks}, ${point(entity.bounds.min)}, ${point(entity.bounds.max)})`; });
  const floors = rows('floorAction', (entity, index) => { const node = nodeFor(entity); const floor = entity.floorAction; const platform = entities.find(value => value.id === floor.targetPlatform); return `new LoadingBayE1M1FloorDefinition(${entity.id}UL, ${index}, ${q(node.label)}, ${q(floor.program)}, ${floor.targetPlatform}UL, ${point(node.transform.translation)}, ${point(floor.upperTranslation)}, ${point(floor.loweredTranslation)}, ${floor.motionDurationTicks}, ${point(entity.bounds.min)}, ${point(entity.bounds.max)}, ${point(platform.bounds.min)}, ${point(platform.bounds.max)})`; });
  const lifts = rows('lift', (entity, index) => { const node = nodeFor(entity); const lift = entity.lift; const platform = entities.find(value => value.id === lift.targetPlatform); return `new LoadingBayE1M1LiftDefinition(${entity.id}UL, ${index}, ${q(node.label)}, ${q(lift.program)}, ${lift.targetPlatform}UL, ${point(node.transform.translation)}, ${point(lift.raisedTranslation)}, ${point(lift.loweredTranslation)}, ${lift.motionDurationTicks}, ${lift.loweredWaitTicks}, ${point(entity.bounds.min)}, ${point(entity.bounds.max)}, ${point(platform.bounds.min)}, ${point(platform.bounds.max)})`; });
  const secrets = rows('secretRegion', (entity, index) => { const node = nodeFor(entity); return `new LoadingBayE1M1SecretDefinition(${entity.id}UL, ${index}, ${q(node.label)}, ${q(entity.secretRegion.program)}, ${point(node.transform.translation)}, ${point(entity.bounds.min)}, ${point(entity.bounds.max)})`; });
  const exits = rows('levelExit', (entity, index) => { const node = nodeFor(entity); return `new LoadingBayE1M1ExitDefinition(${entity.id}UL, ${index}, ${q(node.label)}, ${q(entity.levelExit.program)}, ${entity.levelExit.activationRadius}d, ${point(node.transform.translation)})`; });
  const records = `internal sealed record LoadingBayE1M1HazardDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, int Damage, int CooldownTicks, System.Numerics.Vector3 Translation, System.Numerics.Vector3 BoundsMin, System.Numerics.Vector3 BoundsMax);\ninternal sealed record LoadingBayE1M1BarrelDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, int MaximumHealth, int Damage, double Radius, System.Numerics.Vector3 Translation, System.Numerics.Vector3 HitboxHalfExtents);\ninternal sealed record LoadingBayE1M1DoorDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, double ActivationRadius, System.Numerics.Vector3 ClosedTranslation, System.Numerics.Vector3 OpenTranslation, int MotionDurationTicks, int AutoCloseAfterTicks);\ninternal sealed record LoadingBayE1M1FloorDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, ulong PlatformEntityId, System.Numerics.Vector3 ActivationTranslation, System.Numerics.Vector3 UpperTranslation, System.Numerics.Vector3 LoweredTranslation, int MotionDurationTicks);\ninternal sealed record LoadingBayE1M1LiftDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, ulong PlatformEntityId, System.Numerics.Vector3 ActivationTranslation, System.Numerics.Vector3 RaisedTranslation, System.Numerics.Vector3 LoweredTranslation, int MotionDurationTicks, int LoweredWaitTicks);\ninternal sealed record LoadingBayE1M1SecretDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, System.Numerics.Vector3 Translation, System.Numerics.Vector3 BoundsMin, System.Numerics.Vector3 BoundsMax);\ninternal sealed record LoadingBayE1M1ExitDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, double ActivationRadius, System.Numerics.Vector3 Translation);\n\n`;
  const enrichedRecords = records
    .replace('int MotionDurationTicks, int AutoCloseAfterTicks);', 'int MotionDurationTicks, int AutoCloseAfterTicks, System.Numerics.Vector3 BoundsMin, System.Numerics.Vector3 BoundsMax);')
    .replace('System.Numerics.Vector3 LoweredTranslation, int MotionDurationTicks);', 'System.Numerics.Vector3 LoweredTranslation, int MotionDurationTicks, System.Numerics.Vector3 BoundsMin, System.Numerics.Vector3 BoundsMax, System.Numerics.Vector3 PlatformBoundsMin, System.Numerics.Vector3 PlatformBoundsMax);')
    .replace('int MotionDurationTicks, int LoweredWaitTicks);', 'int MotionDurationTicks, int LoweredWaitTicks, System.Numerics.Vector3 BoundsMin, System.Numerics.Vector3 BoundsMax, System.Numerics.Vector3 PlatformBoundsMin, System.Numerics.Vector3 PlatformBoundsMax);');
  const arrays = `    internal static readonly LoadingBayE1M1HazardDefinition[] Hazards = [\n        ${hazards}\n    ];\n    internal static readonly LoadingBayE1M1BarrelDefinition[] Barrels = [\n        ${barrels}\n    ];\n    internal static readonly LoadingBayE1M1DoorDefinition[] Doors = [\n        ${doors}\n    ];\n    internal static readonly LoadingBayE1M1FloorDefinition[] Floors = [\n        ${floors}\n    ];\n    internal static readonly LoadingBayE1M1LiftDefinition[] Lifts = [\n        ${lifts}\n    ];\n    internal static readonly LoadingBayE1M1SecretDefinition[] Secrets = [\n        ${secrets}\n    ];\n    internal static readonly LoadingBayE1M1ExitDefinition[] Exits = [\n        ${exits}\n    ];\n`;
  return catalog.replace('internal static class LoadingBayE1M1SemanticCatalog', `${enrichedRecords}internal static class LoadingBayE1M1SemanticCatalog`)
    .replace('    internal static LoadingBayE1M1PickupPlacement Pickup', `${arrays}    internal static LoadingBayE1M1PickupPlacement Pickup`);
}
function renderItem(item, sourceIndex) {
  const kind = item.kind;
  switch (kind.kind) {
    case 'ammunition': return `new LoadingBayE1M1Ammunition(${q(item.id)}, ${sourceIndex}, ${item.maxQuantity}UL)`;
    case 'armor': return `new LoadingBayE1M1Armor(${q(item.id)}, ${sourceIndex}, ${item.maxQuantity}UL, ${kind.protection}, ${kind.maximumArmor}, ${kind.absorptionDivisor}, LoadingBayE1M1ArmorGrantMode.${enumName(kind.grantMode ?? 'none')}, LoadingBayE1M1ArmorTransition.${enumName(kind.transition ?? 'none')}, ${Boolean(kind.consumeAtCap)})`;
    case 'healthSupply': return `new LoadingBayE1M1HealthSupply(${q(item.id)}, ${sourceIndex}, ${item.maxQuantity}UL, ${q(item.program)}, ${kind.restoreHealth}, ${kind.maximumHealth}, ${Boolean(kind.automaticUse)}, ${Boolean(kind.consumeAtCap)})`;
    case 'weapon': return `new LoadingBayE1M1Weapon(${q(item.id)}, ${sourceIndex}, ${item.maxQuantity}UL, ${q(item.program)}, ${q(kind.ammunition)}, ${Boolean(kind.repeatWhileHeld)}, LoadingBayE1M1WeaponAttackMode.${enumName(kind.attackMode)}, ${kind.damageRolls}, ${kind.damage}, ${kind.maxDistance}d, ${kind.cooldownTicks}, ${kind.ammunitionCost}, ${kind.pelletCount ?? 0}, ${kind.spreadDegrees ?? 0}d, ${q(kind.presentation)})`;
    default: throw new Error(`unsupported item kind '${kind.kind}'`);
  }
}
function render(project, nodes, entities, families) {
  const itemRows = project.itemDefinitions.map(renderItem);
  const programRows = families.flatMap(([family, key, shape]) => project[key].map((program, sourceIndex) => `new LoadingBayE1M1ProgramDescriptor(LoadingBayE1M1ProgramFamily.${family}, ${q(program.id)}, ${sourceIndex}, ${q(`/${key}/${sourceIndex}`)}, LoadingBayE1M1ProgramBindingShape.${shape})`));
  const dormantPickupIds = new Set(entities.flatMap(entity => entity.defeatDrop?.pickup ?? []));
  const playerSetupRows = project.playerSetupPrograms.map((program, sourceIndex) => {
    const grants = program.program.filter(step => step.kind === 'grantItem').map(step => `new LoadingBayE1M1ItemGrant(${q(step.item)}, ${step.quantity}UL)`);
    const equipped = program.program.find(step => step.kind === 'equipInitialWeapon')?.item;
    if (!equipped) throw new Error(`player setup '${program.id}' has no initial weapon`);
    return `new LoadingBayE1M1PlayerSetup(${q(program.id)}, ${sourceIndex}, [${grants.join(', ')}], ${q(equipped)})`;
  });
  const pickupRows = entities.filter(entity => entity.pickup).map(entity => {
    const node = nodes.find(value => value.id === entity.id);
    const [x, y, z] = node.transform.translation;
    const bounds = entity.bounds;
    const starter = entity.pickup.starterAmmunition;
    return `new LoadingBayE1M1PickupPlacement(${entity.id}UL, ${q(node.label)}, ${q(entity.pickup.item)}, ${entity.pickup.quantity}UL, ${q(entity.pickup.program)}, ${dormantPickupIds.has(entity.id)}, ${starter ? q(starter.item) : 'null'}, ${starter?.quantity ?? 0}UL, new System.Numerics.Vector3(${x}f, ${y}f, ${z}f), new System.Numerics.Vector3(${bounds.min[0]}f, ${bounds.min[1]}f, ${bounds.min[2]}f), new System.Numerics.Vector3(${bounds.max[0]}f, ${bounds.max[1]}f, ${bounds.max[2]}f))`;
  });
  const enemyRows = entities.filter(entity => entity.enemy).map((entity, sourceIndex) => {
    const node = nodes.find(value => value.id === entity.id);
    const [x, y, z] = node.transform.translation;
    const combat = entity.enemyCombat;
    const attack = combat.attack;
    const projectile = attack.projectile;
    const [hx, hy, hz] = entity.health.hitboxHalfExtents;
    const [ox, oy, oz] = attack.originOffset;
    const projectileFields = projectile ? `${projectile.mass}d, ${projectile.radius}d, ${projectile.impulse}d, ${projectile.gravityScale}d, ${projectile.lifetimeTicks}, ${projectile.restitution}d, ${q(projectile.visualAsset)}` : '0d, 0d, 0d, 0d, 0, 0d, null';
    return `new LoadingBayE1M1EnemyDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, new System.Numerics.Vector3(${x}f, ${y}f, ${z}f), new System.Numerics.Vector3(${hx}f, ${hy}f, ${hz}f), ${entity.health.max}, ${combat.sightRange}d, ${combat.hearingRange}d, ${combat.painDurationTicks}, ${q(combat.attackProgram)}, ${q(combat.defeatProgram)}, LoadingBayE1M1EnemyAttackKind.${enumName(attack.kind)}, ${attack.damage}, ${attack.range}d, ${attack.cooldownTicks}, new System.Numerics.Vector3(${ox}f, ${oy}f, ${oz}f), ${q(attack.presentation)}, ${projectileFields}, ${entity.defeatDrop?.pickup ?? 0}UL)`;
  });
  const encounterRows = entities.filter(entity => entity.encounter).map((entity, sourceIndex) => {
    const node = nodes.find(value => value.id === entity.id);
    const [x, y, z] = node.transform.translation;
    return `new LoadingBayE1M1EncounterDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(entity.encounter.program)}, ${entity.encounter.activationRadius}d, new System.Numerics.Vector3(${x}f, ${y}f, ${z}f), [${entity.encounter.members.map(member => `${member}UL`).join(', ')}])`;
  });
  const point = value => `new System.Numerics.Vector3(${value[0]}f, ${value[1]}f, ${value[2]}f)`;
  const nodeFor = entity => nodes.find(value => value.id === entity.id) ?? (() => { throw new Error(`world entity ${entity.id} lacks an authored node`); })();
  const hazardRows = entities.filter(entity => entity.hazard).map((entity, sourceIndex) => {
    const node = nodeFor(entity); const bounds = entity.bounds;
    return `new LoadingBayE1M1HazardDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(entity.hazard.program)}, ${entity.hazard.damage}, ${entity.hazard.cooldownTicks}, ${point(node.transform.translation)}, ${point(bounds.min)}, ${point(bounds.max)})`;
  });
  const barrelRows = entities.filter(entity => entity.explosiveProp).map((entity, sourceIndex) => {
    const node = nodeFor(entity);
    return `new LoadingBayE1M1BarrelDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(entity.explosiveProp.program)}, ${entity.health.max}, ${entity.explosiveProp.damage}, ${entity.explosiveProp.radius}d, ${point(node.transform.translation)}, ${point(entity.health.hitboxHalfExtents)})`;
  });
  const doorRows = entities.filter(entity => entity.door).map((entity, sourceIndex) => {
    const node = nodeFor(entity);
    return `new LoadingBayE1M1DoorDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(entity.switch.program)}, ${entity.switch.activationRadius}d, ${point(node.transform.translation)}, ${point(entity.door.openTranslation)}, ${entity.door.motionDurationTicks}, ${entity.door.autoCloseAfterTicks})`;
  });
  const floorRows = entities.filter(entity => entity.floorAction).map((entity, sourceIndex) => {
    const node = nodeFor(entity); const floor = entity.floorAction;
    return `new LoadingBayE1M1FloorDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(floor.program)}, ${floor.targetPlatform}UL, ${point(node.transform.translation)}, ${point(floor.upperTranslation)}, ${point(floor.loweredTranslation)}, ${floor.motionDurationTicks})`;
  });
  const liftRows = entities.filter(entity => entity.lift).map((entity, sourceIndex) => {
    const node = nodeFor(entity); const lift = entity.lift;
    return `new LoadingBayE1M1LiftDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(lift.program)}, ${lift.targetPlatform}UL, ${point(node.transform.translation)}, ${point(lift.raisedTranslation)}, ${point(lift.loweredTranslation)}, ${lift.motionDurationTicks}, ${lift.loweredWaitTicks})`;
  });
  const secretRows = entities.filter(entity => entity.secretRegion).map((entity, sourceIndex) => {
    const node = nodeFor(entity); const bounds = entity.bounds;
    return `new LoadingBayE1M1SecretDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(entity.secretRegion.program)}, ${point(node.transform.translation)}, ${point(bounds.min)}, ${point(bounds.max)})`;
  });
  const exitRows = entities.filter(entity => entity.levelExit).map((entity, sourceIndex) => {
    const node = nodeFor(entity);
    return `new LoadingBayE1M1ExitDefinition(${entity.id}UL, ${sourceIndex}, ${q(node.label)}, ${q(entity.levelExit.program)}, ${entity.levelExit.activationRadius}d, ${point(node.transform.translation)})`;
  });
  if (enemyRows.length !== 29 || encounterRows.length !== 4) throw new Error(`E1M1 actor catalog drifted: enemies=${enemyRows.length}, encounters=${encounterRows.length}`);
  return `#nullable enable\n// <auto-generated />\n// Source: content/projects/doom-e1m1.project.json (${expectedSha256})\nnamespace LoadingBay.Game;\n\ninternal enum LoadingBayE1M1ArmorGrantMode { None, SetMinimum }\ninternal enum LoadingBayE1M1ArmorTransition { None, Preserve, Replace }\ninternal enum LoadingBayE1M1WeaponAttackMode { Hitscan, Spread }\ninternal enum LoadingBayE1M1EnemyAttackKind { Hitscan, Projectile }\ninternal enum LoadingBayE1M1ProgramFamily { Gameplay, PlayerSetup, Pickup, EnemyAttack, EnemyDefeat, Encounter, Hazard, ExplosiveProp, FloorAction, Lift, Secret, Switch, LevelExit }\ninternal enum LoadingBayE1M1ProgramBindingShape { OperationProgram, PlayerSetupBindings, EncounterActivationAndClear }\ninternal abstract record LoadingBayE1M1ItemDefinition(string Id, int SourceIndex, ulong MaximumQuantity);\ninternal sealed record LoadingBayE1M1Ammunition(string Id, int SourceIndex, ulong MaximumQuantity) : LoadingBayE1M1ItemDefinition(Id, SourceIndex, MaximumQuantity);\ninternal sealed record LoadingBayE1M1Armor(string Id, int SourceIndex, ulong MaximumQuantity, int Protection, int MaximumArmor, int AbsorptionDivisor, LoadingBayE1M1ArmorGrantMode GrantMode, LoadingBayE1M1ArmorTransition Transition, bool ConsumeAtCap) : LoadingBayE1M1ItemDefinition(Id, SourceIndex, MaximumQuantity);\ninternal sealed record LoadingBayE1M1HealthSupply(string Id, int SourceIndex, ulong MaximumQuantity, string ProgramId, int RestoreHealth, int MaximumHealth, bool AutomaticUse, bool ConsumeAtCap) : LoadingBayE1M1ItemDefinition(Id, SourceIndex, MaximumQuantity);\ninternal sealed record LoadingBayE1M1Weapon(string Id, int SourceIndex, ulong MaximumQuantity, string ProgramId, string AmmunitionId, bool RepeatWhileHeld, LoadingBayE1M1WeaponAttackMode AttackMode, int DamageRolls, int Damage, double MaximumDistance, int CooldownTicks, int AmmunitionCost, int PelletCount, double SpreadDegrees, string Presentation) : LoadingBayE1M1ItemDefinition(Id, SourceIndex, MaximumQuantity);\ninternal sealed record LoadingBayE1M1ProgramDescriptor(LoadingBayE1M1ProgramFamily Family, string Id, int SourceIndex, string SourcePath, LoadingBayE1M1ProgramBindingShape BindingShape);\ninternal sealed record LoadingBayE1M1ItemGrant(string ItemId, ulong Quantity);\ninternal sealed record LoadingBayE1M1PlayerSetup(string Id, int SourceIndex, LoadingBayE1M1ItemGrant[] Grants, string EquippedWeaponId);\ninternal sealed record LoadingBayE1M1PickupPlacement(ulong EntityId, string Label, string ItemId, ulong Quantity, string ProgramId, bool StartsDormant, string? StarterAmmunitionItemId, ulong StarterAmmunitionQuantity, System.Numerics.Vector3 Translation, System.Numerics.Vector3 BoundsMin, System.Numerics.Vector3 BoundsMax);\ninternal sealed record LoadingBayE1M1EnemyDefinition(ulong EntityId, int SourceIndex, string Label, System.Numerics.Vector3 Translation, System.Numerics.Vector3 HitboxHalfExtents, int MaximumHealth, double SightRange, double HearingRange, int PainDurationTicks, string AttackProgramId, string DefeatProgramId, LoadingBayE1M1EnemyAttackKind AttackKind, int AttackDamage, double AttackRange, int AttackCooldownTicks, System.Numerics.Vector3 AttackOriginOffset, string AttackPresentation, double ProjectileMass, double ProjectileRadius, double ProjectileImpulse, double ProjectileGravityScale, int ProjectileLifetimeTicks, double ProjectileRestitution, string? ProjectileVisualAsset, ulong DropPickupEntityId);\ninternal sealed record LoadingBayE1M1EncounterDefinition(ulong EntityId, int SourceIndex, string Label, string ProgramId, double ActivationRadius, System.Numerics.Vector3 Translation, ulong[] Members);\n\ninternal static class LoadingBayE1M1SemanticCatalog\n{\n    internal const string ProjectSha256 = ${q(expectedSha256)};\n    internal const ulong CanonicalEntityCount = ${nodes.length}UL;\n    internal static readonly LoadingBayE1M1ItemDefinition[] Items = [\n        ${itemRows.join(',\n        ')}\n    ];\n    internal static readonly LoadingBayE1M1ProgramDescriptor[] ProgramDescriptors = [\n        ${programRows.join(',\n        ')}\n    ];\n    internal static readonly LoadingBayE1M1PlayerSetup[] PlayerSetups = [\n        ${playerSetupRows.join(',\n        ')}\n    ];\n    internal static readonly LoadingBayE1M1PickupPlacement[] Pickups = [\n        ${pickupRows.join(',\n        ')}\n    ];\n    internal static readonly LoadingBayE1M1EnemyDefinition[] Enemies = [\n        ${enemyRows.join(',\n        ')}\n    ];\n    internal static readonly LoadingBayE1M1EncounterDefinition[] Encounters = [\n        ${encounterRows.join(',\n        ')}\n    ];\n    internal static LoadingBayE1M1PickupPlacement Pickup(ulong entityId) => Pickups.Single(value => value.EntityId == entityId);\n    internal static T Item<T>(string id) where T : LoadingBayE1M1ItemDefinition => Items.OfType<T>().Single(value => value.Id == id);\n    internal static LoadingBayE1M1PlayerSetup PlayerSetup(string id) => PlayerSetups.Single(value => value.Id == id);\n    internal static LoadingBayE1M1EnemyDefinition Enemy(ulong entityId) => Enemies.Single(value => value.EntityId == entityId);\n}\n`;
}
