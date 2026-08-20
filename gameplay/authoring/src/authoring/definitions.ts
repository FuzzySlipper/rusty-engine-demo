/**
 * Loading Bay gameplay-authoring grammar (the only shapes catalogs may use).
 *
 * The item DTO mirrors the project document's `itemDefinitions` entry shape
 * exactly (camelCase, `kind`-tagged) so the Rust compiler reuses the same
 * authored DTO and the same single semantic conversion as project admission.
 * TypeScript composes definitions; it never evaluates gameplay.
 */

export type AttackMode = "hitscan" | "spread";

// Mirrors the Rust DTO spelling (`StoredArmorGrantMode`: Add/SetMinimum in
// camelCase) exactly; the compiler rejects any other grant-mode string.
export type ArmorGrantMode = "add" | "setMinimum";

export type ArmorTransition = "replace" | "preserve";

export type AmmunitionDefinition = Readonly<{
  kind: "ammunition";
}>;

export type ArmorDefinition = Readonly<{
  kind: "armor";
  protection: number;
  maximumArmor: number;
  absorptionDivisor: number;
  grantMode?: ArmorGrantMode;
  transition: ArmorTransition;
  consumeAtCap?: boolean;
}>;

export type HealthSupplyDefinition = Readonly<{
  kind: "healthSupply";
  restoreHealth: number;
  maximumHealth: number;
  automaticUse?: boolean;
  consumeAtCap?: boolean;
}>;

export type WeaponDefinition = Readonly<{
  kind: "weapon";
  ammunition: string;
  repeatWhileHeld?: boolean;
  damageRolls?: number;
  attackMode?: AttackMode;
  pelletCount?: number;
  spreadDegrees?: number;
  damage?: number;
  maxDistance?: number;
  cooldownTicks?: number;
  ammunitionCost?: number;
  muzzleOffset?: readonly [number, number, number];
  presentation?: string;
}>;

export type ItemKindDefinition =
  | AmmunitionDefinition
  | ArmorDefinition
  | HealthSupplyDefinition
  | WeaponDefinition;

export type ItemDefinition = Readonly<{
  id: string;
  maxQuantity: number;
  /** Closed Rust program selected for this item, when it has active gameplay. */
  program?: string;
  kind: ItemKindDefinition;
}>;

/** Closed structural grammar for the Demo's Rust-owned hitscan primitives. */
export type GameplayPredicate = "impactIsHit";
export type GameplayOperation =
  | "recordFired"
  | "consumeAmmo"
  | "applyHit"
  | "applyMiss"
  | "applySpreadImpacts"
  | "setCooldown"
  | "useHealthSupply";
export type GameplayProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly GameplayProgramNode[] }>
  | Readonly<{
      kind: "when";
      predicate: GameplayPredicate;
      thenProgram: GameplayProgramNode;
      otherwiseProgram?: GameplayProgramNode;
    }>
  | Readonly<{ kind: "operation"; operation: GameplayOperation }>;
export type GameplayProgram = Readonly<{ id: string; program: GameplayProgramNode }>;

/**
 * Closed Rust-owned pickup grammar. A pickup supplies the only item and
 * optional starter ammunition the program can affect; programs cannot select
 * entities, call services, or introduce expressions/loops.
 */
export type PickupPredicate = "weaponAlreadyOwnedWithStarterAmmunition";
export type PickupOperation =
  | "grantPickedItem"
  | "grantStarterAmmunition"
  | "useGrantedHealthSupply"
  | "applyGrantedArmor"
  | "consumePickup";
export type PickupProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly PickupProgramNode[] }>
  | Readonly<{
      kind: "when";
      predicate: PickupPredicate;
      thenProgram: PickupProgramNode;
      otherwiseProgram?: PickupProgramNode;
    }>
  | Readonly<{ kind: "operation"; operation: PickupOperation }>;
export type PickupProgram = Readonly<{ id: string; program: PickupProgramNode }>;

/**
 * Closed Rust-owned player initialization grammar. Unlike the item, pickup,
 * and enemy families, this is a source-ordered flat setup sequence: every
 * equipment selection observes only the grants that precede it.
 */
export type PlayerSetupOperation =
  | Readonly<{ kind: "grantItem"; item: string; quantity: number }>
  | Readonly<{ kind: "equipInitialWeapon"; item: string }>;
export type PlayerSetupProgram = Readonly<{
  id: string;
  program: readonly PlayerSetupOperation[];
}>;

