# Loading Bay weapon authoring contract

Loading Bay owns one closed, headless project-authoring contract for the
provider-derived weapon entities shown by Rusty Engine Studio:

- component type: `rusty-engine-demo.loading-bay.weapon`
- contract: `rusty-engine-demo.loading-bay.weapon-authoring`
- contract version: `1`
- operations: `readLoadingBayWeapon` and `replaceLoadingBayWeapon`

The decision follows the post-GM6 project/runtime boundary reviewed at demo
revision `741812348f1a99a4e13415467c928a2a0dc32a43` and the downstream inspector
extension design approved at Rusty Engine revision
`31181bd445d072db3334a0ed706dd1f4079b2022`.

## Ownership

A selected weapon entity is a deterministic provider-derived identity for one
weapon slot in the entry scene. The editable durable source is the matching
global `StoredItemDefinition::Weapon`. The readout also reports its inventory
owner, slot, starting quantity, and initially-equipped status, but those values
are immutable binding context. Changing inventory composition remains a
separate future authoring decision owned by the inventory entity.

A complete replacement supplies attack mode (including complete spread
settings), damage, range, cadence, ammunition identity and cost, muzzle offset,
and presentation identity. Rust performs complete stored-project admission
before atomically replacing the project under both the expected project hash
and exact component revision. The receipt returns before/after project hashes,
before/after component revisions, and a canonical admitted readout.
The component revision is the full lowercase content hash of the canonical
stored item definition, not a process-local counter.

Live ammunition quantities, current cooldowns, attack resolution, runtime
state, and save/checkpoint data are deliberately absent. This contract is not a
generic component API, reflection schema, JSON Patch surface, or Engine
protocol operation.

## Consumer fixtures

The frozen identity, operation names, field sets, bounds, and representative
wire shapes are in `contracts/loading-bay-weapon-authoring-v1/`. Hash values in
the fixtures are syntactic placeholders; a host must forward the exact hashes
returned by the current read. Rusty Engine can vendor these fixtures for
identity matching and its static host outlet without importing or inspecting
this repository at runtime.
