use std::time::Instant;

use loading_bay_game::{GameRuntime, VoxelEdit, VoxelEditTransaction, VoxelSourceRevision};
use serde_json::json;

const PROJECT: &str = include_str!("../../../../../content/projects/loading-bay.project.json");
const EDIT_VOXEL: [i64; 3] = [4, 1, 6];
const DEFAULT_TRANSACTIONS: usize = 256;
const MAX_TRANSACTIONS: usize = 4_096;

fn main() {
    let transactions = transaction_count();
    let mut runtime = GameRuntime::from_stored_project(PROJECT).expect("admit workload project");
    let baseline_hash = runtime
        .collision_scene()
        .expect("workload collision scene")
        .authority_hash();
    let mut total_nanos = 0u128;
    let mut maximum_nanos = 0u128;
    let mut peak_mesh_payload_bytes = 0usize;

    for index in 0..transactions {
        let edit = if index.is_multiple_of(2) {
            VoxelEdit::Clear {
                address: EDIT_VOXEL,
            }
        } else {
            VoxelEdit::Set {
                address: EDIT_VOXEL,
                material_slot: 3,
            }
        };
        let started = Instant::now();
        runtime
            .apply_voxel_edits(VoxelEditTransaction {
                expected_revision: VoxelSourceRevision::new(index as u64),
                edits: &[edit],
            })
            .expect("bounded edit rebuild");
        let elapsed = started.elapsed().as_nanos();
        total_nanos += elapsed;
        maximum_nanos = maximum_nanos.max(elapsed);
        peak_mesh_payload_bytes = peak_mesh_payload_bytes.max(mesh_payload_bytes(
            runtime.collision_scene().expect("rebuilt collision scene"),
        ));
    }

    let scene = runtime.collision_scene().expect("final collision scene");
    let final_matches_baseline =
        transactions.is_multiple_of(2) && scene.authority_hash() == baseline_hash;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "workload": "bounded-full-voxel-projection-rebuild",
            "transactions": transactions,
            "editsPerTransaction": 1,
            "acceptedRevision": scene.source_revision().raw(),
            "solidVoxels": scene.solid_voxel_count(),
            "residentChunks": scene.resident_chunk_count(),
            "meshChunks": scene.mesh_chunks().len(),
            "retainedMeshPayloadBytes": mesh_payload_bytes(scene),
            "peakMeshPayloadBytes": peak_mesh_payload_bytes,
            "totalMicros": total_nanos / 1_000,
            "averageMicrosPerTransaction": total_nanos as f64 / transactions as f64 / 1_000.0,
            "maximumMicrosPerTransaction": maximum_nanos / 1_000,
            "rebuildsPerSecond": transactions as f64 / (total_nanos as f64 / 1_000_000_000.0),
            "finalMatchesBaseline": final_matches_baseline,
        }))
        .expect("encode workload receipt")
    );
    assert!(
        final_matches_baseline,
        "even toggle workload must restore authority"
    );
}

fn transaction_count() -> usize {
    let mut args = std::env::args().skip(1);
    let Some(value) = args.next() else {
        return DEFAULT_TRANSACTIONS;
    };
    assert!(
        args.next().is_none(),
        "usage: voxel-edit-workload [transactions]"
    );
    let transactions: usize = value.parse().expect("transactions must be an integer");
    assert!(
        (2..=MAX_TRANSACTIONS).contains(&transactions) && transactions.is_multiple_of(2),
        "transactions must be an even value in 2..={MAX_TRANSACTIONS}"
    );
    transactions
}

fn mesh_payload_bytes(scene: &engine_spatial::VoxelCollisionScene) -> usize {
    scene
        .mesh_chunks()
        .iter()
        .map(|mesh| {
            (mesh.positions.len() + mesh.normals.len()) * size_of::<f32>()
                + mesh.indices.len() * size_of::<u32>()
                + mesh.groups.len() * size_of::<engine_spatial::VoxelMeshGroup>()
        })
        .sum()
}
