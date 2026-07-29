import {
  renderHandle,
  type MeshPayloadDescriptor,
  type RenderFrameDiff,
  type RenderMaterialDescriptor,
  type StaticMeshAsset,
  type StaticMeshInstanceDescriptor,
  type Transform,
} from "@rusty-engine/render-contracts";
import type {
  RendererSurface,
  RendererSurfaceStatistic,
  RendererSurfaceStatisticsSample,
  RendererSurfaceSubmissionSample,
} from "@rusty-engine/renderer-host";

const PROOF_ROOT_HANDLE = 8_000_000;
const PROOF_INSTANCE_COUNT = 32;
const PROOF_ASSET_COUNT = 4;
const IDENTITY_ROTATION = [0, 0, 0, 1] as const;

const STATISTIC_KEYS = [
  "drawCallCount",
  "renderHandleCount",
  "geometryResourceCount",
  "materialResourceCount",
  "textureResourceCount",
  "animatedInstanceCount",
  "triangleCount",
] as const;

type StatisticKey = (typeof STATISTIC_KEYS)[number];

export interface LoadingBayRendererStatisticsProof {
  readonly kind: "loading_bay_renderer_statistics_proof.v1";
  readonly schemaVersion: 1;
  readonly contentRichAssetCount: number;
  readonly contentRichInstanceCount: number;
  readonly placeholder: RendererSurfaceSubmissionSample;
  readonly contentRich: RendererSurfaceSubmissionSample;
  readonly restored: RendererSurfaceSubmissionSample;
}

/**
 * Capture one deterministic richer renderer load without adding a render loop.
 *
 * The ordinary Loading Bay surface remains the only surface and scheduler. The
 * probe submits three explicit frames around one renderer-neutral retained
 * mutation, then removes the complete temporary handle tree before returning.
 */
export function captureLoadingBayRendererStatisticsProof(
  surface: Pick<RendererSurface, "applyFrame" | "renderOnce">,
  now: () => number = () => globalThis.performance?.now() ?? 0,
): LoadingBayRendererStatisticsProof {
  const placeholder = surface.renderOnce(now());
  let contentRichApplied = false;
  try {
    surface.applyFrame(contentRichFrame());
    contentRichApplied = true;
    const contentRich = surface.renderOnce(now());
    assertContentRichDelta(placeholder.statistics, contentRich.statistics);

    surface.applyFrame(cleanupFrame());
    contentRichApplied = false;
    const restored = surface.renderOnce(now());
    assertStatisticsEqual(placeholder.statistics, restored.statistics);

    return Object.freeze({
      kind: "loading_bay_renderer_statistics_proof.v1",
      schemaVersion: 1,
      contentRichAssetCount: PROOF_ASSET_COUNT,
      contentRichInstanceCount: PROOF_INSTANCE_COUNT,
      placeholder,
      contentRich,
      restored,
    });
  } finally {
    if (contentRichApplied) {
      surface.applyFrame(cleanupFrame());
      surface.renderOnce(now());
    }
  }
}

export function contentRichFrame(): RenderFrameDiff {
  const operations: RenderFrameDiff["ops"][number][] = [];
  for (let assetIndex = 0; assetIndex < PROOF_ASSET_COUNT; assetIndex += 1) {
    const materialId = `material/loading-bay-statistics-${String(assetIndex)}`;
    const assetId = `mesh/loading-bay-statistics-${String(assetIndex)}`;
    operations.push(material(materialId, proofColor(assetIndex)));
    operations.push({
      op: "defineStaticMesh",
      asset: panelAsset(assetId, materialId),
    });
  }
  operations.push({
    op: "create",
    handle: renderHandle(PROOF_ROOT_HANDLE),
    parent: null,
    node: {
      geometry: { kind: "group" },
      material: { color: [1, 1, 1, 1], wireframe: false },
      transform: transform([0, 0, 0], [1, 1, 1]),
      visible: true,
      layer: "viewmodel",
      metadata: proofMetadata("content-rich-root"),
    },
  });
  for (let instanceIndex = 0; instanceIndex < PROOF_INSTANCE_COUNT; instanceIndex += 1) {
    const assetIndex = instanceIndex % PROOF_ASSET_COUNT;
    const column = instanceIndex % 8;
    const row = Math.floor(instanceIndex / 8);
    operations.push(instance(
      PROOF_ROOT_HANDLE + 1 + instanceIndex,
      `mesh/loading-bay-statistics-${String(assetIndex)}`,
      `content-rich-panel-${String(instanceIndex)}`,
      [-1.225 + column * 0.35, 0.525 - row * 0.35, -3],
    ));
  }
  return { schemaVersion: 1, ops: operations };
}

export function cleanupFrame(): RenderFrameDiff {
  return {
    schemaVersion: 1,
    ops: [{ op: "destroy", handle: renderHandle(PROOF_ROOT_HANDLE) }],
  };
}

