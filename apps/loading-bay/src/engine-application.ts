import { InjectionToken } from "@angular/core";
import type {
  RustyApplicationCameraPose,
  RustyApplicationFrame,
  RustyApplicationFrameReceipt,
  RustyApplicationUiContext,
} from "@rusty-engine/application-host";

/** The sole public Engine web-host seam composed by this product bootstrap. */
export const ENGINE_APPLICATION = new InjectionToken<RustyApplicationUiContext>(
  "ENGINE_APPLICATION",
);

interface RendererRouteAuthority {
  generation: number;
  mutationQueue: Promise<void>;
  sessionRetirement: Promise<void>;
}

export interface EngineRendererRouteLease {
  readonly clearIfOwned: () => Promise<boolean>;
  readonly generation: number;
  readonly publish: (
    frame: RustyApplicationFrame,
    camera: RustyApplicationCameraPose,
    replaceFrame: boolean,
  ) => Promise<RustyApplicationFrameReceipt | null>;
  readonly retire: () => void;
}

const rendererRouteAuthorities = new WeakMap<
  RustyApplicationUiContext,
  RendererRouteAuthority
>();

/**
 * Serialize application-scoped renderer mutations behind a route generation.
 * A retired route may finish transport cleanup, but it can no longer publish or
 * clear after a successor has claimed the Engine-owned surface.
 */
export function claimEngineRendererRoute(
  application: RustyApplicationUiContext,
): EngineRendererRouteLease {
  const authority = rendererRouteAuthorities.get(application) ?? {
    generation: 0,
    mutationQueue: Promise.resolve(),
    sessionRetirement: Promise.resolve(),
  };
  rendererRouteAuthorities.set(application, authority);
  const generation = ++authority.generation;
  let retired = false;

  const enqueue = <Result>(
    operation: () => Promise<Result> | Result,
  ): Promise<Result | null> => {
    const queued = authority.mutationQueue
      .catch(() => undefined)
      .then(async () => {
        if (retired || authority.generation !== generation) return null;
        return await operation();
      });
    authority.mutationQueue = queued.then(
      () => undefined,
      () => undefined,
    );
    return queued;
  };

  return {
    generation,
    publish: (frame, camera, replaceFrame) =>
      enqueue(async () => {
        let receipt: RustyApplicationFrameReceipt = {
          applied: true,
          diagnostics: [],
        };
        if (replaceFrame) {
          receipt = await application.renderer.replaceFrame(frame);
        }
        if (authority.generation === generation && !retired) {
          application.renderer.setCameraPose(camera);
        }
        return receipt;
      }),
    retire: () => {
      retired = true;
    },
    clearIfOwned: async () => {
      if (authority.generation !== generation) return false;
      const queued = authority.mutationQueue
        .catch(() => undefined)
        .then(async () => {
          if (authority.generation !== generation) return false;
          await application.renderer.clear();
          return true;
        });
      authority.mutationQueue = queued.then(
        () => undefined,
        () => undefined,
      );
      return await queued;
    },
  };
}

/** Wait until the previous game route has released the single Rust session. */
export function waitForEngineRouteSessionRetirement(
  application: RustyApplicationUiContext,
): Promise<void> {
  return (
    rendererRouteAuthorities.get(application)?.sessionRetirement ??
    Promise.resolve()
  );
}

/** Publish teardown before a successor route is allowed to connect to Rust. */
export function queueEngineRouteSessionRetirement(
  application: RustyApplicationUiContext,
  retirement: () => Promise<void>,
): Promise<void> {
  const authority = rendererRouteAuthorities.get(application);
  if (authority === undefined) {
    throw new Error("Engine renderer route must be claimed before retirement");
  }
  const queued = authority.sessionRetirement
    .catch(() => undefined)
    .then(retirement);
  authority.sessionRetirement = queued.catch(() => undefined);
  return queued;
}
