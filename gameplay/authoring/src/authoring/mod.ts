/**
 * The single import surface for catalogs. Catalogs import only from here;
 * extending the grammar means editing `authoring/` and the Rust compiler in
 * `gameplay/src/{authored,compile}.rs` in the same change — intentional
 * coupling per the downstream-adoption guide.
 */

export type {
  AmmunitionDefinition,
  ArmorDefinition,
  ArmorGrantMode,
  ArmorTransition,
  AttackMode,
  EnemyAttackOperation,
  EnemyAttackPredicate,
  EnemyAttackProgram,
  EnemyAttackProgramNode,
  EnemyDefeatOperation,
  EnemyDefeatProgram,
  EnemyDefeatProgramNode,
  ExplosivePropOperation,
  ExplosivePropPredicate,
  ExplosivePropProgram,
  ExplosivePropProgramNode,
  SwitchOperation,
  SwitchPredicate,
  SwitchProgram,
  SwitchProgramNode,
  FloorActionOperation,
  FloorActionPredicate,
  FloorActionProgram,
  FloorActionProgramNode,
  LiftOperation,
  LiftPredicate,
  LiftProgram,
  LiftProgramNode,
  LevelExitOperation,
  LevelExitPredicate,
  LevelExitProgram,
  LevelExitProgramNode,
  HealthSupplyDefinition,
  HazardOperation,
  HazardPredicate,
  HazardProgram,
  HazardProgramNode,
  GameplayOperation,
  GameplayPredicate,
  GameplayProgram,
  GameplayProgramNode,
  ItemDefinition,
  ItemKindDefinition,
  LoadingBayGameplayPayload,
  PackageInput,
  PickupOperation,
  PickupPredicate,
  PickupProgram,
  PickupProgramNode,
  PlayerSetupOperation,
  PlayerSetupProgram,
  SecretOperation,
  SecretPredicate,
  SecretProgram,
  SecretProgramNode,
  WeaponDefinition,
} from "./definitions.js";

export { composePackage } from "./envelope.js";

import type {
  AmmunitionDefinition,
  ArmorDefinition,
  ArmorGrantMode,
  ArmorTransition,
  AttackMode,
  EnemyAttackOperation,
  EnemyAttackPredicate,
  EnemyAttackProgramNode,
  EnemyDefeatOperation,
  EnemyDefeatProgramNode,
  ExplosivePropOperation,
  ExplosivePropPredicate,
  ExplosivePropProgramNode,
  SwitchOperation,
  SwitchPredicate,
  SwitchProgramNode,
  FloorActionOperation,
  FloorActionPredicate,
  FloorActionProgramNode,
  LiftOperation,
  LiftPredicate,
  LiftProgramNode,
  HealthSupplyDefinition,
  HazardOperation,
  HazardPredicate,
  HazardProgramNode,
  GameplayOperation,
  GameplayPredicate,
  GameplayProgramNode,
  ItemDefinition,
  PickupOperation,
  PickupPredicate,
  PickupProgramNode,
  PlayerSetupOperation,
  PlayerSetupProgram,
  WeaponDefinition,
} from "./definitions.js";

export const ammunition = (id: string, maxQuantity: number): ItemDefinition => ({
  id,
  maxQuantity,
  kind: { kind: "ammunition" } satisfies AmmunitionDefinition,
});

export const armor = (
  id: string,
  config: Readonly<{
    protection: number;
    maximumArmor: number;
    absorptionDivisor: number;
    grantMode?: ArmorGrantMode;
    transition: ArmorTransition;
    consumeAtCap?: boolean;
    program?: string;
  }>,
): ItemDefinition => {
  const { program, ...kind } = config;
  return {
    id,
    maxQuantity: 1,
    ...(program === undefined ? {} : { program }),
    kind: { kind: "armor", ...kind } satisfies ArmorDefinition,
  };
};

export const healthSupply = (
  id: string,
  config: Readonly<{
    restoreHealth: number;
    maximumHealth: number;
    automaticUse?: boolean;
    consumeAtCap?: boolean;
    program?: string;
  }>,
): ItemDefinition => {
  const { program, ...kind } = config;
  return {
    id,
    maxQuantity: 1,
    ...(program === undefined ? {} : { program }),
    kind: { kind: "healthSupply", ...kind } satisfies HealthSupplyDefinition,
  };
};

export const weapon = (
  id: string,
  config: Readonly<{
    ammunition: string;
    repeatWhileHeld?: boolean;
    damageRolls?: number;
    attackMode?: AttackMode;
    pelletCount?: number;
    spreadDegrees?: number;
    damage: number;
    maxDistance: number;
    cooldownTicks: number;
    ammunitionCost: number;
    muzzleOffset?: readonly [number, number, number];
    presentation: string;
    program?: string;
  }>,
): ItemDefinition => {
  const { program, ...kind } = config;
  return {
    id,
    maxQuantity: 1,
    ...(program === undefined ? {} : { program }),
    kind: { kind: "weapon", ...kind } satisfies WeaponDefinition,
  };
};

export const sequence = (...steps: readonly GameplayProgramNode[]): GameplayProgramNode => ({
  kind: "sequence",
  steps,
});

export const when = (
  predicate: GameplayPredicate,
  thenProgram: GameplayProgramNode,
  otherwiseProgram?: GameplayProgramNode,
): GameplayProgramNode =>
  otherwiseProgram === undefined
    ? { kind: "when", predicate, thenProgram }
    : { kind: "when", predicate, thenProgram, otherwiseProgram };