function assertContentRichDelta(
  placeholder: RendererSurfaceStatisticsSample,
  contentRich: RendererSurfaceStatisticsSample,
): void {
  assertDelta(placeholder, contentRich, "drawCallCount", PROOF_INSTANCE_COUNT);
  assertDelta(placeholder, contentRich, "renderHandleCount", PROOF_INSTANCE_COUNT + 1);
  assertDelta(placeholder, contentRich, "geometryResourceCount", PROOF_ASSET_COUNT);
  assertDelta(placeholder, contentRich, "materialResourceCount", PROOF_ASSET_COUNT);
  assertDelta(placeholder, contentRich, "textureResourceCount", 0);
  assertDelta(placeholder, contentRich, "animatedInstanceCount", 0);
  assertDelta(placeholder, contentRich, "triangleCount", PROOF_INSTANCE_COUNT * 2);
}

function assertDelta(
  placeholder: RendererSurfaceStatisticsSample,
  contentRich: RendererSurfaceStatisticsSample,
  key: StatisticKey,
  expectedDelta: number,
): void {
  const placeholderValue = availableValue(placeholder[key], `placeholder.${key}`);
  const contentRichValue = availableValue(contentRich[key], `contentRich.${key}`);
  if (contentRichValue - placeholderValue !== expectedDelta) {
    throw new Error(
      `${key} delta was ${String(contentRichValue - placeholderValue)}; expected ${String(expectedDelta)}`,
    );
  }
}

function assertStatisticsEqual(
  placeholder: RendererSurfaceStatisticsSample,
  restored: RendererSurfaceStatisticsSample,
): void {
  for (const key of STATISTIC_KEYS) {
    if (JSON.stringify(restored[key]) !== JSON.stringify(placeholder[key])) {
      throw new Error(`${key} did not return to the placeholder renderer statistic`);
    }
  }
}

function availableValue(statistic: RendererSurfaceStatistic, label: string): number {
  if (statistic.status !== "available") {
    throw new Error(`${label} is ${statistic.status}; exact Three proof requires an available value`);
  }
  return statistic.value;
}

function material(
  id: string,
  color: readonly [number, number, number, number],
): { readonly op: "defineMaterial"; readonly material: RenderMaterialDescriptor } {
  return {
    op: "defineMaterial",
    material: {
      schemaVersion: 2,
      id,
      color,
      texture: null,
      roughness: 0.75,
      textureTint: [1, 1, 1, 1],
      emissionColor: [0, 0, 0],
      emissionIntensity: 0,
      uvStrategy: "flat",
    },
  };
}

function panelAsset(asset: string, materialId: string): StaticMeshAsset {
  return {
    asset,
    payload: quadPayload(),
    materialSlots: [{ slot: 0, material: materialId }],
    collision: { kind: "visualOnly" },
  };
}

function quadPayload(): MeshPayloadDescriptor {
  return {
    layout: {
      vertexCount: 4,
      indexCount: 6,
      indexWidth: "u32",
      attributes: [
        { name: "position", components: 3, kind: "f32" },
        { name: "normal", components: 3, kind: "f32" },
      ],
    },
    groups: [{ materialSlot: 0, start: 0, count: 6 }],
    bounds: { min: [-0.15, -0.15, 0], max: [0.15, 0.15, 0] },
    source: {
      kind: "inline",
      positions: [-0.15, -0.15, 0, 0.15, -0.15, 0, 0.15, 0.15, 0, -0.15, 0.15, 0],
      normals: [0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1],
      indices: [0, 1, 2, 0, 2, 3],
    },
    provenance: "generated",
  };
}

function instance(
  handle: number,
  asset: string,
  label: string,
  translation: readonly [number, number, number],
): {
  readonly op: "createStaticMeshInstance";
  readonly handle: ReturnType<typeof renderHandle>;
  readonly parent: ReturnType<typeof renderHandle>;
  readonly instance: StaticMeshInstanceDescriptor;
} {
  return {
    op: "createStaticMeshInstance",
    handle: renderHandle(handle),
    parent: renderHandle(PROOF_ROOT_HANDLE),
    instance: {
      asset,
      transform: transform(translation, [1, 1, 1]),
      visible: true,
      materialOverrides: [],
      metadata: proofMetadata(label),
    },
  };
}

function transform(
  translation: readonly [number, number, number],
  scale: readonly [number, number, number],
): Transform {
  return { translation, rotation: IDENTITY_ROTATION, scale };
}

function proofMetadata(label: string) {
  return {
    sourceEntity: null,
    sourceSceneNode: null,
    tags: ["renderer-statistics-proof"],
    label,
  } as const;
}

function proofColor(index: number): readonly [number, number, number, number] {
  return [
    0.3 + index * 0.15,
    0.75 - index * 0.1,
    0.45 + index * 0.08,
    1,
  ];
}
