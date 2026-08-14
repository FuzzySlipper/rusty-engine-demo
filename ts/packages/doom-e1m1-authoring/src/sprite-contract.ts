export const DOOM_SPRITE_TICK_RATE_HZ = 35;

export const DOOM_SPRITE_CONTRACT_SOURCES = [
  {
    path: "/home/research/doom.ts/src/doom/sprites/sprite-defs-array.ts",
    sha256: "5f7681a3180d201d5571598c275edf82427b5954025a633268a16de7736b12dc",
    role: "rotation and mirror installation semantics",
  },
  {
    path: "/home/research/doom.ts/src/doom/doom/info/states.ts",
    sha256: "c4316d2278d8c86a42714aac0c95a97b599c48ff78a58b48dfa2bb42d7fa98c3",
    role: "state frame order, duration, full-bright flag, and looping",
  },
  {
    path: "/home/research/doom.ts/src/doom/doom/info/mobj-infos.ts",
    sha256: "3f676d2928c387400fa3ddcefb25c9daf5c80049ef852e91feede0f92d760c38",
    role: "actor and projectile Doom-unit dimensions",
  },
  {
    path: "/home/research/doom.ts/src/doom/play/p-sprite.ts",
    sha256: "03c2dc8b31f752945aebc9be5f7a4d0fb8eee4e2d961efe977601246f75bdb87",
    role: "player weapon ready, fire, flash, recovery, and screen-coordinate semantics",
  },
  {
    path: "/home/research/doom.ts/src/doom/webgl/objects/p-sprite.ts",
    sha256: "b1912113524feda070b55dd53c0b8e79a76e0bc30bf36b4157e30f683fa44713",
    role: "320 by 200 player weapon patch placement",
  },
] as const;

export interface DoomSpriteClipStep {
  readonly state: string;
  readonly frame: string;
  readonly tics: number;
  readonly fullBright: boolean;
}
export interface DoomSpriteClipDefinition {
  readonly id: string;
  readonly loopMode: "once" | "repeat";
  readonly steps: readonly DoomSpriteClipStep[];
}

export interface DoomSpriteFamilyDefinition {
  readonly prefix: string;
  readonly role: "actor" | "projectile" | "effect" | "weapon" | "item";
  readonly thingType: number | null;
  readonly dimensionsDoomUnits: {
    readonly radius: number;
    readonly height: number;
  } | null;
  readonly clips: readonly DoomSpriteClipDefinition[];
}

const step = (
  state: string,
  frame: string,
  tics: number,
  fullBright = false,
): DoomSpriteClipStep => ({ state, frame, tics, fullBright });

