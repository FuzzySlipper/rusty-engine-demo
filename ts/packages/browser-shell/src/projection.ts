import {
  renderHandle,
  type Geometry,
  type Material,
  type MeshPayloadDescriptor,
  type RenderDiff,
  type RenderFrameDiff,
  type RenderHandle,
  type RenderNode,
  type Transform,
} from "@rusty-engine/render-contracts";

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
  readonly selectWeapon: readonly string[];
}

export interface RuntimePlayerState {
  readonly id: number;
  readonly position: readonly [number, number, number];
  readonly yawDegrees: number;
  readonly pitchDegrees: number;
  readonly moveStepSeconds: number;
  readonly lookDegreesPerUnit: number;
  readonly bindings: RuntimePlayerBindings;
  readonly currentHealth: number;
  readonly maxHealth: number;
  readonly armor: number;
  readonly maxArmor: number;
  readonly vitalityState: "alive" | "dead";
}

export interface RuntimeWeaponState {
  readonly item: string;
  readonly presentation: string;
  readonly damage: number;
  readonly ammunition: string;
  readonly ammunitionCost: number;
  readonly ammoRemaining: number;
  readonly ammoCapacity: number;
  readonly readyAtTick: number;
}

export interface RuntimeInventoryStack {
  readonly item: string;
  readonly quantity: number;
}

export interface RuntimeInventoryWeapon {
  readonly slot: number;
  readonly item: string;
  readonly owned: boolean;
  readonly selected: boolean;
  readonly ammunition: string;
  readonly ammunitionQuantity: number;
}

export interface RuntimeInventoryState {
  readonly owner: number;
  readonly capacitySlots: number;
  readonly stacks: readonly RuntimeInventoryStack[];
  readonly equippedWeapon: string | null;
  readonly weapons: readonly RuntimeInventoryWeapon[];
}

export interface RuntimePickupState {
  readonly id: number;
  readonly item: string;
  readonly quantity: number;
  readonly state: "available" | "collected";
  readonly collectedBy: number | null;
  readonly collectedAtTick: number | null;
  readonly collectionCause: "overlap" | "interaction" | null;
}

export interface RuntimeHazardState {
  readonly id: number;
  readonly damage: number;
  readonly cooldownTicks: number;
  readonly readyAtTick: number;
}

export interface RuntimeRestartState {
  readonly authoredBaselineAvailable: boolean;
  readonly checkpointAvailable: boolean;
}

export interface RuntimeInputSessionState {
  readonly connectionGeneration: number;
  readonly connected: boolean;
  readonly paused: boolean;
  readonly acknowledgedSequence: number;
  readonly consumedSequence: number;
  readonly queuedEdgeCommands: number;
}

export interface RuntimeExtractionBeaconState {
  readonly id: number;
  readonly state: "standby" | "active";
  readonly activationRadius: number;
  readonly activatedBy: number | null;
  readonly activatedAtTick: number | null;
}

export interface RuntimeDoorAccessState {
  readonly id: number;
  readonly state: "closed" | "open";
  readonly requiredKey: string;
  readonly keyPolicy: "retain" | "consume";
  readonly activationRadius: number;
  readonly deniedPresentation: string;
}

export interface RuntimeSecretRegionState {
  readonly id: number;
  readonly state: "undiscovered" | "discovered";
  readonly presentation: string;
}

export interface RuntimeLevelExitState {
  readonly id: number;
  readonly state: "available" | "completed";
  readonly activationRadius: number;
  readonly presentation: string;
  readonly completedBy: number | null;
  readonly completedAtTick: number | null;
}

