import {
  entityId,
  renderHandle,
  type Geometry,
  type Material,
  type MeshPayloadDescriptor,
  type RenderDiff,
  type RenderFrameDiff,
  type RenderHandle,
  type RenderNode,
  type Transform,
} from "@rusty-engine-demo/render-contracts";

export interface RuntimeProjectionNode {
  readonly id: number;
  readonly name: string;
  readonly asset: string;
  readonly translation: readonly [number, number, number] | null;
  readonly visible: boolean;
}

export interface RuntimeEnemyState {
  readonly id: number;
  readonly name: string;
  readonly state: "alive" | "defeated";
  readonly position: readonly [number, number, number];
  readonly currentHealth: number;
  readonly maxHealth: number;
}

export interface RuntimePlayerBindings {
  readonly moveForward: string;
  readonly moveBackward: string;
  readonly moveLeft: string;
  readonly moveRight: string;
  readonly mouseLook: string;
  readonly primaryFire: string;
}

export interface RuntimePlayerState {
  readonly id: number;
  readonly position: readonly [number, number, number];
  readonly yawDegrees: number;
  readonly pitchDegrees: number;
  readonly moveStepSeconds: number;
  readonly lookDegreesPerUnit: number;
  readonly bindings: RuntimePlayerBindings;
}

export interface RuntimeWeaponState {
  readonly damage: number;
  readonly ammoRemaining: number;
  readonly ammoCapacity: number;
  readonly readyAtTick: number;
}

export interface RuntimeExtractionBeaconState {
  readonly id: number;
  readonly state: "standby" | "active";
  readonly activationRadius: number;
  readonly activatedBy: number | null;
  readonly activatedAtTick: number | null;
}

export interface DerivedCameraPose {
  readonly position: readonly [number, number, number];
  readonly yawDegrees: number;
  readonly pitchDegrees: number;
}

export interface RuntimeVoxelMeshGroup {
  readonly materialSlot: number;
  readonly start: number;
  readonly count: number;
}

export interface RuntimeVoxelMeshChunk {
  readonly chunk: readonly [number, number, number];
  readonly contentHash: string;
  readonly translation: readonly [number, number, number];
  readonly positions: readonly number[];
  readonly normals: readonly number[];
  readonly indices: readonly number[];
  readonly groups: readonly RuntimeVoxelMeshGroup[];
  readonly boundsMin: readonly [number, number, number];
  readonly boundsMax: readonly [number, number, number];
}

export interface RuntimeGeneratedEnvironment {
  readonly seed: number;
  readonly outputHash: string;
  readonly solidVoxels: number;
  readonly meshVertices: number;
  readonly meshQuads: number;
}

export interface RuntimeAnimationState {
  readonly entity: number;
  readonly posture: "idle" | "moving" | "defeated" | "open" | "closed" | "standby" | "active";
}

export type RuntimeFeedbackCue =
  | {
      readonly kind: "movement";
      readonly entity: number;
      readonly from: readonly [number, number, number];
      readonly to: readonly [number, number, number];
    }
  | { readonly kind: "movementBlocked"; readonly entity: number }
  | {
      readonly kind: "attack";
      readonly attacker: number;
      readonly origin: readonly [number, number, number];
      readonly direction: readonly [number, number, number];
    }
  | {
      readonly kind: "damage";
      readonly attacker: number;
      readonly target: number;
      readonly amount: number;
      readonly remaining: number;
    }
  | { readonly kind: "defeat"; readonly attacker: number | null; readonly entity: number }
  | { readonly kind: "doorChanged"; readonly entity: number; readonly state: "open" | "closed" }
  | { readonly kind: "extractionBeaconActivated"; readonly entity: number; readonly actor: number };

export interface RuntimePresentationState {
  readonly animationStates: readonly RuntimeAnimationState[];
  readonly cues: readonly RuntimeFeedbackCue[];
}

