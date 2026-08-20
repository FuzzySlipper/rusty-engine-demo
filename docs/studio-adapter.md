# Loading Bay Studio adapter

`studio-adapter` is Loading Bay's project-owned Rust authoring boundary for Rusty Engine Studio. It is not a gameplay host, a generic component API, or a second renderer.

The adapter opens an explicit project root, admits Loading Bay project bytes, and exposes a closed JSON-lines operation set for project, scene, asset, transform, collision, material, voxel, and history work. Rust validates every request and atomically publishes an admitted canonical project only after complete candidate validation. Relative project paths remain under the selected root; explicit host-file reads are bounded and path-validated.

Studio uses Engine-owned hierarchy, inspection, persistence, and renderer facilities. Loading Bay keeps game-specific values and mutations behind named closed contracts. The current weapon contract is [weapon-authoring-contract.md](weapon-authoring-contract.md). No downstream Studio UI, renderer package, callback bridge, arbitrary command payload, or browser-side persistence is supported.

Run focused Rust adapter tests when changing this surface, then the normal product gate:

```bash
cargo test --locked -p loading-bay-game --test studio_adapter
pnpm run verify
```
