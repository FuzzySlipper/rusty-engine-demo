import {
  renderHandle,
  type Material,
  type RenderDiff,
  type RenderFrameDiff,
  type RenderHandle,
  type RenderMetadata,
  type RenderNode,
  type Transform,
} from "@rusty-engine/render-contracts";

import type { RuntimeBrowserState } from "./projection.js";

export type WeaponViewmodelImpulse = "idle" | "attack" | "dry";

export interface WeaponViewmodelReadout {
  readonly bobPhase: number;
  readonly impulse: WeaponViewmodelImpulse;
  readonly liveNodeCount: number;
  readonly mounted: boolean;
  readonly visible: boolean;
  readonly weapon: string | null;
}

export interface WeaponViewmodelPlan extends RenderFrameDiff {
  /** Advance the adapter baseline only after the shared renderer accepts it. */
  readonly commit: () => void;
}

interface WeaponPart {
  readonly color: Material["color"];
  readonly label: string;
  readonly scale: readonly [number, number, number];
  readonly translation: readonly [number, number, number];
  readonly visible: boolean;
}

interface WeaponPreset {
  readonly item: string;
  readonly parts: readonly [
    WeaponPart,
    WeaponPart,
    WeaponPart,
    WeaponPart,
    WeaponPart,
  ];
  readonly muzzle: readonly [number, number, number];
}

interface ViewmodelState {
  readonly bobPhase: number;
  readonly flashIntensity: number;
  readonly impulse: WeaponViewmodelImpulse;
  readonly mounted: boolean;
  readonly moving: boolean;
  readonly visible: boolean;
  readonly weapon: WeaponPreset | null;
}

const VIEWMODEL_ROOT = renderHandle(9_000_000);
const VIEWMODEL_PARTS = [
  renderHandle(9_000_001),
  renderHandle(9_000_002),
  renderHandle(9_000_003),
  renderHandle(9_000_004),
  renderHandle(9_000_005),
] as const;
const VIEWMODEL_MUZZLE = renderHandle(9_000_006);
const VIEWMODEL_HANDLES = [...VIEWMODEL_PARTS, VIEWMODEL_MUZZLE] as const;
const IDENTITY_ROTATION = [0, 0, 0, 1] as const;
const HIDDEN_PART: WeaponPart = {
  color: [0, 0, 0, 0],
  label: "unused",
  scale: [0.001, 0.001, 0.001],
  translation: [0, 0, 0],
  visible: false,
};

const WEAPON_PRESETS: Readonly<Record<string, WeaponPreset>> = {
  "weapon/arc-pistol": {
    item: "weapon/arc-pistol",
    muzzle: [0.23, -0.14, -1.08],
    parts: [
      part(
        "receiver",
        [0.23, -0.21, -0.7],
        [0.19, 0.14, 0.34],
        [0.15, 0.3, 0.34, 1],
      ),
      part(
        "barrel",
        [0.23, -0.16, -0.91],
        [0.08, 0.07, 0.27],
        [0.16, 0.73, 0.92, 1],
      ),
      part(
        "grip",
        [0.23, -0.39, -0.59],
        [0.12, 0.28, 0.13],
        [0.1, 0.13, 0.16, 1],
      ),
      part(
        "coil",
        [0.23, -0.09, -0.66],
        [0.13, 0.035, 0.14],
        [0.22, 0.94, 1, 1],
      ),
      part(
        "sight",
        [0.23, -0.055, -0.83],
        [0.025, 0.04, 0.11],
        [0.88, 0.97, 1, 1],
      ),
    ],
  },
  "weapon/breach-scattergun": {
    item: "weapon/breach-scattergun",
    muzzle: [0.28, -0.19, -1.34],
    parts: [
      part(
        "receiver",
        [0.28, -0.29, -0.73],
        [0.24, 0.18, 0.42],
        [0.22, 0.18, 0.13, 1],
      ),
      part(
        "upper-barrel",
        [0.24, -0.2, -1.03],
        [0.075, 0.075, 0.57],
        [0.62, 0.57, 0.48, 1],
      ),
      part(
        "lower-barrel",
        [0.32, -0.2, -1.03],
        [0.075, 0.075, 0.57],
        [0.52, 0.48, 0.42, 1],
      ),
      part(
        "pump",
        [0.28, -0.35, -0.96],
        [0.26, 0.14, 0.25],
        [0.68, 0.32, 0.13, 1],
      ),
      part(
        "stock",
        [0.28, -0.43, -0.45],
        [0.21, 0.19, 0.31],
        [0.36, 0.19, 0.1, 1],
      ),
    ],
  },
  "weapon/rivet-carbine": {
    item: "weapon/rivet-carbine",
    muzzle: [0.27, -0.13, -1.24],
    parts: [
      part(
        "receiver",
        [0.27, -0.25, -0.72],
        [0.23, 0.17, 0.46],
        [0.13, 0.26, 0.29, 1],
      ),
      part(
        "barrel",
        [0.27, -0.14, -1.03],
        [0.065, 0.065, 0.51],
        [0.32, 0.64, 0.66, 1],
      ),
      part(
        "shroud",
        [0.27, -0.2, -0.91],
        [0.16, 0.12, 0.3],
        [0.15, 0.43, 0.44, 1],
      ),
      part(
        "magazine",
        [0.27, -0.46, -0.7],
        [0.12, 0.31, 0.16],
        [0.75, 0.36, 0.12, 1],
      ),
      part(
        "sight",
        [0.27, -0.055, -0.71],
        [0.035, 0.07, 0.18],
        [0.88, 0.58, 0.19, 1],
      ),
    ],
  },
};

