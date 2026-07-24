import assert from "node:assert/strict";
import test from "node:test";

import {
  MOTION_LAB_BODY_COUNT,
  MOTION_LAB_FIRST_ENTITY_ID,
  motionLabProject,
} from "./motion-project.js";

test("motion lab is composed as data with one lane and wall voxel per body", () => {
  const project = motionLabProject(32);

  assert.equal(project.entities.length, 32);
  assert.equal(project.voxelCollision?.solidVoxels.length, 32);
  assert.equal(project.entities[0]?.id, MOTION_LAB_FIRST_ENTITY_ID);
  assert.deepEqual(project.entities[0]?.kinematic?.velocity, [4, 0, 0]);
  assert.deepEqual(project.voxelCollision?.solidVoxels.at(-1), [8, 0, 31]);
});

test("checked-in workload size remains explicit", () => {
  assert.equal(MOTION_LAB_BODY_COUNT, 256);
  assert.throws(() => motionLabProject(0), /body count/);
});