export const operation = (operation: GameplayOperation): GameplayProgramNode => ({
  kind: "operation",
  operation,
});

export const pickupSequence = (...steps: readonly PickupProgramNode[]): PickupProgramNode => ({
  kind: "sequence",
  steps,
});

export const pickupWhen = (
  predicate: PickupPredicate,
  thenProgram: PickupProgramNode,
  otherwiseProgram?: PickupProgramNode,
): PickupProgramNode =>
  otherwiseProgram === undefined
    ? { kind: "when", predicate, thenProgram }
    : { kind: "when", predicate, thenProgram, otherwiseProgram };

export const pickupOperation = (operation: PickupOperation): PickupProgramNode => ({
  kind: "operation",
  operation,
});

export const grantItem = (item: string, quantity: number): PlayerSetupOperation => ({
  kind: "grantItem",
  item,
  quantity,
});

export const equipInitialWeapon = (item: string): PlayerSetupOperation => ({
  kind: "equipInitialWeapon",
  item,
});

export const playerSetupProgram = (
  id: string,
  ...program: readonly PlayerSetupOperation[]
): PlayerSetupProgram => ({ id, program });

export const enemyAttackSequence = (
  ...steps: readonly EnemyAttackProgramNode[]
): EnemyAttackProgramNode => ({ kind: "sequence", steps });

export const enemyAttackWhen = (
  predicate: EnemyAttackPredicate,
  thenProgram: EnemyAttackProgramNode,
  otherwiseProgram?: EnemyAttackProgramNode,
): EnemyAttackProgramNode =>
  otherwiseProgram === undefined
    ? { kind: "when", predicate, thenProgram }
    : { kind: "when", predicate, thenProgram, otherwiseProgram };

export const enemyAttackOperation = (
  operation: EnemyAttackOperation,
): EnemyAttackProgramNode => ({ kind: "operation", operation });

export const enemyDefeatSequence = (
  ...steps: readonly EnemyDefeatProgramNode[]
): EnemyDefeatProgramNode => ({ kind: "sequence", steps });

export const enemyDefeatOperation = (
  operation: EnemyDefeatOperation,
): EnemyDefeatProgramNode => ({ kind: "operation", operation });

export const hazardSequence = (...steps: readonly HazardProgramNode[]): HazardProgramNode => ({
  kind: "sequence",
  steps,
});

export const hazardWhen = (
  predicate: HazardPredicate,
  thenProgram: HazardProgramNode,
  otherwiseProgram?: HazardProgramNode,
): HazardProgramNode =>
  otherwiseProgram === undefined
    ? { kind: "when", predicate, thenProgram }
    : { kind: "when", predicate, thenProgram, otherwiseProgram };

export const hazardOperation = (operation: HazardOperation): HazardProgramNode => ({
  kind: "operation",
  operation,
});

export const explosivePropSequence = (
  ...steps: readonly ExplosivePropProgramNode[]
): ExplosivePropProgramNode => ({ kind: "sequence", steps });

export const explosivePropWhen = (
  predicate: ExplosivePropPredicate,
  thenProgram: ExplosivePropProgramNode,
  otherwiseProgram?: ExplosivePropProgramNode,
): ExplosivePropProgramNode =>
  otherwiseProgram === undefined
    ? { kind: "when", predicate, thenProgram }
    : { kind: "when", predicate, thenProgram, otherwiseProgram };

export const explosivePropOperation = (
  operation: ExplosivePropOperation,
): ExplosivePropProgramNode => ({ kind: "operation", operation });

export const switchSequence = (...steps: readonly SwitchProgramNode[]): SwitchProgramNode => ({
  kind: "sequence",
  steps,
});

export const switchWhen = (
  predicate: SwitchPredicate,
  thenProgram: SwitchProgramNode,
  otherwiseProgram?: SwitchProgramNode,
): SwitchProgramNode =>
  otherwiseProgram === undefined
    ? { kind: "when", predicate, thenProgram }
    : { kind: "when", predicate, thenProgram, otherwiseProgram };

export const switchOperation = (operation: SwitchOperation): SwitchProgramNode => ({
  kind: "operation",
  operation,
});

export const floorActionSequence = (...steps: readonly FloorActionProgramNode[]): FloorActionProgramNode => ({ kind: "sequence", steps });
export const floorActionWhen = (predicate: FloorActionPredicate, thenProgram: FloorActionProgramNode, otherwiseProgram?: FloorActionProgramNode): FloorActionProgramNode =>
  otherwiseProgram === undefined ? { kind: "when", predicate, thenProgram } : { kind: "when", predicate, thenProgram, otherwiseProgram };
export const floorActionOperation = (operation: FloorActionOperation): FloorActionProgramNode => ({ kind: "operation", operation });

export const liftSequence = (...steps: readonly LiftProgramNode[]): LiftProgramNode => ({ kind: "sequence", steps });
export const liftWhen = (predicate: LiftPredicate, thenProgram: LiftProgramNode, otherwiseProgram?: LiftProgramNode): LiftProgramNode =>
  otherwiseProgram === undefined ? { kind: "when", predicate, thenProgram } : { kind: "when", predicate, thenProgram, otherwiseProgram };
export const liftOperation = (operation: LiftOperation): LiftProgramNode => ({ kind: "operation", operation });