export interface RuntimeInteractionState {
  readonly target: number;
  readonly prompt: string;
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
  readonly posture:
    | "idle"
    | "moving"
    | "defeated"
    | "open"
    | "closed"
    | "standby"
    | "active";
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
      readonly weapon: string;
      readonly presentation: string;
      readonly attackMode: "hitscan" | "spread" | "automatic";
      readonly rayCount: number;
      readonly origin: readonly [number, number, number];
      readonly direction: readonly [number, number, number];
    }
  | {
      readonly kind: "dryFire";
      readonly attacker: number;
      readonly weapon: string;
      readonly presentation: string;
    }
  | {
      readonly kind: "damage";
      readonly attacker: number;
      readonly target: number;
      readonly amount: number;
      readonly remaining: number;
    }
  | {
      readonly kind: "defeat";
      readonly attacker: number | null;
      readonly entity: number;
    }
  | {
      readonly kind: "doorChanged";
      readonly entity: number;
      readonly state: "open" | "closed";
    }
  | {
      readonly kind: "extractionBeaconActivated";
      readonly entity: number;
      readonly actor: number;
    }
  | {
      readonly kind: "pickupCollected";
      readonly entity: number;
      readonly actor: number;
      readonly item: string;
      readonly quantity: number;
    }
  | {
      readonly kind: "doorAccessGranted";
      readonly entity: number;
      readonly actor: number;
      readonly requiredKey: string;
      readonly keyConsumed: boolean;
    }
  | {
      readonly kind: "doorAccessDenied";
      readonly entity: number;
      readonly requiredKey: string;
      readonly presentation: string;
    }
  | {
      readonly kind: "secretDiscovered";
      readonly entity: number;
      readonly actor: number;
      readonly presentation: string;
    }
  | {
      readonly kind: "levelCompleted";
      readonly entity: number;
      readonly actor: number;
      readonly presentation: string;
    };

export interface RuntimePresentationState {
  readonly animationStates: readonly RuntimeAnimationState[];
  readonly cues: readonly RuntimeFeedbackCue[];
}

export interface RuntimeBrowserState {
  readonly hostSessionId: string;
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
  readonly input: RuntimeInputSessionState;
  readonly player: RuntimePlayerState;
  readonly weapon: RuntimeWeaponState;
  readonly inventory: RuntimeInventoryState | null;
  readonly pickups: readonly RuntimePickupState[];
  readonly hazards: readonly RuntimeHazardState[];
  readonly restart: RuntimeRestartState;
  readonly extractionBeacon: RuntimeExtractionBeaconState | null;
  readonly doorAccess: readonly RuntimeDoorAccessState[];
  readonly secretRegions: readonly RuntimeSecretRegionState[];
  readonly levelExits: readonly RuntimeLevelExitState[];
  readonly levelComplete: boolean;
  readonly interaction: RuntimeInteractionState | null;
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

export interface RuntimeProjectionPlan extends RenderFrameDiff {
  /** Advance the adapter baseline only after the renderer accepts this frame. */
  readonly commit: () => void;
}

/** Stateful adapter from whole Rust projection readouts to retained renderer diffs. */
export class RuntimeProjectionAdapter {
  readonly #known = new Map<
    number,
    {
      readonly node: RuntimeProjectionNode;
      readonly beaconState: "standby" | "active" | null;
    }
  >();
  readonly #meshHashes = new Map<string, string>();
  readonly #meshHandles = new Map<string, RenderHandle>();
  #nextMeshHandle = FIRST_VOXEL_MESH_HANDLE;
  #revision = 0;

