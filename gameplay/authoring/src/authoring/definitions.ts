/**
 * Loading Bay gameplay-authoring grammar (the only shapes catalogs may use).
 *
 * The item DTO mirrors the project document's `itemDefinitions` entry shape
 * exactly (camelCase, `kind`-tagged) so the Rust compiler reuses the same
 * authored DTO and the same single semantic conversion as project admission.
 * TypeScript composes definitions; it never evaluates gameplay.
 */

export type AttackMode = "hitscan" | "spread" | "projectile" | "automatic";

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
  projectileMass?: number;
  projectileRadius?: number;
  projectileImpulse?: number;
  projectileSpeed?: number;
}>;

export type ItemKindDefinition =
  | AmmunitionDefinition
  | ArmorDefinition
  | HealthSupplyDefinition
  | WeaponDefinition;

export type ItemDefinition = Readonly<{
  id: string;
  maxQuantity: number;
  kind: ItemKindDefinition;
}>;

export type LoadingBayGameplayPayload = Readonly<{
  schemaVersion: 1;
  items: readonly ItemDefinition[];
}>;

export type PackageInput = Readonly<{
  /** Package id inside the `loading-bay` domain, e.g. "e1m1-core". */
  packageId: string;
  version: number;
  /** Section name → source path relative to the repository root. */
  sources: Readonly<Record<string, string>>;
  payload: LoadingBayGameplayPayload;
}>;
