import type { EntityDefinition, ProjectContent, VoxelAddress } from "./schema.js";

export const MOTION_LAB_FIRST_ENTITY_ID = 1_000;
export const MOTION_LAB_BODY_COUNT = 256;

/**
 * Compose a lane-per-body collision workload with ordinary TypeScript.
 *
 * This is deliberately code-as-content: loops and arithmetic make a large
 * authored fixture pleasant to maintain, but the emitted result is strict data
 * with no callback, behavior object, or second runtime lifecycle.
 */
export function motionLabProject(bodyCount: number): ProjectContent {
  if (!Number.isSafeInteger(bodyCount) || bodyCount < 1 || bodyCount > 4_096) {
    throw new Error("body count must be an integer in 1..4096");
  }

  const entities: EntityDefinition[] = Array.from({ length: bodyCount }, (_, index) => ({
    id: MOTION_LAB_FIRST_ENTITY_ID + index,
    name: `runner-${String(index).padStart(4, "0")}`,
    translation: [-0.5 * (index % 8), 0.5, index + 0.5],
    renderable: { asset: "primitive/motion-runner", visible: true },
    kinematic: {
      halfExtents: [0.2, 0.2, 0.2],
      velocity: [4 + (index % 5), 0, 0],
    },
  }));
  const wall: VoxelAddress[] = Array.from(
    { length: bodyCount },
    (_, index) => [8, 0, index] as const,
  );

  return {
    schemaVersion: 6,
    entities,
    voxelCollision: {
      voxelSize: 1,
      chunkSize: 8,
      solidVoxels: wall,
    },
  };
}

export const generatedMotionProjects = {
  "motion-lab.project.json": motionLabProject(MOTION_LAB_BODY_COUNT),
} as const;
