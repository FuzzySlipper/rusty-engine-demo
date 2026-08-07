use voxel_asset::decode_voxel_asset;

#[test]
fn doom_e1m1_voxel_asset_decodes_without_mutation() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest_dir}/../../../content/doom-e1m1/doom-e1m1.voxel.json");
    let input = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let asset = decode_voxel_asset(&input).expect("decode_voxel_asset failed — run cargo run -p loading-bay-game --bin doom-voxel-hash -- content/doom-e1m1/doom-e1m1.voxel.json after regenerating via pnpm --filter doom-e1m1-authoring exec node dist/voxelize.js");
    assert_eq!(asset.asset_id, "voxel-volume/doom-e1m1");
    assert_eq!(asset.schema_version, 1);
    assert!(asset.representation.sparse_runs.len() > 1000);
    // E1M1 should have 54 material slots (22 flats + 32 walls) per R6676
    assert_eq!(asset.material_palette.len(), 54);
    assert_eq!(asset.material_map.len(), 54);
    // Budgets
    let voxel_count: usize = asset
        .representation
        .sparse_runs
        .iter()
        .map(|r| r.length as usize)
        .sum();
    assert!(
        voxel_count < 1_000_000,
        "voxel count {voxel_count} exceeds 1M"
    );
    // Bounds should match 16-scale Hangar extents
    assert_eq!(asset.grid.cell_size, 1.0);
    assert_eq!(asset.bounds.min, [0, 0, 0]);
}

#[test]
fn doom_e1m1_voxel_hash_is_stable() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest_dir}/../../../content/doom-e1m1/doom-e1m1.voxel.json");
    let input = std::fs::read_to_string(&path).expect("read");
    let first = decode_voxel_asset(&input).expect("decode");
    let encoded = voxel_asset::encode_voxel_asset(&first).expect("encode");
    let second = decode_voxel_asset(&encoded).expect("re-decode");
    assert_eq!(first.voxel_data_hash, second.voxel_data_hash);
    assert_eq!(first.content_hash, second.content_hash);
}