function actorClips(
  prefix: "POSS" | "SPOS" | "TROO",
): readonly DoomSpriteClipDefinition[] {
  const state = (name: string): string => `S_${prefix}_${name}`;
  if (prefix === "POSS") {
    return [
      { id: "idle", loopMode: "repeat", steps: [step(state("STND"), "A", 10), step(state("STND2"), "B", 10)] },
      { id: "walk", loopMode: "repeat", steps: ["A", "A", "B", "B", "C", "C", "D", "D"].map((frame, index) => step(state(`RUN${index + 1}`), frame, 4)) },
      { id: "attack", loopMode: "once", steps: [step(state("ATK1"), "E", 10), step(state("ATK2"), "F", 8), step(state("ATK3"), "E", 8)] },
      { id: "pain", loopMode: "once", steps: [step(state("PAIN"), "G", 3), step(state("PAIN2"), "G", 3)] },
      { id: "death", loopMode: "once", steps: [step(state("DIE1"), "H", 5), step(state("DIE2"), "I", 5), step(state("DIE3"), "J", 5), step(state("DIE4"), "K", 5), step(state("DIE5"), "L", -1)] },
      { id: "gibDeath", loopMode: "once", steps: "MNOPQRSTU".split("").map((frame, index) => step(state(`XDIE${index + 1}`), frame, index === 8 ? -1 : 5)) },
      { id: "raise", loopMode: "once", steps: ["K", "J", "I", "H"].map((frame, index) => step(state(`RAISE${index + 1}`), frame, 5)) },
    ];
  }
  if (prefix === "SPOS") {
    return [
      { id: "idle", loopMode: "repeat", steps: [step(state("STND"), "A", 10), step(state("STND2"), "B", 10)] },
      { id: "walk", loopMode: "repeat", steps: ["A", "A", "B", "B", "C", "C", "D", "D"].map((frame, index) => step(state(`RUN${index + 1}`), frame, 3)) },
      { id: "attack", loopMode: "once", steps: [step(state("ATK1"), "E", 10), step(state("ATK2"), "F", 10, true), step(state("ATK3"), "E", 10)] },
      { id: "pain", loopMode: "once", steps: [step(state("PAIN"), "G", 3), step(state("PAIN2"), "G", 3)] },
      { id: "death", loopMode: "once", steps: [step(state("DIE1"), "H", 5), step(state("DIE2"), "I", 5), step(state("DIE3"), "J", 5), step(state("DIE4"), "K", 5), step(state("DIE5"), "L", -1)] },
      { id: "gibDeath", loopMode: "once", steps: "MNOPQRSTU".split("").map((frame, index) => step(state(`XDIE${index + 1}`), frame, index === 8 ? -1 : 5)) },
      { id: "raise", loopMode: "once", steps: ["L", "K", "J", "I", "H"].map((frame, index) => step(state(`RAISE${index + 1}`), frame, 5)) },
    ];
  }
  return [
    { id: "idle", loopMode: "repeat", steps: [step(state("STND"), "A", 10), step(state("STND2"), "B", 10)] },
    { id: "walk", loopMode: "repeat", steps: ["A", "A", "B", "B", "C", "C", "D", "D"].map((frame, index) => step(state(`RUN${index + 1}`), frame, 3)) },
    { id: "attack", loopMode: "once", steps: [step(state("ATK1"), "E", 8), step(state("ATK2"), "F", 8), step(state("ATK3"), "G", 6)] },
    { id: "pain", loopMode: "once", steps: [step(state("PAIN"), "H", 2), step(state("PAIN2"), "H", 2)] },
    { id: "death", loopMode: "once", steps: [step(state("DIE1"), "I", 8), step(state("DIE2"), "J", 8), step(state("DIE3"), "K", 6), step(state("DIE4"), "L", 6), step(state("DIE5"), "M", -1)] },
    { id: "gibDeath", loopMode: "once", steps: "NOPQRSTU".split("").map((frame, index) => step(state(`XDIE${index + 1}`), frame, index === 7 ? -1 : 5)) },
    { id: "raise", loopMode: "once", steps: [step(state("RAISE1"), "M", 8), step(state("RAISE2"), "L", 8), step(state("RAISE3"), "K", 6), step(state("RAISE4"), "J", 6), step(state("RAISE5"), "I", 6)] },
  ];
}

