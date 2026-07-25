# Loading Bay Studio adapter

The `studio-adapter` binary is Loading Bay's project-owned Rust composition boundary for Rusty
Engine Studio. It is not a gameplay runtime facade and it does not generalize Loading Bay concepts
into Engine vocabulary.

Protocol version 2 has exactly five request families:

- `describe` identifies the adapter and supported project schema;
- `openProject` selects one explicit absolute root and safe relative project file;
- `readProject` rereads the open project through the same owner path;
- `setEntityTranslation` performs one typed authored transform operation; and
- `closeProject` discards the adapter's open-project state.

Requests and responses are one bounded JSON value per line. There is no method-name dispatch,
provider registry, arbitrary command payload, callback subscription, HTTP route, or browser
persistence seam.

## Owner path

On open, the adapter uses the existing bounded `ProjectStore` and Loading Bay project decoder, then
composes the public Engine owners:

1. `content-store` admits the exact project body against a hash- and length-bearing manifest.
2. Loading Bay admits every scene and its game-specific relationships into concrete Rust state.
3. `asset-catalog` validates the derived canonical asset authority.
4. `authored-scene` validates and admits the derived entry-scene view through `entity-state`.
5. `engine-inspector` produces catalog, scene, entity, persistence, and voxel readouts.
6. `render-projection` produces entity instances, which the adapter composes with explicit
   catalog-derived material and mesh definitions into a complete public `render-model` frame.

The response includes canonical owner codecs as strings so Studio can display or retain them without
reimplementing their semantics. The TypeScript boundary performs structural decoding and delegates
the frame to `@rusty-engine/render-contracts`; Rust remains the only semantic validator. A separate
`sceneHierarchy` readout gives Studio the validated `authored-scene` traversal order, node/entity
mapping, and local/world transforms without requiring a TypeScript scene codec. Every response
contains a complete, atomically replaceable projection frame so renderer resources and instances
cannot drift across refresh or reopen.

## Mutation and persistence

`setEntityTranslation` requires both the exact project content hash and the derived authored-scene
revision observed by Studio. The adapter stages the `authored-scene` edit, reruns complete Loading
Bay admission, builds and authorizes a `content-store` write candidate, and stages renderer
projection before touching disk. The admitted project is then written to a same-directory pending
file, source identity is checked again, and an atomic rename installs the canonical bytes. A
canonical reread and publication confirmation complete the operation.

Traversal, non-absolute roots, oversized input, non-files, and symlinks anywhere in the writable
project path are rejected. Invalid candidates, stale identities, and malformed protocol values do
not alter the project file.

## Verification

The focused Rust proof is:

```bash
cargo test --locked -p loading-bay-game --test studio_adapter --test project_store
```

Rusty Engine owns an explicit integration command that builds this binary, opens the real checkout
through the TypeScript editor store, validates owner, hierarchy, projection, and voxel readouts,
and performs a canonical reread. That command takes the checkout as an explicit argument; neither
repository gains an ordinary sibling-checkout dependency.