  apply(state: RuntimeBrowserState): RuntimeProjectionPlan {
    const baseRevision = this.#revision;
    const nextKnown = new Map(this.#known);
    const nextMeshHashes = new Map(this.#meshHashes);
    const nextMeshHandles = new Map(this.#meshHandles);
    let nextMeshHandle = this.#nextMeshHandle;
    const ops: RenderDiff[] = [];
    const incomingMeshes = new Set<string>();
    for (const mesh of state.voxelMeshes) {
      const key = mesh.chunk.join(",");
      incomingMeshes.add(key);
      let handle = nextMeshHandles.get(key);
      if (handle === undefined) {
        handle = renderHandle(nextMeshHandle);
        nextMeshHandle += 1;
        nextMeshHandles.set(key, handle);
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
      if (nextMeshHashes.get(key) !== mesh.contentHash) {
        ops.push({
          op: "replaceMeshPayload",
          handle,
          payload: meshPayload(mesh),
        });
        nextMeshHashes.set(key, mesh.contentHash);
      }
    }
    for (const [key, handle] of nextMeshHandles) {
      if (!incomingMeshes.has(key)) {
        ops.push({ op: "destroy", handle });
        nextMeshHandles.delete(key);
        nextMeshHashes.delete(key);
      }
    }

    const incoming = new Set<number>();
    for (const node of state.projection) {
      incoming.add(node.id);
      const known = nextKnown.get(node.id);
      const beaconState =
        state.extractionBeacon?.id === node.id
          ? state.extractionBeacon.state
          : null;
      if (known === undefined) {
        ops.push({
          op: "create",
          handle: entityHandle(node.id),
          parent: null,
          node: projectedNode(node, beaconState),
        });
      } else if (
        !sameProjectionNode(known.node, node) ||
        known.beaconState !== beaconState
      ) {
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
      nextKnown.set(node.id, { node, beaconState });
    }

    for (const id of [...nextKnown.keys()]) {
      if (!incoming.has(id)) {
        ops.push({ op: "destroy", handle: entityHandle(id) });
        nextKnown.delete(id);
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
          throw new Error("cannot commit a stale runtime projection plan");
        }
        replaceMap(this.#known, nextKnown);
        replaceMap(this.#meshHashes, nextMeshHashes);
        replaceMap(this.#meshHandles, nextMeshHandles);
        this.#nextMeshHandle = nextMeshHandle;
        this.#revision += 1;
        committed = true;
      },
    };
  }

  get trackedEntityCount(): number {
    return this.#known.size;
  }

  get trackedMeshCount(): number {
    return this.#meshHandles.size;
  }
}

function replaceMap<K, V>(target: Map<K, V>, source: ReadonlyMap<K, V>): void {
  target.clear();
  for (const [key, value] of source) {
    target.set(key, value);
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
  if (
    !Number.isSafeInteger(id) ||
    id < 0 ||
    id > Number.MAX_SAFE_INTEGER - ENTITY_HANDLE_OFFSET
  ) {
    throw new RangeError(
      "projection entity id is outside the browser-safe integer range",
    );
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
  const pickup = node.asset.includes("pickup-");
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
            : pickup
              ? [0.55, 0.55, 0.55]
              : [1.1, 1.8, 1.1];
  const authored = node.translation ?? [0, 0, 0];
  const translation: readonly [number, number, number] = [
    authored[0],
    authored[1] + (probe || wall || pickup ? 0 : scale[1] / 2),
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
            : pickup
              ? pickupMaterial(node.asset)
              : { color: [0.82, 0.18, 0.14, 1], wireframe: false };
  return primitiveNode(
    node.name,
    node.id,
    probe || player || pickup ? "sphere" : "cube",
    translation,
    scale,
    color,
    node.visible && !player,
  );
}

function pickupMaterial(asset: string): Material {
  if (asset.includes("pickup-ammunition")) {
    return { color: [0.95, 0.76, 0.2, 1], wireframe: false };
  }
  if (asset.includes("pickup-health")) {
    return { color: [0.3, 0.95, 0.44, 1], wireframe: false };
  }
  if (asset.includes("pickup-armor")) {
    return { color: [0.3, 0.63, 0.98, 1], wireframe: false };
  }
  if (asset.includes("pickup-key")) {
    return { color: [0.92, 0.42, 0.95, 1], wireframe: false };
  }
  return { color: [0.92, 0.28, 0.2, 1], wireframe: false };
}

function primitiveNode(
  label: string,
  source: number | null,
  kind: Exclude<Geometry["kind"], "line" | "group">,
  translation: readonly [number, number, number],
  scale: readonly [number, number, number],
  material: Material,
  visible = true,
): RenderNode {
  return {
    geometry: { kind },
    material,
    transform: identityTransform(translation, scale),
    visible,
    layer: "scene",
    metadata: {
      sourceEntity: source,
      sourceSceneNode: null,
      tags: [],
      label,
    },
  };
}

function identityTransform(
  translation: readonly [number, number, number],
  scale: readonly [number, number, number],
): Transform {
  return { translation, rotation: [0, 0, 0, 1], scale };
}

function sameProjectionNode(
  left: RuntimeProjectionNode,
  right: RuntimeProjectionNode,
): boolean {
  return (
    left.name === right.name &&
    left.asset === right.asset &&
    left.visible === right.visible &&
    JSON.stringify(left.translation) === JSON.stringify(right.translation)
  );
}
