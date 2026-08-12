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
] as const;

export interface DoomSpriteClipStep {
  readonly state: string;
  readonly frame: string;
  readonly tics: number | null;
  readonly fullBright: boolean;
}

export interface DoomSpriteClipDefinition {
  readonly id: string;
  readonly loopMode: "once" | "repeat";
  readonly steps: readonly DoomSpriteClipStep[];
}

export interface DoomSpriteFamilyDefinition {
  readonly prefix: string;
  readonly role: "actor" | "projectile" | "effect";
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
  tics: number | null,
  fullBright = false,
): DoomSpriteClipStep => ({ state, frame, tics, fullBright });

function actorClips(
  prefix: "POSS" | "SPOS" | "TROO",
): readonly DoomSpriteClipDefinition[] {
  const p = prefix.toLowerCase();
  if (prefix === "POSS") {
    return [
      { id: "idle", loopMode: "repeat", steps: [step(`${p}Stnd`, "A", 10), step(`${p}Stnd2`, "B", 10)] },
      { id: "walk", loopMode: "repeat", steps: ["A", "A", "B", "B", "C", "C", "D", "D"].map((frame, index) => step(`${p}Run${index + 1}`, frame, 4)) },
      { id: "attack", loopMode: "once", steps: [step(`${p}Atk1`, "E", 10), step(`${p}Atk2`, "F", 8), step(`${p}Atk3`, "E", 8)] },
      { id: "pain", loopMode: "once", steps: [step(`${p}Pain`, "G", 3), step(`${p}Pain2`, "G", 3)] },
      { id: "death", loopMode: "once", steps: [step(`${p}Die1`, "H", 5), step(`${p}Die2`, "I", 5), step(`${p}Die3`, "J", 5), step(`${p}Die4`, "K", 5), step(`${p}Die5`, "L", null)] },
      { id: "gibDeath", loopMode: "once", steps: "MNOPQRSTU".split("").map((frame, index) => step(`${p}Xdie${index + 1}`, frame, index === 8 ? null : 5)) },
      { id: "raise", loopMode: "once", steps: ["K", "J", "I", "H"].map((frame, index) => step(`${p}Raise${index + 1}`, frame, 5)) },
    ];
  }
  if (prefix === "SPOS") {
    return [
      { id: "idle", loopMode: "repeat", steps: [step(`${p}Stnd`, "A", 10), step(`${p}Stnd2`, "B", 10)] },
      { id: "walk", loopMode: "repeat", steps: ["A", "A", "B", "B", "C", "C", "D", "D"].map((frame, index) => step(`${p}Run${index + 1}`, frame, 3)) },
      { id: "attack", loopMode: "once", steps: [step(`${p}Atk1`, "E", 10), step(`${p}Atk2`, "F", 10, true), step(`${p}Atk3`, "E", 10)] },
      { id: "pain", loopMode: "once", steps: [step(`${p}Pain`, "G", 3), step(`${p}Pain2`, "G", 3)] },
      { id: "death", loopMode: "once", steps: [step(`${p}Die1`, "H", 5), step(`${p}Die2`, "I", 5), step(`${p}Die3`, "J", 5), step(`${p}Die4`, "K", 5), step(`${p}Die5`, "L", null)] },
      { id: "gibDeath", loopMode: "once", steps: "MNOPQRSTU".split("").map((frame, index) => step(`${p}Xdie${index + 1}`, frame, index === 8 ? null : 5)) },
      { id: "raise", loopMode: "once", steps: ["L", "K", "J", "I", "H"].map((frame, index) => step(`${p}Raise${index + 1}`, frame, 5)) },
    ];
  }
  return [
    { id: "idle", loopMode: "repeat", steps: [step(`${p}Stnd`, "A", 10), step(`${p}Stnd2`, "B", 10)] },
    { id: "walk", loopMode: "repeat", steps: ["A", "A", "B", "B", "C", "C", "D", "D"].map((frame, index) => step(`${p}Run${index + 1}`, frame, 3)) },
    { id: "attack", loopMode: "once", steps: [step(`${p}Atk1`, "E", 8), step(`${p}Atk2`, "F", 8), step(`${p}Atk3`, "G", 6)] },
    { id: "pain", loopMode: "once", steps: [step(`${p}Pain`, "H", 2), step(`${p}Pain2`, "H", 2)] },
    { id: "death", loopMode: "once", steps: [step(`${p}Die1`, "I", 8), step(`${p}Die2`, "J", 8), step(`${p}Die3`, "K", 6), step(`${p}Die4`, "L", 6), step(`${p}Die5`, "M", null)] },
    { id: "gibDeath", loopMode: "once", steps: "NOPQRSTU".split("").map((frame, index) => step(`${p}Xdie${index + 1}`, frame, index === 7 ? null : 5)) },
    { id: "raise", loopMode: "once", steps: [step(`${p}Raise1`, "M", 8), step(`${p}Raise2`, "L", 8), step(`${p}Raise3`, "K", 6), step(`${p}Raise4`, "J", 6), step(`${p}Raise5`, "I", 6)] },
  ];
}

export const DOOM_SPRITE_FAMILY_DEFINITIONS: readonly DoomSpriteFamilyDefinition[] = [
  { prefix: "POSS", role: "actor", thingType: 3004, dimensionsDoomUnits: { radius: 20, height: 56 }, clips: actorClips("POSS") },
  { prefix: "SPOS", role: "actor", thingType: 9, dimensionsDoomUnits: { radius: 20, height: 56 }, clips: actorClips("SPOS") },
  { prefix: "TROO", role: "actor", thingType: 3001, dimensionsDoomUnits: { radius: 20, height: 56 }, clips: actorClips("TROO") },
  {
    prefix: "BAL1", role: "projectile", thingType: null, dimensionsDoomUnits: { radius: 6, height: 8 },
    clips: [
      { id: "flight", loopMode: "repeat", steps: [step("tball1", "A", 4, true), step("tball2", "B", 4, true)] },
      { id: "impact", loopMode: "once", steps: [step("tballx1", "C", 6, true), step("tballx2", "D", 6, true), step("tballx3", "E", 6, true)] },
    ],
  },
  { prefix: "BLUD", role: "effect", thingType: null, dimensionsDoomUnits: null, clips: [{ id: "hit", loopMode: "once", steps: [step("blood1", "C", 8), step("blood2", "B", 8), step("blood3", "A", 8)] }] },
  { prefix: "PUFF", role: "effect", thingType: null, dimensionsDoomUnits: null, clips: [{ id: "impact", loopMode: "once", steps: [step("puff1", "A", 4, true), step("puff2", "B", 4), step("puff3", "C", 4), step("puff4", "D", 4)] }] },
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
