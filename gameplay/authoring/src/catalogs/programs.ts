/** Closed E1M1 item programs selected and executed by the Rust gameplay authority. */
import {
  enemyAttackOperation,
  enemyAttackSequence,
  enemyAttackWhen,
  enemyDefeatOperation,
  enemyDefeatSequence,
  equipInitialWeapon,
  grantItem,
  operation,
  playerSetupProgram,
  pickupOperation,
  pickupSequence,
  pickupWhen,
  sequence,
  when,
  type EnemyAttackProgram,
  type EnemyDefeatProgram,
  type GameplayProgram,
  type PickupProgram,
  type PlayerSetupProgram,
} from "../authoring/mod.js";

const resolveImpact = when(
  "impactIsHit",
  operation("applyHit"),
  operation("applyMiss"),
);

export const gameplayPrograms = [
  {
    id: "weapon/hitscan-ammunition",
    program: sequence(
      operation("recordFired"),
      operation("consumeAmmo"),
      resolveImpact,
      operation("setCooldown"),
    ),
  },
  {
    id: "weapon/hitscan-unarmed",
    program: sequence(operation("recordFired"), resolveImpact, operation("setCooldown")),
  },
  {
    id: "weapon/spread",
    program: sequence(
      operation("recordFired"),
      operation("consumeAmmo"),
      operation("applySpreadImpacts"),
      operation("setCooldown"),
    ),
  },
  {
    id: "item/health-supply",
    program: sequence(operation("useHealthSupply")),
  },
] as const satisfies readonly GameplayProgram[];

const consume = pickupOperation("consumePickup");

/**
 * Immutable E1M1 collection programs. Rust supplies the picked item and
 * starter ammunition context and owns every resulting mutation.
 */
export const pickupPrograms = [
  {
    id: "pickup/ammunition",
    program: pickupSequence(pickupOperation("grantPickedItem"), consume),
  },
  {
    id: "pickup/weapon-starter",
    program: pickupSequence(
      pickupWhen(
        "weaponAlreadyOwnedWithStarterAmmunition",
        pickupOperation("grantStarterAmmunition"),
        pickupSequence(
          pickupOperation("grantPickedItem"),
          pickupOperation("grantStarterAmmunition"),
        ),
      ),
      consume,
    ),
  },
  {
    id: "pickup/automatic-health",
    program: pickupSequence(
      pickupOperation("grantPickedItem"),
      pickupOperation("useGrantedHealthSupply"),
      consume,
    ),
  },
  {
    id: "pickup/automatic-armor",
    program: pickupSequence(
      pickupOperation("grantPickedItem"),
      pickupOperation("applyGrantedArmor"),
      consume,
    ),
  },
] as const satisfies readonly PickupProgram[];

/** Immutable initial inventory/equipment policy, admitted before a session exists. */
export const playerSetupPrograms = [
  playerSetupProgram(
    "player/e1m1-pistol-start",
    grantItem("weapon/fist", 1),
    grantItem("weapon/pistol", 1),
    grantItem("ammo/bullets", 50),
    equipInitialWeapon("weapon/pistol"),
  ),
  playerSetupProgram(
    "player/shotgun-start",
    grantItem("weapon/fist", 1),
    grantItem("weapon/shotgun", 1),
    grantItem("ammo/shells", 8),
    equipInitialWeapon("weapon/shotgun"),
  ),
] as const satisfies readonly PlayerSetupProgram[];

const resolveEnemyImpact = enemyAttackWhen(
  "impactIsHit",
  enemyAttackOperation("applyEnemyHit"),
  enemyAttackOperation("applyEnemyMiss"),
);

/** E1M1 enemies select one of these closed Rust execution programs. */
export const enemyAttackPrograms = [
  {
    id: "enemy-attack/hitscan",
    program: enemyAttackSequence(
      enemyAttackOperation("recordEnemyAttack"),
      resolveEnemyImpact,
      enemyAttackOperation("setEnemyCooldown"),
    ),
  },
  {
    id: "enemy-attack/projectile",
    program: enemyAttackSequence(
      enemyAttackOperation("recordEnemyAttack"),
      enemyAttackOperation("spawnEnemyProjectile"),
      enemyAttackOperation("setEnemyCooldown"),
    ),
  },
] as const satisfies readonly EnemyAttackProgram[];

/** Core death remains Rust-owned; these only record and activate bound drops. */
export const enemyDefeatPrograms = [
  {
    id: "enemy-defeat/with-drop",
    program: enemyDefeatSequence(
      enemyDefeatOperation("recordEnemyDefeat"),
      enemyDefeatOperation("activateBoundDrop"),
    ),
  },
  {
    id: "enemy-defeat/without-drop",
    program: enemyDefeatSequence(enemyDefeatOperation("recordEnemyDefeat")),
  },
] as const satisfies readonly EnemyDefeatProgram[];