const EMPTY_STATE: ViewmodelState = {
  bobPhase: 0,
  flashIntensity: 1,
  impulse: "idle",
  mounted: false,
  moving: false,
  visible: false,
  weapon: null,
};

/**
 * Game-specific descriptor owner for the shared renderer's bounded,
 * camera-relative viewmodel layer. It derives only disposable transforms from
 * accepted Rust state and facts; it has no input, aim, damage, or clock
 * authority.
 */
export class WeaponViewmodelAdapter {
  #revision = 0;
  #state: ViewmodelState = EMPTY_STATE;

  project(
    state: RuntimeBrowserState,
    reset = false,
    flashIntensity = 1,
  ): WeaponViewmodelPlan {
    const weapon = WEAPON_PRESETS[state.weapon.item] ?? null;
    const moving =
      state.playerMotionState === "moved" ||
      state.presentation.cues.some(
        (cue) => cue.kind === "movement" && cue.entity === state.player.id,
      );
    const attack = state.presentation.cues.some(
      (cue) => cue.kind === "attack" && cue.attacker === state.player.id,
    );
    const dry = state.presentation.cues.some(
      (cue) => cue.kind === "dryFire" && cue.attacker === state.player.id,
    );
    const next: ViewmodelState = {
      bobPhase: reset
        ? 0
        : moving
          ? (this.#state.bobPhase + 0.82) % (Math.PI * 2)
          : this.#state.bobPhase,
      flashIntensity: Number.isFinite(flashIntensity)
        ? Math.min(1, Math.max(0, flashIntensity))
        : 1,
      impulse: reset
        ? "idle"
        : attack
          ? "attack"
          : dry
            ? "dry"
            : this.#state.impulse,
      mounted: weapon !== null,
      moving: !reset && moving,
      visible: weapon !== null && state.player.vitalityState === "alive",
      weapon,
    };
    return this.#plan(next);
  }

  clearImpulse(): WeaponViewmodelPlan {
    return this.#plan({ ...this.#state, impulse: "idle" });
  }

  destroy(): WeaponViewmodelPlan {
    return this.#plan(EMPTY_STATE);
  }

  readout(): WeaponViewmodelReadout {
    return {
      bobPhase: this.#state.bobPhase,
      impulse: this.#state.impulse,
      liveNodeCount: this.#state.mounted ? VIEWMODEL_HANDLES.length + 1 : 0,
      mounted: this.#state.mounted,
      visible: this.#state.visible,
      weapon: this.#state.weapon?.item ?? null,
    };
  }

  #plan(next: ViewmodelState): WeaponViewmodelPlan {
    const baseRevision = this.#revision;
    const previous = this.#state;
    const ops: RenderDiff[] = [];
    if (!previous.mounted && next.mounted) {
      ops.push({
        op: "create",
        handle: VIEWMODEL_ROOT,
        parent: null,
        node: rootNode(next),
      });
      VIEWMODEL_PARTS.forEach((handle, index) => {
        ops.push({
          op: "create",
          handle,
          parent: VIEWMODEL_ROOT,
          node: partNode(next, index),
        });
      });
      ops.push({
        op: "create",
        handle: VIEWMODEL_MUZZLE,
        parent: VIEWMODEL_ROOT,
        node: muzzleNode(next),
      });
    } else if (previous.mounted && !next.mounted) {
      for (const handle of [...VIEWMODEL_HANDLES].reverse()) {
        ops.push({ op: "destroy", handle });
      }
      ops.push({ op: "destroy", handle: VIEWMODEL_ROOT });
    } else if (previous.mounted && next.mounted) {
      const previousRoot = rootNode(previous);
      const nextRoot = rootNode(next);
      if (!sameNodePresentation(previousRoot, nextRoot)) {
        ops.push(updateNode(VIEWMODEL_ROOT, nextRoot));
      }
      VIEWMODEL_PARTS.forEach((handle, index) => {
        const previousPart = partNode(previous, index);
        const nextPart = partNode(next, index);
        if (!sameNodePresentation(previousPart, nextPart)) {
          ops.push(updateNode(handle, nextPart));
        }
      });
      const previousMuzzle = muzzleNode(previous);
      const nextMuzzle = muzzleNode(next);
      if (!sameNodePresentation(previousMuzzle, nextMuzzle)) {
        ops.push(updateNode(VIEWMODEL_MUZZLE, nextMuzzle));
      }
    }
    let committed = false;
    return {
      schemaVersion: 1,
      ops,
      commit: () => {
        if (committed) {
          return;
        }
        if (this.#revision !== baseRevision) {
          throw new Error("cannot commit a stale weapon viewmodel plan");
        }
        this.#state = next;
        this.#revision += 1;
        committed = true;
      },
    };
  }
}

