import assert from "node:assert/strict";
import test from "node:test";

import * as THREE from "three";

import {
  entityId,
  renderHandle,
  type Material,
  type MeshPayloadDescriptor,
  type RenderNode,
} from "@rusty-engine-demo/render-contracts";

import { RenderApplyError, ThreeRenderer } from "./three-renderer.js";

const RED: Material = { color: [1, 0, 0, 1], wireframe: false };
const BLUE: Material = { color: [0, 0, 1, 1], wireframe: false };

test("typed lifecycle operations retain hierarchy and partial visual updates", () => {
  const renderer = new ThreeRenderer();
  const parent = renderHandle(1);
  const child = renderHandle(2);
  renderer.applyFrame({
    ops: [
      { op: "create", handle: parent, parent: null, node: node("parent", RED) },
      { op: "create", handle: child, parent, node: node("child", RED) },
      {
        op: "update",
        handle: child,
        transform: {
          translation: [3, 2, 1],
          rotation: [0, 0, 0, 1],
          scale: [2, 2, 2],
        },
        material: BLUE,
        visible: false,
        metadata: { source: entityId(9), tags: [], label: "updated-child" },
      },
    ],
  });

  const object = renderer.objectFor(child);
  assert.ok(object instanceof THREE.Mesh);
  assert.deepEqual(object.position.toArray(), [3, 2, 1]);
  assert.equal(object.visible, false);
  assert.equal(object.name, "updated-child");
  assert.match(renderer.snapshot(), /label "updated-child"/);

  renderer.applyFrame({
    ops: [
      { op: "destroy", handle: parent },
      { op: "destroy", handle: child },
    ],
  });
  assert.equal(renderer.handleCount, 0);
  assert.equal(renderer.snapshot(), "(empty scene)\n");
});

test("inline mesh replacement validates first and preserves the prior mesh on failure", () => {
  const renderer = new ThreeRenderer();
  const handle = renderHandle(3);
  renderer.applyFrame({
    ops: [{ op: "create", handle, parent: null, node: node("voxel-mesh", RED) }],
  });
  renderer.applyFrame({ ops: [{ op: "replaceMeshPayload", handle, payload: triangle() }] });

  const object = renderer.objectFor(handle);
  assert.ok(object instanceof THREE.Mesh);
  const installedGeometry = object.geometry;
  assert.equal(installedGeometry.getAttribute("position").count, 3);
  assert.equal(installedGeometry.getIndex()?.count, 3);
  assert.equal(installedGeometry.groups.length, 1);
  assert.ok(object.material instanceof THREE.MeshStandardMaterial);
  assert.match(renderer.snapshot(), /shape mesh/);

  const malformed: MeshPayloadDescriptor = {
    ...triangle(),
    source: { ...triangle().source, indices: [0, 1, 8] },
  };
  assert.throws(
    () => renderer.applyFrame({
      ops: [{ op: "replaceMeshPayload", handle, payload: malformed }],
    }),
    (error: unknown) =>
      error instanceof RenderApplyError && error.message.includes("outside vertex count"),
  );
  assert.equal(object.geometry, installedGeometry);
});

test("unknown and duplicate handles fail closed", () => {
  const renderer = new ThreeRenderer();
  const handle = renderHandle(5);
  renderer.applyFrame({
    ops: [{ op: "create", handle, parent: null, node: node("only", RED) }],
  });
  assert.throws(
    () => renderer.applyFrame({
      ops: [{ op: "create", handle, parent: null, node: node("duplicate", RED) }],
    }),
    /already exists/,
  );
  assert.throws(
    () => renderer.applyFrame({ ops: [{ op: "destroy", handle: renderHandle(99) }] }),
    /unknown handle/,
  );
});

function node(label: string, material: Material): RenderNode {
  return {
    geometry: { shape: "cube" },
    material,
    transform: {
      translation: [0, 0, 0],
      rotation: [0, 0, 0, 1],
      scale: [1, 1, 1],
    },
    visible: true,
    layer: "scene",
    metadata: { source: null, tags: [], label },
  };
}

function triangle(): MeshPayloadDescriptor {
  return {
    layout: {
      vertexCount: 3,
      indexCount: 3,
      indexWidth: "u32",
      attributes: [
        { name: "position", components: 3, kind: "f32" },
        { name: "normal", components: 3, kind: "f32" },
      ],
    },
    groups: [{ materialSlot: 2, start: 0, count: 3 }],
    bounds: { min: [0, 0, 0], max: [1, 1, 0] },
    source: {
      kind: "inline",
      positions: [0, 0, 0, 1, 0, 0, 0, 1, 0],
      normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
      indices: [0, 1, 2],
    },
    provenance: "voxelChunk",
  };
}
