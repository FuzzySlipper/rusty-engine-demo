# Animated mesh capture certification

The Loading Bay product certifies Rusty Engine's public animated-mesh capture
surface against the two authored actors in the checked project. The capture
path does not create a second renderer: it locates the retained animated-mesh
instance on the live `RendererSurface`, applies a fixed camera, and calls
`captureRendererAnimatedMesh` from the exact pinned Engine provider.

Run the browser-owned capture from the repository root:

```bash
pnpm run capture:actor-animation
```

The owner writes 60 fixed-time PNG samples, 12 five-frame contact sheets, their
manifests, and `certification.json` under
`docs/evidence/animated-mesh-contact-sheets/`. It covers both actors, all six
authored clips, normalized times `0`, `0.25`, `0.5`, `0.75`, and `1`, a
640-by-640 viewport, resize to 900-by-600, disposal, remount, and a final
post-remount capture. Two clean regenerations produced the same capture tree
SHA-256:

```text
11745f1c5510f92f2662f8cad83bffe9dbe1806de36f9a05293f6c0ff6aac54e
```

The capture certification belongs to Engine revision
`d5fac3dd01326590594342104c00c85917bc1e99`. Its manifest SHA-256 is
`2cad7334d33914a92c02d60b48997400fa11654b830c6bed62168292bd8fe7dd`.

## Independent source comparison

Run the non-browser source comparison with:

```bash
pnpm run check:actor-animation
```

This diagnostic loads the same exact GLB bytes independently with Three's
`GLTFLoader`, clones them with `SkeletonUtils`, samples them with a direct
`AnimationMixer`, and compares that result with Engine's public exact-time
sampler. It covers 60 actor/clip/time samples plus clip switching, fading,
translated instances, and cross-instance isolation. The checked result is
`source-equivalence.json` (SHA-256
`74eeee5c895b99d4d4f2928bfd7d213943ac858dc64e364555162f7ccfd86646`).

The comparison found zero bounds or vertex-count divergence. It therefore does
not justify renderer-side compensation. The contact sheets instead preserve
the authored-source limitations: attack and hit motion is visually slight, and
the death clip carries the actors below their source ground plane. Those are
content-authoring findings, not Engine import, bind, interpolation, switching,
or multi-instance defects.