/** Closed Rust-owned enemy attack grammar; it is intentionally not the item grammar. */
export type EnemyAttackPredicate = "impactIsHit";
export type EnemyAttackOperation =
  | "recordEnemyAttack"
  | "applyEnemyHit"
  | "applyEnemyMiss"
  | "spawnEnemyProjectile"
  | "setEnemyCooldown";
export type EnemyAttackProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly EnemyAttackProgramNode[] }>
  | Readonly<{
      kind: "when";
      predicate: EnemyAttackPredicate;
      thenProgram: EnemyAttackProgramNode;
      otherwiseProgram?: EnemyAttackProgramNode;
    }>
  | Readonly<{ kind: "operation"; operation: EnemyAttackOperation }>;
export type EnemyAttackProgram = Readonly<{
  id: string;
  program: EnemyAttackProgramNode;
}>;

/** Closed Rust-owned enemy defeat grammar; no authored predicates or selectors. */
export type EnemyDefeatOperation = "recordEnemyDefeat" | "activateBoundDrop";
export type EnemyDefeatProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly EnemyDefeatProgramNode[] }>
  | Readonly<{ kind: "operation"; operation: EnemyDefeatOperation }>;
export type EnemyDefeatProgram = Readonly<{
  id: string;
  program: EnemyDefeatProgramNode;
}>;

/** Closed Rust-owned environmental hazard grammar. */
export type HazardPredicate = "playerOverlapping" | "playerEligible" | "cooldownReady";
export type HazardOperation = "applyHazardDamage" | "scheduleHazardCooldown";
export type HazardProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly HazardProgramNode[] }>
  | Readonly<{
      kind: "when";
      predicate: HazardPredicate;
      thenProgram: HazardProgramNode;
      otherwiseProgram?: HazardProgramNode;
    }>
  | Readonly<{ kind: "operation"; operation: HazardOperation }>;
export type HazardProgram = Readonly<{ id: string; program: HazardProgramNode }>;

/** Closed Rust-owned explosive-prop consequence grammar. */
export type ExplosivePropPredicate = "explosionPending";
export type ExplosivePropOperation =
  | "selectRadialTargets"
  | "applyScaledDamage"
  | "resolveExplosion";
export type ExplosivePropProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly ExplosivePropProgramNode[] }>
  | Readonly<{
      kind: "when";
      predicate: ExplosivePropPredicate;
      thenProgram: ExplosivePropProgramNode;
      otherwiseProgram?: ExplosivePropProgramNode;
    }>
  | Readonly<{ kind: "operation"; operation: ExplosivePropOperation }>;
export type ExplosivePropProgram = Readonly<{
  id: string;
  program: ExplosivePropProgramNode;
}>;

/** Closed Rust-owned switch interaction grammar. Door identities come only
 * from the switch's separately authored, Rust-validated bound effects. */
export type SwitchPredicate = "switchAvailable";
export type SwitchOperation =
  | "recordActivation"
  | "requestOpenBoundDoor"
  | "requestCloseBoundDoor"
  | "emitInteractionFeedback";
export type SwitchProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly SwitchProgramNode[] }>
  | Readonly<{
      kind: "when";
      predicate: SwitchPredicate;
      thenProgram: SwitchProgramNode;
      otherwiseProgram?: SwitchProgramNode;
    }>
  | Readonly<{ kind: "operation"; operation: SwitchOperation }>;
export type SwitchProgram = Readonly<{ id: string; program: SwitchProgramNode }>;

/** Closed Rust-owned walk-trigger floor motion grammar. */
export type FloorActionPredicate = "activationEntered" | "loweringMotionTick";
export type FloorActionOperation =
  | "recordActivation"
  | "requestLowerBoundPlatform"
  | "advanceLowering"
  | "emitFloorFeedback";
export type FloorActionProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly FloorActionProgramNode[] }>
  | Readonly<{ kind: "when"; predicate: FloorActionPredicate; thenProgram: FloorActionProgramNode; otherwiseProgram?: FloorActionProgramNode }>
  | Readonly<{ kind: "operation"; operation: FloorActionOperation }>;
export type FloorActionProgram = Readonly<{ id: string; program: FloorActionProgramNode }>;

