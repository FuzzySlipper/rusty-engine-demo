# Rusty Engine Three renderer

This package is a narrow successor fork of the retained browser renderer in Asha Engine at commit
`a431974330589761c9e35fc4f8a55996a1b5ee48`, principally
`ts/packages/renderer-three/src/{three-renderer,browser-surface}.ts` and the generated render values
those files consumed.

Rusty Engine keeps only its live product border: primitive create/update/destroy operations, inline
mesh replacement, deterministic structural inspection, camera placement, and a real Three/WebGL
canvas. Encoded frames, runtime buffer handles, generic projection helpers, editor/tunnel/static-room
surfaces, picking, animation assets, sprites, catalogs, and renderer-authored authority are omitted
because no Rusty Engine consumer uses them. The package imports the small local
`@rusty-engine-demo/render-contracts` vocabulary and has no Asha runtime or workspace dependency.

The donor repository and package manifests declare no source license. This same-owner transfer does
not invent one; commit/path provenance and the substantive divergences are recorded here and in
`docs/donor-provenance.md`. Registry packages `three` and `@types/three` retain their published MIT
metadata through the pnpm lockfile.
