# Loading Bay Studio adapter

The `studio-adapter` binary is Loading Bay's project-owned Rust composition boundary for Rusty
Engine Studio. It is not a gameplay runtime facade and it does not generalize Loading Bay concepts
into Engine vocabulary.

Protocol version 4 is a closed, named operation set. It covers project open/read/close; typed scene
translation; material and palette authoring; canonical voxel asset and transformed-instance
lifecycle; authoritative picking; brush and primitive edits; deterministic templates; durable
history query/preview/apply; every typed annotation edit/query/export family; bounded model and GLB
conversion planning; trusted host voxel/GLB/license files; and deterministic environment
materialization. Requests and responses are one bounded JSON value per line. There is no method-name
dispatch, provider registry, arbitrary command payload, callback subscription, HTTP route, or
browser persistence seam.

Prepared conversion plans and history reverts remain private adapter state. A later apply must name
the exact plan/preview identity and current project hash; discard releases the private candidate.
Studio receives bounded typed previews, never a callback or executable recipe.

## Owner path

On open, the adapter uses the bounded `ProjectStore` and Loading Bay project decoder, then composes
the public Engine owners:

1. `content-store` admits the exact project body against a hash- and length-bearing manifest.
2. Loading Bay admits every scene and its game-specific relationships into concrete Rust state.
3. `asset-catalog`, `voxel-asset`, and `voxel-annotation` validate canonical authored assets.
4. `authored-scene`, `entity-state`, and `engine-spatial` own scene, entity, voxel, history,
   collision, navigation, and mesh authority.
5. `voxel-convert` owns bounded mesh inspection, material policy, planning, preview, and exact output.
6. `environment-authoring` owns deterministic preset/seed generation; Loading Bay atomically maps
   the resulting voxel asset, instance, and markers into its named project entities.
7. `engine-inspector` and `render-projection` produce owner readouts and the shared renderer frame.

The response includes canonical owner codecs as strings so Studio can display or retain them without
reimplementing their semantics. TypeScript performs closed structural decoding and delegates frames
to `@rusty-engine/render-contracts`; Rust remains the semantic validator and mutation authority.

## Mutation, host files, and persistence

Every project mutation requires the exact project hash it observed, plus the narrower scene, asset,
voxel-data, layer, plan, or preview identity relevant to that owner. The adapter builds and admits a
complete candidate, prepares the renderer projection, authorizes the content write, and only then
publishes canonical bytes atomically. Project-owned replacement writers serialize on the target
file, recheck the expected hash while holding that lock, sync the complete candidate, and treat the
atomic rename as the commit point. Semantic admission, path revalidation, and content confirmation
all finish before that rename, so a response cannot reject after changing project bytes. A later
explicit read or fresh adapter process rereads the canonical committed document.

Project-relative paths stay within the selected project root. Explicit host-file operations require
absolute, lexically normalized paths with no symlink in the existing chain and enforce bounded
reads. Host-file replacement requires the exact prior SHA-256, stages and syncs a same-directory
candidate, rechecks the target, and atomically promotes it. Invalid, stale, oversized, or ambiguous
requests preserve project and target bytes.

## Verification

Focused protocol and voxel-owner proof:

```bash
cargo test --locked -p loading-bay-game --test studio_adapter --test studio_voxel_authoring
```

The complete downstream gate is `pnpm run verify`; it also checks exact Engine resolution, boundary
isolation, the full Rust suite and Clippy, TypeScript content/presentation, and real Chromium/WebGL.
Rusty Engine's Studio integration command takes this checkout as an explicit argument. Neither
repository has an ordinary sibling-checkout dependency.
