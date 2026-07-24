import * as THREE from "three";

import type { RenderFrameDiff } from "@rusty-engine-demo/render-contracts";

import { ThreeRenderer } from "./three-renderer.js";

export interface RendererBrowserCameraPose {
  readonly position: readonly [number, number, number];
  readonly pitchDegrees: number;
  readonly yawDegrees: number;
}

export interface RendererBrowserCameraBasis {
  readonly forward: readonly [number, number, number];
  readonly right: readonly [number, number, number];
  readonly up: readonly [number, number, number];
}

export interface PerspectiveProjection {
  readonly fovYDegrees: number;
  readonly near: number;
  readonly far: number;
}

export interface RendererBrowserSurfaceOptions {
  readonly autoStart?: boolean;
  readonly camera?: {
    readonly initialBasis?: RendererBrowserCameraBasis;
    readonly initialPose?: RendererBrowserCameraPose;
    readonly projection?: PerspectiveProjection;
  };
  readonly clearColor?: number;
  readonly frame?: RenderFrameDiff;
  readonly pixelRatio?: number;
}

export interface RendererBrowserSurface {
  readonly kind: "rusty_renderer_browser_surface.v0";
  readonly applyFrame: (frame: RenderFrameDiff) => void;
  readonly dispose: () => void;
  readonly renderOnce: (timeMs?: number) => void;
  readonly setCameraPose: (
    pose: RendererBrowserCameraPose,
    basis?: RendererBrowserCameraBasis,
  ) => void;
  readonly snapshot: () => string;
  readonly start: () => void;
  readonly stop: () => void;
}

/** Mount the Rusty-owned typed render projection onto a real Three/WebGL canvas. */
export function mountRendererBrowserSurface(
  canvas: HTMLCanvasElement,
  options: RendererBrowserSurfaceOptions = {},
): RendererBrowserSurface {
  const renderer = new ThreeRenderer();
  const ambientLight = new THREE.HemisphereLight(0xffffff, 0x263238, 2.4);
  const keyLight = new THREE.DirectionalLight(0xffffff, 2.2);
  keyLight.position.set(5, 8, 6);
  renderer.scene.add(ambientLight, keyLight);
  renderer.applyFrame(options.frame ?? { ops: [] });

  const webgl = new THREE.WebGLRenderer({ canvas, antialias: true });
  webgl.setClearColor(options.clearColor ?? 0x101820, 1);
  const pixelRatio = validatePixelRatio(
    options.pixelRatio ?? globalThis.devicePixelRatio ?? 1,
  );
  webgl.setPixelRatio(pixelRatio);

  const projection = validatePerspectiveProjection(
    options.camera?.projection ?? { fovYDegrees: 55, near: 0.1, far: 100 },
  );
  const camera = new THREE.PerspectiveCamera(
    projection.fovYDegrees,
    1,
    projection.near,
    projection.far,
  );
  const cameraLookTarget = new THREE.Vector3();
  let currentPose = options.camera?.initialPose ?? {
    position: [0, 1.62, 8] as const,
    pitchDegrees: 0,
    yawDegrees: 0,
  };
  let currentBasis = options.camera?.initialBasis ?? null;
  let animationFrame: number | null = null;
  let viewport = { width: 0, height: 0 };

  const setCameraPose = (
    pose: RendererBrowserCameraPose,
    basis?: RendererBrowserCameraBasis,
  ): void => {
    currentPose = pose;
    currentBasis = basis ?? null;
    camera.position.set(...pose.position);
    if (currentBasis === null) {
      camera.up.set(0, 1, 0);
      camera.rotation.order = "YXZ";
      camera.rotation.x = degreesToRadians(pose.pitchDegrees);
      camera.rotation.y = degreesToRadians(pose.yawDegrees);
      camera.rotation.z = 0;
      return;
    }
    camera.up.set(...currentBasis.up);
    cameraLookTarget.set(
      camera.position.x + currentBasis.forward[0],
      camera.position.y + currentBasis.forward[1],
      camera.position.z + currentBasis.forward[2],
    );
    camera.lookAt(cameraLookTarget);
  };

  const resize = (): void => {
    const width = Math.max(
      1,
      canvas.clientWidth || Math.round(canvas.width / pixelRatio) || 800,
    );
    const height = Math.max(
      1,
      canvas.clientHeight || Math.round(canvas.height / pixelRatio) || 450,
    );
    if (viewport.width !== width || viewport.height !== height) {
      webgl.setSize(width, height, false);
      viewport = { width, height };
    }
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
  };

  const renderOnce = (): void => {
    resize();
    webgl.render(renderer.scene, camera);
  };

  const tick = (): void => {
    renderOnce();
    animationFrame = globalThis.requestAnimationFrame(tick);
  };

  const start = (): void => {
    if (animationFrame === null) {
      animationFrame = globalThis.requestAnimationFrame(tick);
    }
  };

  const stop = (): void => {
    if (animationFrame !== null) {
      globalThis.cancelAnimationFrame(animationFrame);
      animationFrame = null;
    }
  };

  const dispose = (): void => {
    stop();
    renderer.dispose();
    webgl.dispose();
  };

  setCameraPose(currentPose, currentBasis ?? undefined);
  renderOnce();
  if (options.autoStart !== false) {
    start();
  }

  return {
    kind: "rusty_renderer_browser_surface.v0",
    applyFrame: (frame) => renderer.applyFrame(frame),
    dispose,
    renderOnce,
    setCameraPose,
    snapshot: () => renderer.snapshot(),
    start,
    stop,
  };
}

function validatePerspectiveProjection(projection: PerspectiveProjection): PerspectiveProjection {
  if (
    ![projection.fovYDegrees, projection.near, projection.far].every(Number.isFinite)
    || projection.fovYDegrees <= 0
    || projection.fovYDegrees >= 180
    || projection.near <= 0
    || projection.far <= projection.near
  ) {
    throw new RangeError(
      "camera projection must have a finite FOV in (0, 180) and 0 < near < far",
    );
  }
  return { ...projection };
}

function validatePixelRatio(pixelRatio: number): number {
  if (!Number.isFinite(pixelRatio) || pixelRatio <= 0) {
    throw new RangeError("renderer pixel ratio must be finite and greater than zero");
  }
  return pixelRatio;
}

function degreesToRadians(degrees: number): number {
  return (degrees * Math.PI) / 180;
}
