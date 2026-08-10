# Design and authority boundary

Rusty Engine Demo is a downstream game, not an Engine extension layer. Objects carry admitted
facts, named Rust services own Loading Bay mechanisms, and the product owns meaning and
orchestration.

## Dependency direction

The only normal Rust provider dependency is the adjacent complete `rusty-engine` facade. Downstream
imports preserved owner namespaces such as `rusty_engine::entity_state` and
`rusty_engine::render_model`; it does not select an accidental subset of provider crates or manage
the sibling checkout through versions, Git pins, SHAs, freshness checks, or update helpers.

Rusty Studio is an Engine-hosted product. This repository exposes `.rusty-studio.json`, project
content, and its Rust adapter; it does not install or import Studio or renderer packages.

## Runtime authority

- Loading Bay Rust owns project admission, live gameplay state, fixed phases, consequences,
  snapshots, saves, and persistence.
- Browser TypeScript owns semantic device capture, typed transport, HUD/readout composition, and
  bounded startup or failure state. It transports the complete Rust-projected content aggregate
  without deriving renderer manifests or backend configuration.
- The native product owns its window, mount rectangle, timing, semantic input mapping, picks and
  their consequences, and product resource selection.
- Rusty Engine owns retained rendering, the Rust-to-TypeScript decoder border, the private renderer
  artifact, resource lifecycle, and renderer cleanup. Downstream calls only named Rust adapter
  operations.

The renderer observes Rust facts. A renderer failure can discard presentation, but cannot publish
or retain a second gameplay, project, or save state.

Studio protocol 15 exposes the Engine-owned voxel surface selector for voxel-object entities. The
dropdown is observational TypeScript wiring over a closed Rust mutation: Loading Bay persists the
per-instance `surfaceMode`, guards it with the exact project hash, stages complete admission and
projection before atomic replacement, and returns the canonical readout. Engine remains the owner
of greedy-cube, marching-cubes, and dual-contouring meshing and of their renderer resource
lifecycle. Textured reconstructed surfaces reject until Engine has a stable UV contract; Studio
never reaches into the renderer or keeps a local mode override.

## Concrete hosts

`browser-host` remains the game-specific transport and diagnostics host. Its Rust projection feeds
one bundled `@rusty-engine/application-host`, which owns renderer/DOM composition while Angular
owns Loading Bay's rich HUD, forms, menus, and accessibility tree. The identical web application
runs in an ordinary browser and the Tauri wrapper; only the existing Rust transport/sidecar launch
varies. `native-host` remains a focused Rust-adapter acceptance product for named operations,
physical input, picking, resource admission, transactional mount failure, and disposal.

There is intentionally no generic command tunnel, eval seam, callback registry, renderer-package
graph, or downstream renderer bootstrap. The one application-host import exposes only bounded
frame, camera, interaction-mode, and lifecycle ports.
