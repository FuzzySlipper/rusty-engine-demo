/**
 * E1M1 item definitions: the exact weapons, ammunition, armor, and health
 * supplies of the Doom E1M1 calibration set, with source tuning values.
 * Provenance: classic Doom shareware E1M1 balance (see
 * docs/source-provenance.md). Adding or tuning an item is a one-entry edit
 * here; the package materializes and the Rust compiler admits it.
 */

import { ammunition, armor, healthSupply, weapon } from "../authoring/mod.js";

export const items = [
  ammunition("ammo/bullets", 200),
  ammunition("ammo/shells", 50),
  armor("armor/blue", {
    protection: 200,
    maximumArmor: 200,
    absorptionDivisor: 2,
    grantMode: "setMinimum",
    transition: "replace",
  }),
  armor("armor/bonus", {
    protection: 1,
    maximumArmor: 200,
    absorptionDivisor: 3,
    transition: "preserve",
    consumeAtCap: true,
  }),
  armor("armor/green", {
    protection: 100,
    maximumArmor: 200,
    absorptionDivisor: 3,
    grantMode: "setMinimum",
    transition: "replace",
  }),
  healthSupply("supply/health-bonus", {
    restoreHealth: 1,
    maximumHealth: 200,
    automaticUse: true,
    consumeAtCap: true,
    program: "item/health-supply",
  }),
  healthSupply("supply/medikit", {
    restoreHealth: 25,
    maximumHealth: 100,
    automaticUse: true,
    program: "item/health-supply",
  }),
  healthSupply("supply/stimpack", {
    restoreHealth: 10,
    maximumHealth: 100,
    automaticUse: true,
    program: "item/health-supply",
  }),
  weapon("weapon/fist", {
    ammunition: "ammo/bullets",
    repeatWhileHeld: true,
    damageRolls: 10,
    attackMode: "hitscan",
    damage: 2,
    maxDistance: 4.0,
    cooldownTicks: 38,
    ammunitionCost: 0,
    muzzleOffset: [0.0, 0.0, 0.0],
    presentation: "fist",
    program: "weapon/hitscan-unarmed",
  }),
  weapon("weapon/pistol", {
    ammunition: "ammo/bullets",
    repeatWhileHeld: true,
    damageRolls: 3,
    attackMode: "hitscan",
    damage: 5,
    maxDistance: 128.0,
    cooldownTicks: 24,
    ammunitionCost: 1,
    muzzleOffset: [0.0, 0.0, 0.0],
    presentation: "pistol",
    program: "weapon/hitscan-ammunition",
  }),
  weapon("weapon/shotgun", {
    ammunition: "ammo/shells",
    repeatWhileHeld: true,
    damageRolls: 3,
    attackMode: "spread",
    pelletCount: 7,
    spreadDegrees: 5.625,
    damage: 5,
    maxDistance: 128.0,
    cooldownTicks: 63,
    ammunitionCost: 1,
    muzzleOffset: [0.0, 0.0, 0.0],
    presentation: "shotgun",
    program: "weapon/spread",
  }),
] as const;