/** Closed Rust-owned lift-cycle grammar. */
export type LiftPredicate = "activationEntered" | "loweringMotionTick" | "waitingTick" | "raisingMotionTick";
export type LiftOperation =
  | "recordActivation"
  | "requestLowerBoundPlatform"
  | "advanceLowering"
  | "advanceWait"
  | "advanceRaising"
  | "emitLiftFeedback";
export type LiftProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly LiftProgramNode[] }>
  | Readonly<{ kind: "when"; predicate: LiftPredicate; thenProgram: LiftProgramNode; otherwiseProgram?: LiftProgramNode }>
  | Readonly<{ kind: "operation"; operation: LiftOperation }>;
export type LiftProgram = Readonly<{ id: string; program: LiftProgramNode }>;

export type EncounterActivationPredicate = "activationEligible";
export type EncounterActivationOperation =
  | "recordEncounterActivation"
  | "activateBoundMembers"
  | "emitEncounterFeedback";
export type EncounterActivationProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly EncounterActivationProgramNode[] }>
  | Readonly<{ kind: "when"; predicate: EncounterActivationPredicate; thenProgram: EncounterActivationProgramNode; otherwiseProgram?: EncounterActivationProgramNode }>
  | Readonly<{ kind: "operation"; operation: EncounterActivationOperation }>;
export type EncounterClearPredicate = "membersDefeated";
export type EncounterClearOperation = "recordEncounterCleared" | "openBoundExit";
export type EncounterClearProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly EncounterClearProgramNode[] }>
  | Readonly<{ kind: "when"; predicate: EncounterClearPredicate; thenProgram: EncounterClearProgramNode; otherwiseProgram?: EncounterClearProgramNode }>
  | Readonly<{ kind: "operation"; operation: EncounterClearOperation }>;
export type EncounterProgram = Readonly<{
  id: string;
  activation: EncounterActivationProgramNode;
  clear: EncounterClearProgramNode;
}>;

/** Closed Rust-owned secret discovery grammar. Region identity, overlap,
 * once-state, and fact data stay with the Rust progression service. */
export type SecretPredicate = "secretRegionEntered" | "secretUndiscovered";
export type SecretOperation = "recordDiscovery" | "emitSecretPresentation";
export type SecretProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly SecretProgramNode[] }>
  | Readonly<{ kind: "when"; predicate: SecretPredicate; thenProgram: SecretProgramNode; otherwiseProgram?: SecretProgramNode }>
  | Readonly<{ kind: "operation"; operation: SecretOperation }>;
export type SecretProgram = Readonly<{ id: string; program: SecretProgramNode }>;

/** Closed Rust-owned level-exit completion grammar. Actor admission, range,
 * exit state, and the completion fact remain with Rust. */
export type LevelExitPredicate = "exitAvailable";
export type LevelExitOperation = "recordCompletion" | "emitCompletionPresentation";
export type LevelExitProgramNode =
  | Readonly<{ kind: "sequence"; steps: readonly LevelExitProgramNode[] }>
  | Readonly<{ kind: "when"; predicate: LevelExitPredicate; thenProgram: LevelExitProgramNode; otherwiseProgram?: LevelExitProgramNode }>
  | Readonly<{ kind: "operation"; operation: LevelExitOperation }>;
export type LevelExitProgram = Readonly<{ id: string; program: LevelExitProgramNode }>;

export type LoadingBayGameplayPayload = Readonly<{
  schemaVersion: 1;
  items: readonly ItemDefinition[];
  gameplayPrograms: readonly GameplayProgram[];
  pickupPrograms: readonly PickupProgram[];
  playerSetupPrograms: readonly PlayerSetupProgram[];
  enemyAttackPrograms: readonly EnemyAttackProgram[];
  enemyDefeatPrograms: readonly EnemyDefeatProgram[];
  hazardPrograms: readonly HazardProgram[];
  explosivePropPrograms: readonly ExplosivePropProgram[];
  encounterPrograms: readonly EncounterProgram[];
  switchPrograms: readonly SwitchProgram[];
  floorActionPrograms: readonly FloorActionProgram[];
  liftPrograms: readonly LiftProgram[];
  secretPrograms: readonly SecretProgram[];
  levelExitPrograms: readonly LevelExitProgram[];
}>;

export type PackageInput = Readonly<{
  /** Package id inside the `loading-bay` domain, e.g. "e1m1-core". */
  packageId: string;
  version: number;
  /** Section name → source path relative to the repository root. */
  sources: Readonly<Record<string, string>>;
  payload: LoadingBayGameplayPayload;
}>;