function part(
  label: string,
  translation: readonly [number, number, number],
  scale: readonly [number, number, number],
  color: Material["color"],
): WeaponPart {
  return { color, label, scale, translation, visible: true };
}

function rootNode(state: ViewmodelState): RenderNode {
  return {
    geometry: { kind: "group" },
    material: { color: [0, 0, 0, 0], wireframe: false },
    transform: rootTransform(state),
    visible: state.visible,
    layer: "viewmodel",
    metadata: metadata("loading-bay-viewmodel-root", [
      "loading-bay",
      "weapon-viewmodel",
      state.weapon?.item ?? "unavailable",
    ]),
  };
}

function partNode(state: ViewmodelState, index: number): RenderNode {
  const part = state.weapon?.parts[index] ?? HIDDEN_PART;
  return {
    geometry: { kind: "cube" },
    material: { color: part.color, wireframe: false },
    transform: transform(part.translation, part.scale),
    visible: state.visible && part.visible,
    layer: "viewmodel",
    metadata: metadata(`loading-bay-viewmodel-${part.label}`, [
      "loading-bay",
      "weapon-viewmodel",
      state.weapon?.item ?? "unavailable",
    ]),
  };
}

function muzzleNode(state: ViewmodelState): RenderNode {
  const intensity = state.flashIntensity;
  const size = Math.max(0.001, 0.11 * intensity);
  return {
    geometry: { kind: "sphere" },
    material: {
      color: [1, 0.86, 0.28, 0.92 * intensity],
      wireframe: false,
    },
    transform: transform(state.weapon?.muzzle ?? [0, 0, 0], [size, size, size]),
    visible: state.visible && state.impulse === "attack" && intensity > 0,
    layer: "viewmodel",
    metadata: metadata("loading-bay-viewmodel-muzzle-flash", [
      "loading-bay",
      "weapon-viewmodel",
      "muzzle-flash",
      state.weapon?.item ?? "unavailable",
    ]),
  };
}

function rootTransform(state: ViewmodelState): Transform {
  const moving = state.moving && state.mounted && state.visible;
  const bobX = moving ? Math.sin(state.bobPhase) * 0.012 : 0;
  const bobY = moving ? Math.abs(Math.cos(state.bobPhase)) * 0.014 - 0.007 : 0;
  const recoil =
    state.impulse === "attack" ? 0.09 : state.impulse === "dry" ? 0.025 : 0;
  const angle =
    state.impulse === "attack"
      ? (-5 * Math.PI) / 180
      : state.impulse === "dry"
        ? (-1.5 * Math.PI) / 180
        : 0;
  return {
    translation: [bobX, bobY - recoil * 0.25, recoil],
    rotation: [Math.sin(angle / 2), 0, 0, Math.cos(angle / 2)],
    scale: [1, 1, 1],
  };
}

function transform(
  translation: readonly [number, number, number],
  scale: readonly [number, number, number],
): Transform {
  return { translation, rotation: IDENTITY_ROTATION, scale };
}

function metadata(label: string, tags: readonly string[]): RenderMetadata {
  return {
    sourceEntity: null,
    sourceSceneNode: null,
    tags,
    label,
  };
}

function updateNode(handle: RenderHandle, node: RenderNode): RenderDiff {
  return {
    op: "update",
    handle,
    transform: node.transform,
    material: node.material,
    visible: node.visible,
    metadata: node.metadata,
  };
}

function sameNodePresentation(left: RenderNode, right: RenderNode): boolean {
  return (
    left.visible === right.visible &&
    JSON.stringify(left.transform) === JSON.stringify(right.transform) &&
    JSON.stringify(left.material) === JSON.stringify(right.material) &&
    JSON.stringify(left.metadata) === JSON.stringify(right.metadata)
  );
}
