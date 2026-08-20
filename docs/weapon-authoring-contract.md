# Loading Bay weapon authoring contract

Loading Bay exposes one closed project-authoring contract for provider-derived weapon entities:

- component: `rusty-engine-demo.loading-bay.weapon`
- contract: `rusty-engine-demo.loading-bay.weapon-authoring`
- version: `1`
- operations: `readLoadingBayWeapon`, `replaceLoadingBayWeapon`

The selected entity identifies one authored weapon slot. The durable editable source is its matching `StoredItemDefinition::Weapon`; inventory ownership and initial equipment are read-only binding context. Rust admits the complete replacement atomically under the observed project hash and exact component revision, then returns canonical readback.

Live ammunition, cooldowns, combat resolution, runtime state, and saves are not authoring inputs. This is not a generic component schema, reflection API, JSON Patch surface, or Engine protocol operation. Frozen field/shape fixtures live in `contracts/loading-bay-weapon-authoring-v1/` and are consumer syntax examples, not current project identities.
