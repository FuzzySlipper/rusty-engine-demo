use std::fs;
use std::path::PathBuf;

use voxel_asset::{decode_voxel_asset, encode_voxel_asset, with_computed_content_hash};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: doom-voxel-hash <path-to-voxel-json>");
    let path_buf = PathBuf::from(path);
    let input = fs::read_to_string(&path_buf).unwrap_or_else(|e| panic!("read {}: {e}", path_buf.display()));
    let asset = decode_voxel_asset(&input).unwrap_or_else(|e| {
        // If decode fails due to hash mismatch, try with_computed_content_hash path via tolerant parse:
        // First try to parse without validation by using serde directly, then compute.
        // For now, print diagnostics and fallback.
        eprintln!("decode failed (expected for placeholder hashes): {e}");
        // Attempt to deserialize with serde directly, ignoring hash validation via intermediate
        let mut raw: serde_json::Value = serde_json::from_str(&input).expect("json parse");
        // Clear hashes to allow with_computed_content_hash to succeed via manual construction?
        // Instead, construct via serde then call with_computed_content_hash which will validate semantic before hash check.
        // Use serde to get VoxelAsset with placeholder hashes, then clear and recompute.
        let mut asset: voxel_asset::VoxelAsset = serde_json::from_value(raw).expect("serde");
        asset.voxel_data_hash.clear();
        asset.content_hash.clear();
        with_computed_content_hash(asset).expect("recompute")
    });
    // Ensure hashes are correct (recompute)
    let fixed = with_computed_content_hash(asset).unwrap_or_else(|e| panic!("recompute failed: {e}"));
    let encoded = encode_voxel_asset(&fixed).expect("encode");
    fs::write(&path_buf, encoded).unwrap_or_else(|e| panic!("write {}: {e}", path_buf.display()));
    println!("fixed {} voxel_data_hash={} content_hash={}", path_buf.display(), fixed.voxel_data_hash, fixed.content_hash);
}
