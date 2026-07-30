import "@angular/compiler";

import assert from "node:assert/strict";
import test from "node:test";

import type { RenderFrameDiff } from "@rusty-engine/render-contracts";
import type {
  RendererEditorViewportChannelReceipt,
  RendererSurfaceSubmissionSample,
} from "@rusty-engine/renderer-host";
import {
  submitStudioViewportFrame,
  type StudioViewportFrameSubmitted,
} from "@rusty-engine/studio-viewport";

import {
  appendStudioFrameSubmission,
  MAX_RECORDED_STUDIO_FRAME_SUBMISSIONS,
  studioFrameSubmissionEvidence,
} from "./studio-frame-submission.ts";

function submitted(
  generation: number,
  updateKind: StudioViewportFrameSubmitted["updateKind"],
): StudioViewportFrameSubmitted {
  return Object.freeze({
    kind: "rusty_studio_viewport_frame_submitted.v1",
    generation,
    updateKind,
    submission: Object.freeze({
      schemaVersion: 1,
      renderSequence: generation,
      source: "explicit",
      sourceTimeMs: generation,
      frameIntervalMs: generation === 1 ? null : 16.7,
      frameIntervalStatus: generation === 1 ? "firstFrame" : "available",
      backendSubmissionDurationMs: 1.25,
      backendSubmissionDurationStatus: "available",
      statistics: Object.freeze({
        schemaVersion: 1,
        drawCallCount: Object.freeze({
          scope: "perSubmission",
          status: "available",
          value: 71,
        }),
        renderHandleCount: Object.freeze({
          scope: "liveResident",
          status: "available",
          value: 84,
        }),
        geometryResourceCount: Object.freeze({
          scope: "liveResident",
          status: "available",
          value: 44,
        }),
        materialResourceCount: Object.freeze({
          scope: "liveResident",
          status: "available",
          value: 12,
        }),
        textureResourceCount: Object.freeze({
          scope: "liveResident",
          status: "available",
          value: 0,
        }),
        animatedInstanceCount: Object.freeze({
          scope: "liveResident",
          status: "available",
          value: 0,
        }),
        triangleCount: Object.freeze({
          scope: "liveResident",
          status: "available",
          value: 124_340,
        }),
      }),
    }),
  });
}

test("submission evidence retains exact public events in a bounded history", () => {
  let history: readonly StudioViewportFrameSubmitted[] = [];
  const kinds = ["complete", "incremental", "presentation"] as const;
  const total = MAX_RECORDED_STUDIO_FRAME_SUBMISSIONS + 5;
  for (let generation = 1; generation <= total; generation += 1) {
    history = appendStudioFrameSubmission(
      history,
      submitted(generation, kinds[(generation - 1) % kinds.length]),
    );
  }

  const evidence = studioFrameSubmissionEvidence(total, history);
  assert.equal(history.length, MAX_RECORDED_STUDIO_FRAME_SUBMISSIONS);
  assert.equal(evidence.count, total);
  assert.equal(evidence.latest?.generation, total);
  assert.deepEqual(evidence.latest?.submission.statistics.drawCallCount, {
    scope: "perSubmission",
    status: "available",
    value: 71,
  });
  assert(Object.isFrozen(history));
  assert(Object.isFrozen(evidence));
  assert(Object.isFrozen(evidence.updateKinds));
});

test("public complete, incremental, and presentation submissions reach downstream evidence unchanged", () => {
  const frame: RenderFrameDiff = Object.freeze({
    schemaVersion: 1,
    ops: Object.freeze([]),
  });
  const accepted: RendererEditorViewportChannelReceipt = Object.freeze({
    applied: true,
    channel: "authored",
    diagnostics: Object.freeze([]),
    generation: 1,
    snapshotHash: "accepted",
  });

  class PublicSubmissionSurface {
    latest: RendererSurfaceSubmissionSample = submitted(1, "complete")
      .submission;
    readonly methods: string[] = [];

    applyAuthoredFrame(): RendererEditorViewportChannelReceipt {
      this.methods.push("apply");
      return accepted;
    }

    replaceFrame(): RendererEditorViewportChannelReceipt {
      this.methods.push("replace");
      return accepted;
    }

    renderOnce(): void {
      this.latest = submitted(
        this.latest.renderSequence + 1,
        "complete",
      ).submission;
    }

    submission(): RendererSurfaceSubmissionSample {
      return this.latest;
    }
  }

  const surface = new PublicSubmissionSurface();
  let history: readonly StudioViewportFrameSubmitted[] = [];
  for (const [index, updateKind] of (
    ["complete", "incremental", "presentation"] as const
  ).entries()) {
    const result = submitStudioViewportFrame(
      surface,
      frame,
      100 + index,
      updateKind,
    );
    assert(result.event !== null);
    history = appendStudioFrameSubmission(history, result.event);
    assert.equal(history.at(-1), result.event);
  }

  assert.deepEqual(surface.methods, ["replace", "apply", "replace"]);
  assert.deepEqual(studioFrameSubmissionEvidence(3, history).updateKinds, [
    "complete",
    "incremental",
    "presentation",
  ]);
});
