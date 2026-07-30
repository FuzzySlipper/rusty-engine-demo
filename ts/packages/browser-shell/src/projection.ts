import {
  renderHandle,
  type Geometry,
  type LightDescriptor,
  type Material,
  type MaterialInstanceParameters,
  type MeshPayloadDescriptor,
  type RenderDiff,
  type RenderFrameDiff,
  type RenderHandle,
  type RenderMaterialDescriptor,
  type RenderNode,
  type StaticMeshAsset,
  type StaticMeshInstanceDescriptor,
  type Transform,
} from "@rusty-engine/render-contracts";

export type RuntimeVisualState =
  | "default"
  | "open"
  | "closed"
  | "active"
  | "inactive"
  | "standby"
  | "available"
  | "dormant"
  | "collected"
  | "cooling"
  | "completed";

export interface RuntimeProjectionNode {
  readonly id: number;
  readonly name: string;
  readonly asset: string;
  readonly translation: readonly [number, number, number] | null;
  readonly visible: boolean;
  readonly visualState: RuntimeVisualState;
}

export interface RuntimeEnemyState {
  readonly id: number;
  readonly name: string;
  readonly state: "alive" | "defeated";
  readonly position: readonly [number, number, number];
  readonly currentHealth: number;
  readonly maxHealth: number;
  readonly combatPosture:
    | "sleeping"
    | "alert"
    | "pursuing"
    | "attacking"
    | "dead"
    | null;
  readonly attackKind: "melee" | "rangedHitscan" | null;
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
  readonly state: "dormant" | "available" | "collected";
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

export type RuntimeAuthoredLightDefinition =
  | {
      readonly kind: "ambient";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "directional";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "point";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly shadows: boolean;
    }
  | {
      readonly kind: "spot";
      readonly color: readonly [number, number, number];
      readonly intensity: number;
      readonly enabled: boolean;
      readonly range: number | null;
      readonly decay: number;
      readonly outerAngleRadians: number;
      readonly penumbra: number;
      readonly shadows: boolean;
    };

export interface RuntimeAuthoredLight {
  readonly id: number;
  readonly translation: readonly [number, number, number] | null;
  readonly rotation: readonly [number, number, number, number];
  readonly light: RuntimeAuthoredLightDefinition;
}

export interface RuntimeAnimationState {
  readonly entity: number;
  readonly posture:
    | "idle"
    | "moving"
    | "alert"
    | "attacking"
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
      readonly kind: "attackHit";
      readonly attacker: number;
      readonly target: number;
    }
  | {
      readonly kind: "attackMissed";
      readonly attacker: number;
      readonly reason: "noTarget" | "worldBlocked";
    }
  | {
      readonly kind: "damage";
      readonly attacker: number;
      readonly target: number;
      readonly amount: number;
      readonly remaining: number;
    }
  | {
      readonly kind: "enemyAlert";
      readonly entity: number;
      readonly target: number;
      readonly cause: "sight" | "hearing";
    }
  | {
      readonly kind: "enemyAttack";
      readonly attacker: number;
      readonly target: number;
      readonly attackKind: "melee" | "rangedHitscan";
      readonly presentation: string;
      readonly origin: readonly [number, number, number];
      readonly targetPosition: readonly [number, number, number];
    }
  | {
      readonly kind: "enemyAttackMissed";
      readonly attacker: number;
      readonly target: number;
      readonly reason: "worldBlocked" | "targetOutOfRange" | "targetDead";
    }
  | {
      readonly kind: "defeat";
      readonly attacker: number | null;
      readonly entity: number;
    }
  | {
      readonly kind: "enemyDropMaterialized";
      readonly enemy: number;
      readonly pickup: number;
      readonly item: string;
      readonly quantity: number;
      readonly position: readonly [number, number, number];
    }
  | {
      readonly kind: "encounterActivated";
      readonly entity: number;
      readonly player: number;
    }
  | {
      readonly kind: "doorChanged";
      readonly entity: number;
      readonly state: "open" | "closed";
    }
  | {
      readonly kind: "switchActivated";
      readonly entity: number;
      readonly actor: number;
    }
  | {
      readonly kind: "checkpoint";
      readonly player: number;
      readonly action: "saved" | "restored";
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

export type RuntimeSaveSlotId = "checkpoint" | "slot1" | "slot2" | "slot3";

export type RuntimeSaveSlotCompatibility =
  | "empty"
  | "available"
  | "corrupt"
  | "incompatible";

export interface RuntimeSaveGameMetadata {
  readonly revision: number;
  readonly savedAtUnixMilliseconds: number;
  readonly displayName: string;
  readonly tick: number;
  readonly snapshotSchemaVersion: number;
  readonly playerState: "alive" | "dead" | "unavailable";
  readonly levelComplete: boolean;
}

export interface RuntimeSaveProjectIdentity {
  readonly projectId: string;
  readonly entryScene: string;
  readonly playerEntity: number;
  readonly projectSchemaVersion: number;
  readonly contentRevision: string;
}

export interface RuntimeSaveSlotSummary {
  readonly slot: RuntimeSaveSlotId;
  readonly compatibility: RuntimeSaveSlotCompatibility;
  readonly storageRevision: string | null;
  readonly metadata: RuntimeSaveGameMetadata | null;
  readonly project: RuntimeSaveProjectIdentity | null;
  readonly diagnostic: string | null;
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
  readonly encounterState: "dormant" | "active" | "cleared";
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
  readonly saveSlots: readonly RuntimeSaveSlotSummary[];
  readonly extractionBeacon: RuntimeExtractionBeaconState | null;
  readonly doorAccess: readonly RuntimeDoorAccessState[];
  readonly secretRegions: readonly RuntimeSecretRegionState[];
  readonly levelExits: readonly RuntimeLevelExitState[];
  readonly levelComplete: boolean;
  readonly interaction: RuntimeInteractionState | null;
  readonly voxelEnvironmentRole: "visible" | "gameplayProxy" | "none";
  readonly voxelMeshes: readonly RuntimeVoxelMeshChunk[];
  readonly voxelObjectFrame: RenderFrameDiff;
  readonly lights: readonly RuntimeAuthoredLight[];
  readonly renderMaterials: readonly RenderMaterialDescriptor[];
  readonly staticMeshes: readonly StaticMeshAsset[];
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
const LIGHT_HANDLE_OFFSET = 400_000;
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
      readonly kind: "primitive" | "staticMesh";
      readonly renderedAsset: string | null;
    }
  >();
  readonly #definedMaterials = new Map<string, string>();
  readonly #definedStaticMeshes = new Map<string, string>();
  #acceptedRenderMaterials: readonly RenderMaterialDescriptor[] | null = null;
  #acceptedStaticMeshes: readonly StaticMeshAsset[] | null = null;
  #acceptedVoxelObjectFrame: RenderFrameDiff | null = null;
  #voxelObjectFrameFingerprint: string | null = null;
  readonly #meshHashes = new Map<string, string>();
  readonly #meshHandles = new Map<string, RenderHandle>();
  readonly #knownLights = new Map<number, LightDescriptor>();
  #nextMeshHandle = FIRST_VOXEL_MESH_HANDLE;
  #revision = 0;

  apply(state: RuntimeBrowserState): RuntimeProjectionPlan {
    const baseRevision = this.#revision;
    const nextKnown = new Map(this.#known);
    const nextMeshHashes = new Map(this.#meshHashes);
    const nextMeshHandles = new Map(this.#meshHandles);
    const nextKnownLights = new Map(this.#knownLights);
    const nextDefinedMaterials = new Map(this.#definedMaterials);
    const nextDefinedStaticMeshes = new Map(this.#definedStaticMeshes);
    let nextAcceptedRenderMaterials = this.#acceptedRenderMaterials;
    let nextAcceptedStaticMeshes = this.#acceptedStaticMeshes;
    let nextAcceptedVoxelObjectFrame = this.#acceptedVoxelObjectFrame;
    let nextVoxelObjectFrameFingerprint = this.#voxelObjectFrameFingerprint;
    let nextMeshHandle = this.#nextMeshHandle;
    const ops: RenderDiff[] = [];
    const staticMeshes = new Map(
      state.staticMeshes.map((asset) => [asset.asset, asset] as const),
    );
    const changedStaticMeshes =
      state.staticMeshes === this.#acceptedStaticMeshes
        ? new Set<string>()
        : new Set(
            state.staticMeshes
              .filter(
                (asset) =>
                  nextDefinedStaticMeshes.get(asset.asset) !==
                  JSON.stringify(asset),
              )
              .map((asset) => asset.asset),
          );
    for (const [id, known] of nextKnown) {
      if (
        known.kind === "staticMesh" &&
        known.renderedAsset !== null &&
        changedStaticMeshes.has(known.renderedAsset)
      ) {
        ops.push({ op: "destroy", handle: entityHandle(id) });
        nextKnown.delete(id);
      }
    }
    if (state.renderMaterials !== this.#acceptedRenderMaterials) {
      for (const material of state.renderMaterials) {
        const fingerprint = JSON.stringify(material);
        if (nextDefinedMaterials.get(material.id) !== fingerprint) {
          ops.push({ op: "defineMaterial", material });
          nextDefinedMaterials.set(material.id, fingerprint);
        }
      }
      nextAcceptedRenderMaterials = state.renderMaterials;
    }
    if (state.staticMeshes !== this.#acceptedStaticMeshes) {
      for (const asset of state.staticMeshes) {
        const fingerprint = JSON.stringify(asset);
        if (nextDefinedStaticMeshes.get(asset.asset) !== fingerprint) {
          ops.push({ op: "defineStaticMesh", asset });
          nextDefinedStaticMeshes.set(asset.asset, fingerprint);
        }
      }
      nextAcceptedStaticMeshes = state.staticMeshes;
    }
    if (state.voxelObjectFrame !== this.#acceptedVoxelObjectFrame) {
      const fingerprint = JSON.stringify(state.voxelObjectFrame);
      if (
        this.#voxelObjectFrameFingerprint !== null &&
        fingerprint !== this.#voxelObjectFrameFingerprint
      ) {
        throw new Error(
          "runtime voxel-object structure changed without a renderer session replacement",
        );
      }
      if (this.#voxelObjectFrameFingerprint === null) {
        ops.push(...state.voxelObjectFrame.ops);
      }
      nextAcceptedVoxelObjectFrame = state.voxelObjectFrame;
      nextVoxelObjectFrameFingerprint = fingerprint;
    }
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

    const incomingLights = new Set<number>();
    for (const authored of state.lights) {
      incomingLights.add(authored.id);
      const light = lightDescriptor(authored);
      const known = nextKnownLights.get(authored.id);
      if (known === undefined) {
        ops.push({
          op: "createLight",
          handle: lightHandle(authored.id),
          parent: null,
          light,
        });
      } else if (!sameLight(known, light)) {
        ops.push({
          op: "updateLight",
          handle: lightHandle(authored.id),
          light,
        });
      }
      nextKnownLights.set(authored.id, light);
    }
    for (const id of [...nextKnownLights.keys()]) {
      if (!incomingLights.has(id)) {
        ops.push({ op: "destroy", handle: lightHandle(id) });
        nextKnownLights.delete(id);
      }
    }

    const incoming = new Set<number>();
    for (const node of state.projection) {
      incoming.add(node.id);
      const known = nextKnown.get(node.id);
      const staticMesh = staticMeshes.get(node.asset);
      const kind = staticMesh === undefined ? "primitive" : "staticMesh";
      if (kind === "primitive" && !PRIMITIVE_FALLBACK_ASSETS.has(node.asset)) {
        throw new Error(
          `runtime projection is missing canonical static mesh ${node.asset}`,
        );
      }
      if (known === undefined) {
        ops.push(...createProjectedNode(node, staticMesh));
      } else if (
        known.kind !== kind ||
        known.renderedAsset !== (staticMesh?.asset ?? null)
      ) {
        ops.push({ op: "destroy", handle: entityHandle(node.id) });
        ops.push(...createProjectedNode(node, staticMesh));
      } else if (!sameProjectionNode(known.node, node)) {
        const nextTransform =
          staticMesh === undefined
            ? primitiveFallbackNode(node).transform
            : staticMeshInstance(node).transform;
        const nextMetadata =
          staticMesh === undefined
            ? primitiveFallbackNode(node).metadata
            : staticMeshInstance(node).metadata;
        ops.push({
          op: "update",
          handle: entityHandle(node.id),
          transform: nextTransform,
          material:
            staticMesh === undefined
              ? primitiveFallbackNode(node).material
              : null,
          visible: node.visible && node.asset !== "mesh/player-marker",
          metadata: nextMetadata,
        });
        if (
          staticMesh !== undefined &&
          known.node.visualState !== node.visualState
        ) {
          ops.push(...materialStateOperations(node, staticMesh));
        }
      }
      nextKnown.set(node.id, {
        node,
        kind,
        renderedAsset: staticMesh?.asset ?? null,
      });
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
        replaceMap(this.#knownLights, nextKnownLights);
        replaceMap(this.#definedMaterials, nextDefinedMaterials);
        replaceMap(this.#definedStaticMeshes, nextDefinedStaticMeshes);
        this.#acceptedRenderMaterials = nextAcceptedRenderMaterials;
        this.#acceptedStaticMeshes = nextAcceptedStaticMeshes;
        this.#acceptedVoxelObjectFrame = nextAcceptedVoxelObjectFrame;
        this.#voxelObjectFrameFingerprint =
          nextVoxelObjectFrameFingerprint;
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

  get trackedLightCount(): number {
    return this.#knownLights.size;
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

function lightHandle(id: number): RenderHandle {
  if (
    !Number.isSafeInteger(id) ||
    id < 0 ||
    id >= FIRST_VOXEL_MESH_HANDLE - LIGHT_HANDLE_OFFSET
  ) {
    throw new RangeError(
      "authored light id is outside the browser-safe handle range",
    );
  }
  return renderHandle(LIGHT_HANDLE_OFFSET + id);
}

function lightDescriptor(authored: RuntimeAuthoredLight): LightDescriptor {
  const shadowIntent = authored.light.shadows ? "requested" : "disabled";
  if (authored.light.kind === "ambient") {
    return {
      kind: "ambient",
      color: authored.light.color,
      intensity: authored.light.intensity,
      enabled: authored.light.enabled,
      shadowIntent,
    };
  }
  const direction = rotateForward(authored.rotation);
  if (authored.light.kind === "directional") {
    return {
      kind: "directional",
      color: authored.light.color,
      intensity: authored.light.intensity,
      enabled: authored.light.enabled,
      direction,
      shadowIntent,
    };
  }
  const position = authored.translation ?? [0, 0, 0];
  if (authored.light.kind === "point") {
    return {
      kind: "point",
      color: authored.light.color,
      intensity: authored.light.intensity,
      enabled: authored.light.enabled,
      position,
      range: authored.light.range,
      decay: authored.light.decay,
      shadowIntent,
    };
  }
  return {
    kind: "spot",
    color: authored.light.color,
    intensity: authored.light.intensity,
    enabled: authored.light.enabled,
    position,
    direction,
    range: authored.light.range,
    decay: authored.light.decay,
    outerAngleRadians: authored.light.outerAngleRadians,
    penumbra: authored.light.penumbra,
    shadowIntent,
  };
}

function rotateForward(
  rotation: readonly [number, number, number, number],
): readonly [number, number, number] {
  const [x, y, z, w] = rotation;
  return [
    -2 * (x * z + w * y),
    -2 * (y * z - w * x),
    -(1 - 2 * (x * x + y * y)),
  ];
}

function sameLight(left: LightDescriptor, right: LightDescriptor): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

const PRIMITIVE_FALLBACK_ASSETS = new Set([
  "mesh/player-marker",
  "mesh/bay-rusher",
  "mesh/arc-warden",
]);

function primitiveFallbackNode(node: RuntimeProjectionNode): RenderNode {
  const player = node.asset.includes("player-marker");
  const bayRusher = node.asset.includes("bay-rusher");
  const arcWarden = node.asset.includes("arc-warden");
  const scale: readonly [number, number, number] = player
    ? [0.7, 1.4, 0.7]
    : bayRusher
      ? [1.45, 1.25, 1.45]
      : [0.85, 2.35, 0.85];
  const authored = node.translation ?? [0, 0, 0];
  const translation: readonly [number, number, number] = [
    authored[0],
    authored[1] + scale[1] / 2,
    authored[2],
  ];
  const color: Material = player
    ? { color: [0.24, 0.74, 0.91, 1], wireframe: false }
    : bayRusher
      ? { color: [0.95, 0.34, 0.12, 1], wireframe: false }
      : { color: [0.55, 0.25, 0.95, 1], wireframe: false };
  return primitiveNode(
    node.name,
    node.id,
    player || arcWarden ? "sphere" : "cube",
    translation,
    scale,
    color,
    node.visible && !player,
  );
}

function createProjectedNode(
  node: RuntimeProjectionNode,
  staticMesh: StaticMeshAsset | undefined,
): RenderDiff[] {
  if (staticMesh === undefined) {
    return [
      {
        op: "create",
        handle: entityHandle(node.id),
        parent: null,
        node: primitiveFallbackNode(node),
      },
    ];
  }
  return [
    {
      op: "createStaticMeshInstance",
      handle: entityHandle(node.id),
      parent: null,
      instance: staticMeshInstance(node),
    },
    ...materialStateOperations(node, staticMesh),
  ];
}

function staticMeshInstance(
  node: RuntimeProjectionNode,
): StaticMeshInstanceDescriptor {
  return {
    asset: node.asset,
    transform: identityTransform(node.translation ?? [0, 0, 0], [1, 1, 1]),
    visible: node.visible,
    materialOverrides: [],
    metadata: {
      sourceEntity: node.id,
      sourceSceneNode: null,
      tags: ["loading-bay", "serialized-prop"],
      label: node.name,
    },
  };
}

function materialStateOperations(
  node: RuntimeProjectionNode,
  asset: StaticMeshAsset,
): RenderDiff[] {
  const parameters = visualStateParameters(node.visualState);
  return asset.materialSlots.map(({ slot }) => ({
    op: "setMaterialInstanceParameters",
    handle: entityHandle(node.id),
    slot,
    parameters,
  }));
}

function visualStateParameters(
  state: RuntimeVisualState,
): MaterialInstanceParameters | null {
  switch (state) {
    case "open":
    case "active":
    case "completed":
      return {
        textureTint: [0.62, 1, 0.82, 1],
        emissionColor: [0.12, 0.82, 0.52],
        emissionIntensity: 0.35,
      };
    case "closed":
    case "inactive":
    case "standby":
      return {
        textureTint: [1, 0.78, 0.48, 1],
        emissionColor: [0.75, 0.28, 0.05],
        emissionIntensity: 0.12,
      };
    case "cooling":
    case "dormant":
      return {
        textureTint: [0.58, 0.62, 0.68, 1],
        emissionColor: [0.12, 0.14, 0.18],
        emissionIntensity: 0.04,
      };
    case "available":
    case "collected":
    case "default":
      return null;
  }
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
    left.visualState === right.visualState &&
    JSON.stringify(left.translation) === JSON.stringify(right.translation)
  );
}
