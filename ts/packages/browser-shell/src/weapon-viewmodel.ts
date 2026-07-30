import {
  renderHandle,
  type RenderDiff,
  type RenderFrameDiff,
  type RenderHandle,
  type RenderMetadata,
  type RenderNode,
  type StaticMeshInstanceDescriptor,
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

interface WeaponPreset {
  readonly item: string;
  readonly asset: string;
  readonly translation: readonly [number, number, number];
  readonly scale: readonly [number, number, number];
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
const VIEWMODEL_WEAPON = renderHandle(9_000_001);
const VIEWMODEL_MUZZLE = renderHandle(9_000_002);
const IDENTITY_ROTATION = [0, 0, 0, 1] as const;

const WEAPON_PRESETS: Readonly<Record<string, WeaponPreset>> = {
  "weapon/arc-pistol": {
    item: "weapon/arc-pistol",
    asset: "mesh/prop-kit/arc-pistol",
    translation: [0.23, -0.24, -0.78],
    scale: [0.72, 0.72, 0.72],
    muzzle: [0.23, -0.14, -1.08],
  },
  "weapon/breach-scattergun": {
    item: "weapon/breach-scattergun",
    asset: "mesh/prop-kit/breach-scattergun",
    translation: [0.28, -0.27, -0.78],
    scale: [0.7, 0.7, 0.7],
    muzzle: [0.28, -0.19, -1.34],
  },
  "weapon/rivet-carbine": {
    item: "weapon/rivet-carbine",
    asset: "mesh/prop-kit/rivet-carbine",
    translation: [0.27, -0.23, -0.77],
    scale: [0.72, 0.72, 0.72],
    muzzle: [0.27, -0.13, -1.24],
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
 * camera-relative viewmodel layer. Geometry is always a serialized project
 * asset; this adapter derives only disposable transforms and visibility from
 * accepted Rust state and facts.
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
      liveNodeCount: this.#state.mounted ? 3 : 0,
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
      ops.push({
        op: "createStaticMeshInstance",
        handle: VIEWMODEL_WEAPON,
        parent: VIEWMODEL_ROOT,
        instance: weaponInstance(next),
      });
      ops.push({
        op: "createStaticMeshInstance",
        handle: VIEWMODEL_MUZZLE,
        parent: VIEWMODEL_ROOT,
        instance: muzzleInstance(next),
      });
    } else if (previous.mounted && !next.mounted) {
      ops.push({ op: "destroy", handle: VIEWMODEL_MUZZLE });
      ops.push({ op: "destroy", handle: VIEWMODEL_WEAPON });
      ops.push({ op: "destroy", handle: VIEWMODEL_ROOT });
    } else if (previous.mounted && next.mounted) {
      const previousRoot = rootNode(previous);
      const nextRoot = rootNode(next);
      if (!samePresentation(previousRoot, nextRoot)) {
        ops.push(updateNode(VIEWMODEL_ROOT, nextRoot));
      }
      if (previous.weapon?.asset !== next.weapon?.asset) {
        ops.push({ op: "destroy", handle: VIEWMODEL_WEAPON });
        ops.push({
          op: "createStaticMeshInstance",
          handle: VIEWMODEL_WEAPON,
          parent: VIEWMODEL_ROOT,
          instance: weaponInstance(next),
        });
      } else {
        const previousWeapon = weaponInstance(previous);
        const nextWeapon = weaponInstance(next);
        if (!samePresentation(previousWeapon, nextWeapon)) {
          ops.push(updateInstance(VIEWMODEL_WEAPON, nextWeapon));
        }
      }
      const previousMuzzle = muzzleInstance(previous);
      const nextMuzzle = muzzleInstance(next);
      if (!samePresentation(previousMuzzle, nextMuzzle)) {
        ops.push(updateInstance(VIEWMODEL_MUZZLE, nextMuzzle));
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

function weaponInstance(state: ViewmodelState): StaticMeshInstanceDescriptor {
  const weapon = requiredWeapon(state);
  return {
    asset: weapon.asset,
    transform: transform(weapon.translation, weapon.scale),
    visible: state.visible,
    materialOverrides: [],
    metadata: metadata("loading-bay-viewmodel-weapon", [
      "loading-bay",
      "weapon-viewmodel",
      "serialized-asset",
      weapon.item,
    ]),
  };
}

function muzzleInstance(state: ViewmodelState): StaticMeshInstanceDescriptor {
  const weapon = requiredWeapon(state);
  const intensity = state.flashIntensity;
  const size = Math.max(0.001, 0.55 * intensity);
  return {
    asset: "mesh/prop-kit/muzzle-flash",
    transform: transform(weapon.muzzle, [size, size, size]),
    visible: state.visible && state.impulse === "attack" && intensity > 0,
    materialOverrides: [],
    metadata: metadata("loading-bay-viewmodel-muzzle-flash", [
      "loading-bay",
      "weapon-viewmodel",
      "muzzle-flash",
      "serialized-asset",
      weapon.item,
    ]),
  };
}

function requiredWeapon(state: ViewmodelState): WeaponPreset {
  if (state.weapon === null) {
    throw new Error("mounted weapon viewmodel has no serialized asset");
  }
  return state.weapon;
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

function updateInstance(
  handle: RenderHandle,
  instance: StaticMeshInstanceDescriptor,
): RenderDiff {
  return {
    op: "update",
    handle,
    transform: instance.transform,
    material: null,
    visible: instance.visible,
    metadata: instance.metadata,
  };
}

function samePresentation(
  left: RenderNode | StaticMeshInstanceDescriptor,
  right: RenderNode | StaticMeshInstanceDescriptor,
): boolean {
  return (
    left.visible === right.visible &&
    JSON.stringify(left.transform) === JSON.stringify(right.transform) &&
    JSON.stringify(left.metadata) === JSON.stringify(right.metadata)
  );
}
