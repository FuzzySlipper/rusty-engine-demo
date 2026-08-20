# Loading Bay design and authority boundary

Loading Bay is one Rusty Engine downstream game. Its supported content is the authored Doom E1M1 project; retired fixture projects are not alternate products.

## Authority

`LoadingBayProductService` is the game-specific, transport-neutral Rust authority. It admits the authored project, builds and advances the fixed-step loop, accepts typed semantic commands, owns session generations, saves, facts, and projection readouts. It knows no socket, Tauri, or renderer implementation.

Rust owns runtime state, validation, mutation, scheduling, persistence, and projection. TypeScript owns immutable content composition, browser semantic-input capture, and disposable HUD/product-shell presentation. It does not become a second gameplay evaluator.

## Adapters

`browser-host` is the normal development adapter: it translates bounded `loading-bay.v2` WebSocket messages to the service and exposes read-only diagnostics. The Angular shell consumes the projected content through Rusty Engine's public application-host.

Tauri is the final-product adapter. It packages the same shell in one WebView and calls typed in-process commands over the same service. It has no packaged browser-host process, loopback readiness protocol, asset-hash handshake, orphan process cleanup, or second product window.

Rusty Engine alone owns renderer and canvas lifetime. This repository neither imports a private bridge nor builds a competing renderer/resource cache/frame loop.

## Dependency and extension boundary

The only Rust provider dependency is the complete adjacent `rusty-engine` facade, through one unconditional path dependency. Owner namespaces remain explicit. This repo does not pin or manage the sibling checkout, and it does not select Engine subcrates.

Game-owned semantics belong here first. A reusable Engine seam requires evidence from another real consumer. Do not introduce a plugin registry, service locator, generic method-name bridge, behavior IR, replay/certification framework, or live TypeScript authority.

## Content and evidence

`content/projects/doom-e1m1.project.json` is the sole canonical project. E1M1 textures, sprites, voxel data, and the eight-prop closure live under `content/doom-e1m1/`; exact source boundaries are in [source-provenance.md](source-provenance.md). Historical campaign material belongs in Den, not as active repository documentation or proof.
