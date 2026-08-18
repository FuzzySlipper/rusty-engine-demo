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
  HealthSupplyDefinition,
  ItemDefinition,
  ItemKindDefinition,
  LoadingBayGameplayPayload,
  PackageInput,
  WeaponDefinition,
} from "./definitions.js";

export { composePackage } from "./envelope.js";

import type {
  AmmunitionDefinition,
  ArmorDefinition,
  ArmorGrantMode,
  ArmorTransition,
  AttackMode,
  HealthSupplyDefinition,
  ItemDefinition,
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
  }>,
): ItemDefinition => ({
  id,
  maxQuantity: 1,
  kind: { kind: "armor", ...config } satisfies ArmorDefinition,
});

export const healthSupply = (
  id: string,
  config: Readonly<{
    restoreHealth: number;
    maximumHealth: number;
    automaticUse?: boolean;
    consumeAtCap?: boolean;
  }>,
): ItemDefinition => ({
  id,
  maxQuantity: 1,
  kind: { kind: "healthSupply", ...config } satisfies HealthSupplyDefinition,
});

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
    projectileMass?: number;
    projectileRadius?: number;
    projectileImpulse?: number;
    projectileSpeed?: number;
  }>,
): ItemDefinition => ({
  id,
  maxQuantity: 1,
  kind: { kind: "weapon", ...config } satisfies WeaponDefinition,
});
