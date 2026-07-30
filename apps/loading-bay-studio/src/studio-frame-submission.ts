import type { StudioViewportFrameSubmitted } from "@rusty-engine/studio-viewport";

export const MAX_RECORDED_STUDIO_FRAME_SUBMISSIONS = 32;

export interface StudioFrameSubmissionEvidence {
  readonly count: number;
  readonly updateKinds: readonly StudioViewportFrameSubmitted["updateKind"][];
  readonly latest: StudioViewportFrameSubmitted | null;
}

export function appendStudioFrameSubmission(
  history: readonly StudioViewportFrameSubmitted[],
  event: StudioViewportFrameSubmitted,
): readonly StudioViewportFrameSubmitted[] {
  return Object.freeze(
    [...history, event].slice(-MAX_RECORDED_STUDIO_FRAME_SUBMISSIONS),
  );
}

export function studioFrameSubmissionEvidence(
  totalCount: number,
  history: readonly StudioViewportFrameSubmitted[],
): StudioFrameSubmissionEvidence {
  return Object.freeze({
    count: totalCount,
    updateKinds: Object.freeze(history.map(({ updateKind }) => updateKind)),
    latest: history.at(-1) ?? null,
  });
}
