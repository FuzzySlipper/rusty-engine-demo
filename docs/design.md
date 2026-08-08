# Design and authority boundary

Rusty Engine Demo is a downstream game, not an Engine extension layer. Objects carry admitted
facts, named Rust services own Loading Bay mechanisms, and the product owns meaning and
orchestration.

## Dependency direction

The only normal Rust provider dependency is the rolling-current `rusty-engine` facade. Downstream
imports preserved owner namespaces such as `rusty_engine::entity_state` and
`rusty_engine::render_model`; it does not select an accidental subset of provider crates. The
checked lock records the exact resolved public SHA, and `engine:freshness` fails when that SHA is
behind the canonical public `main` branch.

Engine Studio packages are first-party authoring dependencies. Their renderer peers appear only as
exact root dev resolvers because the Git packages currently publish those peers externally. No
ordinary product package depends on them and no downstream source may import them.

## Runtime authority

- Loading Bay Rust owns project admission, live gameplay state, fixed phases, consequences,
  snapshots, saves, and persistence.
- Browser TypeScript owns semantic device capture, transport, HUD/readout composition, and bounded
  startup or failure state. Renderer payload fields crossing the existing browser protocol are
  opaque compatibility data and are not interpreted as a renderer API.
- The native product owns its window, mount rectangle, timing, semantic input mapping, picks and
  their consequences, and product resource selection.
- Rusty Engine owns retained rendering, the Rust-to-TypeScript decoder border, the private renderer
  artifact, resource lifecycle, and renderer cleanup. Downstream calls only named Rust adapter
  operations.

The renderer observes Rust facts. A renderer failure can discard presentation, but cannot publish
or retain a second gameplay, project, or save state.

## Concrete hosts

`browser-host` remains the game-specific transport and diagnostics host. Its browser client is a
control/HUD shell and does not mount an Engine renderer. `native-host` is the concrete rendered
Loading Bay product: it admits the canonical project, packages a checked-in product GLB through
Engine Rust types, submits retained frames, maps physical input to player-controller mutations,
maps picks to a named game interaction, and verifies transactional mount failure and disposal.

There is intentionally no generic command tunnel, eval seam, callback registry, shared downstream
renderer bootstrap, or universal host abstraction.