export const DOOM_SPRITE_FAMILY_DEFINITIONS: readonly DoomSpriteFamilyDefinition[] = [
  { prefix: "POSS", role: "actor", thingType: 3004, dimensionsDoomUnits: { radius: 20, height: 56 }, clips: actorClips("POSS") },
  { prefix: "SPOS", role: "actor", thingType: 9, dimensionsDoomUnits: { radius: 20, height: 56 }, clips: actorClips("SPOS") },
  { prefix: "TROO", role: "actor", thingType: 3001, dimensionsDoomUnits: { radius: 20, height: 56 }, clips: actorClips("TROO") },
  {
    prefix: "BAL1", role: "projectile", thingType: null, dimensionsDoomUnits: { radius: 6, height: 8 },
    clips: [
      { id: "flight", loopMode: "repeat", steps: [step("S_TBALL1", "A", 4, true), step("S_TBALL2", "B", 4, true)] },
      { id: "impact", loopMode: "once", steps: [step("S_TBALLX1", "C", 6, true), step("S_TBALLX2", "D", 6, true), step("S_TBALLX3", "E", 6, true)] },
    ],
  },
  { prefix: "BLUD", role: "effect", thingType: null, dimensionsDoomUnits: null, clips: [{ id: "hit", loopMode: "once", steps: [step("S_BLOOD1", "C", 8), step("S_BLOOD2", "B", 8), step("S_BLOOD3", "A", 8)] }] },
  { prefix: "PUFF", role: "effect", thingType: null, dimensionsDoomUnits: null, clips: [{ id: "impact", loopMode: "once", steps: [step("S_PUFF1", "A", 4, true), step("S_PUFF2", "B", 4), step("S_PUFF3", "C", 4), step("S_PUFF4", "D", 4)] }] },
  {
    prefix: "PUNG", role: "weapon", thingType: null, dimensionsDoomUnits: null,
    clips: [
      { id: "ready", loopMode: "repeat", steps: [step("S_PUNCH", "A", 1)] },
      { id: "fire", loopMode: "once", steps: [step("S_PUNCH1", "B", 4), step("S_PUNCH2", "C", 4), step("S_PUNCH3", "D", 5), step("S_PUNCH4", "C", 4), step("S_PUNCH5", "B", 5)] },
    ],
  },
  {
    prefix: "PISG", role: "weapon", thingType: null, dimensionsDoomUnits: null,
    clips: [
      { id: "ready", loopMode: "repeat", steps: [step("S_PISTOL", "A", 1)] },
      { id: "fire", loopMode: "once", steps: [step("S_PISTOL2", "B", 6), step("S_PISTOL3", "C", 4), step("S_PISTOL4", "B", 5)] },
    ],
  },
  {
    prefix: "PISF", role: "weapon", thingType: null, dimensionsDoomUnits: null,
    clips: [{ id: "flash", loopMode: "once", steps: [step("S_PISTOLFLASH", "A", 7, true)] }],
  },
  {
    prefix: "SHTG", role: "weapon", thingType: null, dimensionsDoomUnits: null,
    clips: [
      { id: "ready", loopMode: "repeat", steps: [step("S_SGUN", "A", 1)] },
      { id: "fire", loopMode: "once", steps: [step("S_SGUN2", "A", 7), step("S_SGUN3", "B", 5), step("S_SGUN4", "C", 5), step("S_SGUN5", "D", 4), step("S_SGUN6", "C", 5), step("S_SGUN7", "B", 5), step("S_SGUN8", "A", 3), step("S_SGUN9", "A", 7)] },
    ],
  },
  {
    prefix: "SHTF", role: "weapon", thingType: null, dimensionsDoomUnits: null,
    clips: [{ id: "flash", loopMode: "once", steps: [step("S_SGUNFLASH1", "A", 4, true), step("S_SGUNFLASH2", "B", 3, true)] }],
  },
  { prefix: "SHOT", role: "item", thingType: 2001, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_SHOT", "A", -1)] }] },
  { prefix: "CLIP", role: "item", thingType: 2007, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_CLIP", "A", -1)] }] },
  { prefix: "SHEL", role: "item", thingType: 2008, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_SHEL", "A", -1)] }] },
  { prefix: "AMMO", role: "item", thingType: 2048, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_AMMO", "A", -1)] }] },
  { prefix: "SBOX", role: "item", thingType: 2049, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_SBOX", "A", -1)] }] },
  { prefix: "STIM", role: "item", thingType: 2011, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_STIM", "A", -1)] }] },
  { prefix: "MEDI", role: "item", thingType: 2012, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_MEDI", "A", -1)] }] },
  { prefix: "BON1", role: "item", thingType: 2014, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_BON1", "A", 6), step("S_BON1A", "B", 6), step("S_BON1B", "C", 6), step("S_BON1C", "D", 6), step("S_BON1B", "C", 6), step("S_BON1A", "B", 6)] }] },
  { prefix: "BON2", role: "item", thingType: 2015, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_BON2", "A", 6), step("S_BON2A", "B", 6), step("S_BON2B", "C", 6), step("S_BON2C", "D", 6), step("S_BON2B", "C", 6), step("S_BON2A", "B", 6)] }] },
  { prefix: "ARM1", role: "item", thingType: 2018, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_ARM1", "A", 6), step("S_ARM1A", "B", 7, true)] }] },
  { prefix: "ARM2", role: "item", thingType: 2019, dimensionsDoomUnits: { radius: 20, height: 16 }, clips: [{ id: "available", loopMode: "repeat", steps: [step("S_ARM2", "A", 6), step("S_ARM2A", "B", 6, true)] }] },
];

export interface ParsedSpriteLumpAssignment {
  readonly frame: string;
  readonly rotation: number;
  readonly mirrored: boolean;
}

/** Decode Doom's `A1`, `A2A8`, and `A0` suffix contract. */
export function parseSpriteLumpAssignments(
  prefix: string,
  lumpName: string,
): readonly ParsedSpriteLumpAssignment[] {
  if (!lumpName.startsWith(prefix)) throw new Error(`${lumpName} is not a ${prefix} sprite lump`);
  const suffix = lumpName.slice(prefix.length);
  if (!/^[A-Z][0-8](?:[A-Z][1-8])?$/u.test(suffix)) {
    throw new Error(`${lumpName} has an invalid Doom sprite suffix`);
  }
  const assignments: ParsedSpriteLumpAssignment[] = [
    { frame: suffix[0]!, rotation: Number(suffix[1]), mirrored: false },
  ];
  if (suffix.length === 4) {
    assignments.push({ frame: suffix[2]!, rotation: Number(suffix[3]), mirrored: true });
  }
  return assignments;
}
