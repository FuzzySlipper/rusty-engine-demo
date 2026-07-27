# Loading Bay Studio adapter

The `studio-adapter` binary is Loading Bay's project-owned Rust composition boundary for Rusty
Engine Studio. It is not a gameplay runtime facade and it does not generalize Loading Bay concepts
into Engine vocabulary.

Protocol version 7 is a closed, named operation set. It covers project
create/open/save-as/read/close; typed scene, hierarchy, transform, appearance, collision, and
kinematic authoring; deterministic source-mesh import/reimport with catalog dependencies and locks;
material and palette authoring; canonical voxel asset and transformed-instance lifecycle;
authoritative picking; brush and primitive edits; deterministic templates; durable history
query/preview/apply; every typed annotation edit/query/export family; bounded model, GLB, static
voxel-object, and animated flipbook conversion planning; trusted host voxel/mesh/GLB/license files;
and deterministic environment materialization. Requests and responses are one bounded JSON value
per line. There is no method-name dispatch, provider registry, arbitrary command payload, callback
subscription, HTTP route, or browser persistence seam.

Prepared source imports, voxel and voxel-object conversion plans, and history reverts remain private
adapter state. Preparing a second candidate replaces the first. A later apply must name the exact
plan, output, source, and current project identities; discard releases the private candidate. A
rejected preview or apply preserves both project bytes and the candidate so the caller can correct
its request. Studio receives bounded typed previews, diagnostics, complete shared-renderer frames,
and generated-artifact identities, never a callback or executable recipe.

## Owner path

On open, the adapter uses the bounded `ProjectStore` and Loading Bay project decoder, then composes
the public Engine owners:

1. `content-store` admits the exact project body against a hash- and length-bearing manifest.
2. Loading Bay admits every scene and its game-specific relationships into concrete Rust state.
3. `asset-import`, `asset-catalog`, `voxel-asset`, and `voxel-annotation` validate canonical authored
   assets, import provenance, dependency graphs, and generated locks.
4. `authored-scene`, `entity-state`, and `engine-spatial` own scene, entity, voxel, history,
   collision, navigation, and mesh authority.
5. `voxel-convert` owns bounded static and animated mesh inspection, material policy, object and
   flipbook planning, frame preview, source fingerprints, and exact output.
6. `environment-authoring` owns deterministic preset/seed generation; Loading Bay atomically maps
   the resulting voxel asset, instance, and markers into its named project entities.
7. `voxel-object-runtime` admits persisted object frames and clip selection.
8. `engine-inspector` and `render-projection` produce owner readouts and the complete shared
   renderer frame. Candidate and attached objects use Engine `defineVoxelObject` and
   `createVoxelObjectInstance` operations rather than a downstream preview renderer.

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

Imported static meshes retain their canonical `render-model` payload, catalog metadata, manifest,
sidecar, source fingerprint, and generated asset identities in the project document. Readout reports
source drift without mutating content. Reimport replaces only the prior generated identity set and
then runs complete project admission, so unrelated collisions, removed dependencies, stale plans,
and invalid renderer payloads fail atomically.

Voxel objects persist the Engine-owned asset, default frame, clips and per-frame timing, palette,
source-material mapping, exact conversion provenance, and game-owned scene instances. Instances
select the default frame or a named clip frame and retain transform plus bounded material overrides.
Before any object runtime admission or complete projection, Loading Bay counts objects, frames,
sparse-run cells, worst-case mesh faces, and instances, treating a same-identity private candidate
as a replacement. The project bounds are 256 objects, 8,193 frames, 65,536 resolved cells, 393,216
worst-case faces, and 4,096 instances; excess is rejected with
`project.voxelObjectAggregateLimit`. The checked preflight applies equally to open, read, prepare,
preview, apply, and attachment publication, so the later 32 MiB response bound is not the first
limit on object expansion work.
Opening the project in a fresh adapter process reconstructs the same typed authoring readout and
complete renderer frame from those canonical bytes. Schema-19 documents migrate only when the new
object fields are absent; relabeling schema-20 object content as an older schema fails closed.

## Verification

Focused protocol and voxel-owner proof:

```bash
cargo test --locked -p loading-bay-game \
  --test studio_adapter \
  --test studio_voxel_authoring \
  --test studio_voxel_objects
```

The complete downstream gate is `pnpm run verify`; it also checks exact Engine resolution, boundary
isolation, the full Rust suite and Clippy, TypeScript content/presentation, and real Chromium/WebGL.
Rusty Engine's Studio integration command takes this checkout as an explicit argument. Neither
repository has an ordinary sibling-checkout dependency.
