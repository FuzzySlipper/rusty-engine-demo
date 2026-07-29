import assert from "node:assert/strict";
import { test } from "node:test";

import type {
  RendererSurfaceStatisticsSample,
  RendererSurfaceSubmissionSample,
} from "@rusty-engine/renderer-host";

import {
  captureLoadingBayRendererStatisticsProof,
  cleanupFrame,
  contentRichFrame,
} from "./renderer-statistics-proof.ts";

test("content-rich probe uses shared assets under one disposable viewmodel tree", () => {
  const frame = contentRichFrame();
  const definitions = frame.ops.filter(
    (operation) => operation.op === "defineStaticMesh",
  );
  const instances = frame.ops.filter(
    (operation) => operation.op === "createStaticMeshInstance",
  );
  const roots = frame.ops.filter((operation) => operation.op === "create");

  assert.equal(definitions.length, 4);
  assert.equal(instances.length, 32);
  assert.equal(roots.length, 1);
  assert.equal(roots[0]?.node.layer, "viewmodel");
  assert.ok(
    instances.every((operation) => operation.parent === roots[0]?.handle),
  );
  assert.deepEqual(cleanupFrame().ops, [
    { op: "destroy", handle: roots[0]?.handle },
  ]);
});

test("probe samples one surface explicitly and restores every renderer statistic", () => {
  const placeholder = submission(1, statistics(17, 40, 9, 8, 2, 1, 600));
  const contentRich = submission(2, statistics(49, 73, 13, 12, 2, 1, 664));
  const restored = submission(3, placeholder.statistics);
  const submissions = [placeholder, contentRich, restored];
  const renderReturns = [
    submission(101, statistics(0, 0, 0, 0, 0, 0, 0)),
    submission(102, statistics(0, 0, 0, 0, 0, 0, 0)),
    submission(103, statistics(0, 0, 0, 0, 0, 0, 0)),
  ];
  const frames = [];
  let latest = submission(0, statistics(0, 0, 0, 0, 0, 0, 0));
  const surface = {
    applyFrame: (frame: ReturnType<typeof contentRichFrame>) => {
      frames.push(frame);
      return { applied: true, diagnostics: [] } as const;
    },
    renderOnce: () => {
      const next = submissions.shift();
      if (next === undefined) throw new Error("unexpected renderer submission");
      latest = next;
      const returned = renderReturns.shift();
      if (returned === undefined) throw new Error("unexpected render return");
      return returned;
    },
    submission: () => latest,
  };

  const proof = captureLoadingBayRendererStatisticsProof(surface, () => 1);

  assert.equal(Object.isFrozen(proof), true);
  assert.equal(proof.placeholder, placeholder);
  assert.equal(proof.contentRich, contentRich);
  assert.equal(proof.restored, restored);
  assert.equal(frames.length, 2);
  assert.equal(submissions.length, 0);
  assert.equal(renderReturns.length, 0);
});

test("probe reads stored submissions, rejects a bad richer count, and cleans up", () => {
  const placeholder = submission(1, statistics(17, 40, 9, 8, 2, 1, 600));
  const badContentRich = submission(2, statistics(48, 73, 13, 12, 2, 1, 664));
  const cleanupSubmission = submission(3, placeholder.statistics);
  const submissions = [placeholder, badContentRich, cleanupSubmission];
  const renderReturns = [
    submission(101, statistics(17, 40, 9, 8, 2, 1, 600)),
    submission(102, statistics(49, 73, 13, 12, 2, 1, 664)),
    submission(103, statistics(17, 40, 9, 8, 2, 1, 600)),
  ];
  const frames = [];
  let latest = submission(0, statistics(0, 0, 0, 0, 0, 0, 0));
  const surface = {
    applyFrame: (frame: ReturnType<typeof contentRichFrame>) => {
      frames.push(frame);
      return { applied: true, diagnostics: [] } as const;
    },
    renderOnce: () => {
      const next = submissions.shift();
      if (next === undefined) throw new Error("unexpected renderer submission");
      latest = next;
      const returned = renderReturns.shift();
      if (returned === undefined) throw new Error("unexpected render return");
      return returned;
    },
    submission: () => latest,
  };

  assert.throws(
    () => captureLoadingBayRendererStatisticsProof(surface, () => 1),
    /drawCallCount delta was 31; expected 32/u,
  );
  assert.equal(frames.length, 2, "finally applied the one cleanup frame");
  assert.equal(
    submissions.length,
    0,
    "finally submitted the restored surface once",
  );
  assert.equal(
    renderReturns.length,
    0,
    "renderOnce return values were not used as proof samples",
  );
});

function statistics(
  drawCallCount: number,
  renderHandleCount: number,
  geometryResourceCount: number,
  materialResourceCount: number,
  textureResourceCount: number,
  animatedInstanceCount: number,
  triangleCount: number,
): RendererSurfaceStatisticsSample {
  return {
    schemaVersion: 1,
    drawCallCount: {
      scope: "perSubmission",
      status: "available",
      value: drawCallCount,
    },
    renderHandleCount: {
      scope: "liveResident",
      status: "available",
      value: renderHandleCount,
    },
    geometryResourceCount: {
      scope: "liveResident",
      status: "available",
      value: geometryResourceCount,
    },
    materialResourceCount: {
      scope: "liveResident",
      status: "available",
      value: materialResourceCount,
    },
    textureResourceCount: {
      scope: "liveResident",
      status: "available",
      value: textureResourceCount,
    },
    animatedInstanceCount: {
      scope: "liveResident",
      status: "available",
      value: animatedInstanceCount,
    },
    triangleCount: {
      scope: "perSubmission",
      status: "available",
      value: triangleCount,
    },
  };
}

function submission(
  renderSequence: number,
  rendererStatistics: RendererSurfaceStatisticsSample,
): RendererSurfaceSubmissionSample {
  return {
    schemaVersion: 1,
    renderSequence,
    source: "explicit",
    sourceTimeMs: renderSequence,
    frameIntervalMs: renderSequence === 1 ? null : 1,
    frameIntervalStatus: renderSequence === 1 ? "firstFrame" : "available",
    backendSubmissionDurationMs: 0.1,
    backendSubmissionDurationStatus: "available",
    statistics: rendererStatistics,
  };
}