export interface RuntimeBrowserState {
  readonly tick: number;
  readonly entityRevision: number;
  readonly voxelRevision: number;
  readonly voxelAuthorityHash: string;
  readonly voxelSolidCount: number;
  readonly voxelNavigationHash: string;
  readonly voxelProbePathLength: number;
  readonly projection: readonly RuntimeProjectionNode[];
  readonly doorState: "closed" | "open";
  readonly encounterState: "active" | "cleared";
  readonly motionState: "moving" | "blocked";
  readonly navigationState: "following" | "arrived" | "blocked" | "unreachable";
  readonly playerMotionState: "idle" | "moved" | "blocked";
  readonly combatState: "ready" | "hit" | "missed";
  readonly player: RuntimePlayerState;
  readonly weapon: RuntimeWeaponState;
  readonly extractionBeacon: RuntimeExtractionBeaconState | null;
  readonly voxelMeshes: readonly RuntimeVoxelMeshChunk[];
  readonly generatedEnvironment: RuntimeGeneratedEnvironment | null;
  readonly enemies: readonly RuntimeEnemyState[];
  readonly presentation: RuntimePresentationState;
  readonly lastEvents: readonly string[];
  readonly voxelEditReceipt?: RuntimeVoxelEditReceipt;
}

export interface RuntimeVoxelEditReceipt {
  readonly revisionBefore: number;
  readonly acceptedRevision: number;
  readonly changedVoxels: number;
  readonly changedMin: readonly [number, number, number];
  readonly changedMaxInclusive: readonly [number, number, number];
  readonly authorityHash: string;
  readonly persistedToProject: boolean;
}

/** Presentation-only follow camera rebuilt from the accepted Rust player pose. */
export function derivePlayerCameraPose(
  player: RuntimePlayerState,
  height = 1.2,
  followDistance = 1,
): DerivedCameraPose {
  const yawRadians = (player.yawDegrees * Math.PI) / 180;
  const forwardX = -Math.sin(yawRadians);
  const forwardZ = -Math.cos(yawRadians);
  return {
    position: [
      player.position[0] - forwardX * followDistance,
      player.position[1] + height,
      player.position[2] - forwardZ * followDistance,
    ],
    yawDegrees: player.yawDegrees,
    pitchDegrees: player.pitchDegrees,
  };
}

const ENTITY_HANDLE_OFFSET = 100_000;
const FIRST_VOXEL_MESH_HANDLE = 800_000;

/** Stateful adapter from whole Rust projection readouts to retained renderer diffs. */
export class RuntimeProjectionAdapter {
  readonly #known = new Map<
    number,
    { readonly node: RuntimeProjectionNode; readonly beaconState: "standby" | "active" | null }
  >();
  readonly #meshHashes = new Map<string, string>();
  readonly #meshHandles = new Map<string, RenderHandle>();
  #nextMeshHandle = FIRST_VOXEL_MESH_HANDLE;

  apply(state: RuntimeBrowserState): RenderFrameDiff {
    const ops: RenderDiff[] = [];
    const incomingMeshes = new Set<string>();
    for (const mesh of state.voxelMeshes) {
      const key = mesh.chunk.join(",");
      incomingMeshes.add(key);
      let handle = this.#meshHandles.get(key);
      if (handle === undefined) {
        handle = renderHandle(this.#nextMeshHandle);
        this.#nextMeshHandle += 1;
        this.#meshHandles.set(key, handle);
        ops.push({
          op: "create",
          handle,
          parent: null,
          node: primitiveNode(
            `generated-room-chunk-${mesh.chunk.join("-")}`,
            null,
            "cube",
            mesh.translation,
            [1, 1, 1],
            { color: [0.68, 0.78, 0.75, 1], wireframe: false },
          ),
        });
      }
      if (this.#meshHashes.get(key) !== mesh.contentHash) {
        ops.push({ op: "replaceMeshPayload", handle, payload: meshPayload(mesh) });
        this.#meshHashes.set(key, mesh.contentHash);
      }
    }
    for (const [key, handle] of this.#meshHandles) {
      if (!incomingMeshes.has(key)) {
        ops.push({ op: "destroy", handle });
        this.#meshHandles.delete(key);
        this.#meshHashes.delete(key);
      }
    }

    const incoming = new Set<number>();
    for (const node of state.projection) {
      incoming.add(node.id);
      const known = this.#known.get(node.id);
      const beaconState = state.extractionBeacon?.id === node.id
        ? state.extractionBeacon.state
        : null;
      if (known === undefined) {
        ops.push({
          op: "create",
          handle: entityHandle(node.id),
          parent: null,
          node: projectedNode(node, beaconState),
        });
      } else if (!sameProjectionNode(known.node, node) || known.beaconState !== beaconState) {
        const next = projectedNode(node, beaconState);
        ops.push({
          op: "update",
          handle: entityHandle(node.id),
          transform: next.transform,
          material: next.material,
          visible: next.visible,
          metadata: next.metadata,
        });
      }
      this.#known.set(node.id, { node, beaconState });
    }

    for (const id of [...this.#known.keys()]) {
      if (!incoming.has(id)) {
        ops.push({ op: "destroy", handle: entityHandle(id) });
        this.#known.delete(id);
      }
    }
    return { ops };
  }

  get trackedEntityCount(): number {
    return this.#known.size;
  }

  get trackedMeshCount(): number {
    return this.#meshHandles.size;
  }
}

function meshPayload(mesh: RuntimeVoxelMeshChunk): MeshPayloadDescriptor {
  return {
    layout: {
      vertexCount: mesh.positions.length / 3,
      indexCount: mesh.indices.length,
      indexWidth: "u32",
      attributes: [
        { name: "position", components: 3, kind: "f32" },
        { name: "normal", components: 3, kind: "f32" },
      ],
    },
    groups: mesh.groups,
    bounds: { min: mesh.boundsMin, max: mesh.boundsMax },
    source: {
      kind: "inline",
      positions: mesh.positions,
      normals: mesh.normals,
      indices: mesh.indices,
    },
    provenance: "voxelChunk",
  };
}

export function entityHandle(id: number): RenderHandle {
  if (!Number.isSafeInteger(id) || id < 0 || id > Number.MAX_SAFE_INTEGER - ENTITY_HANDLE_OFFSET) {
    throw new RangeError("projection entity id is outside the browser-safe integer range");
  }
  return renderHandle(ENTITY_HANDLE_OFFSET + id);
}

function projectedNode(
  node: RuntimeProjectionNode,
  beaconState: "standby" | "active" | null,
): RenderNode {
  const door = node.asset.includes("door");
  const beacon = node.asset.includes("extraction-beacon");
  const probe = node.asset.includes("spatial-probe");
  const wall = node.asset.includes("voxel-wall");
  const player = node.asset.includes("player-marker");
  const scale: readonly [number, number, number] = door
    ? [2.4, 3.4, 0.55]
    : beacon
      ? [0.8, 2.4, 0.8]
    : probe
      ? [0.5, 0.5, 0.5]
      : wall
        ? [1, 1, 1]
        : player
          ? [0.7, 1.4, 0.7]
          : [1.1, 1.8, 1.1];
  const authored = node.translation ?? [0, 0, 0];
  const translation: readonly [number, number, number] = [
    authored[0],
    authored[1] + (probe || wall ? 0 : scale[1] / 2),
    authored[2],
  ];
  const color: Material = door
    ? { color: [0.9, 0.55, 0.16, 1], wireframe: false }
    : beacon
      ? beaconState === "active"
        ? { color: [0.22, 0.95, 0.72, 1], wireframe: false }
        : { color: [0.85, 0.54, 0.18, 1], wireframe: false }
    : probe
      ? { color: [0.26, 0.85, 0.68, 1], wireframe: false }
      : wall
        ? { color: [0.22, 0.38, 0.43, 1], wireframe: false }
        : player
          ? { color: [0.24, 0.74, 0.91, 1], wireframe: false }
          : { color: [0.82, 0.18, 0.14, 1], wireframe: false };
  return primitiveNode(
    node.name,
    node.id,
    probe || player ? "sphere" : "cube",
    translation,
    scale,
    color,
    node.visible && !player,
  );
}

function primitiveNode(
  label: string,
  source: number | null,
  shape: Exclude<Geometry["shape"], "line">,
  translation: readonly [number, number, number],
  scale: readonly [number, number, number],
  material: Material,
  visible = true,
): RenderNode {
  return {
    geometry: { shape },
    material,
    transform: identityTransform(translation, scale),
    visible,
    layer: "scene",
    metadata: { source: source === null ? null : entityId(source), tags: [], label },
  };
}

function identityTransform(
  translation: readonly [number, number, number],
  scale: readonly [number, number, number],
): Transform {
  return { translation, rotation: [0, 0, 0, 1], scale };
}

function sameProjectionNode(left: RuntimeProjectionNode, right: RuntimeProjectionNode): boolean {
  return (
    left.name === right.name &&
    left.asset === right.asset &&
    left.visible === right.visible &&
    JSON.stringify(left.translation) === JSON.stringify(right.translation)
  );
}
