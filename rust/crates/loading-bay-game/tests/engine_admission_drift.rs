use loading_bay_game::{diagnostic_code, GameRuntime, RuntimeError};
use serde_json::{json, Value};

const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");

fn mutated_project(mutate: impl FnOnce(&mut Value)) -> Value {
    let mut project: Value = serde_json::from_str(PROJECT).unwrap();
    mutate(&mut project);
    project
}

/// Entity bounds are Engine-owned invariants: the downstream validator no
/// longer restates finite/limit/ordering rules, so an inverted bound must
/// fail admission through the typed Engine rejection translated onto the
/// product entity path — atomically, with no partial publish.
#[test]
fn inverted_entity_bounds_fail_through_engine_admission_with_product_path() {
    let project = mutated_project(|project| {
        let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
        let pickup = entities
            .iter_mut()
            .find(|entity| entity.get("pickup").is_some())
            .expect("E1M1 authors pickups with bounds");
        pickup["bounds"]["min"] = json!([2.0, 2.0, 2.0]);
        pickup["bounds"]["max"] = json!([1.0, 1.0, 1.0]);
    });
    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&project.to_string()).unwrap_err()
    else {
        panic!("inverted bounds must fail admission");
    };
    assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_COMPONENT);
    assert!(
        error.diagnostic().path.ends_with(".bounds"),
        "Engine rejection must translate onto the product bounds path, got {}",
        error.diagnostic().path
    );
}

/// Out-of-range bounds fail the same way without any downstream numeric
/// restatement of the spatial limit.
#[test]
fn out_of_range_entity_bounds_fail_through_engine_admission() {
    let project = mutated_project(|project| {
        let entities = project["scenes"][0]["entities"].as_array_mut().unwrap();
        let pickup = entities
            .iter_mut()
            .find(|entity| entity.get("pickup").is_some())
            .expect("E1M1 authors pickups with bounds");
        pickup["bounds"]["min"] = json!([-2_000_000.0, -1.0, -1.0]);
        pickup["bounds"]["max"] = json!([2_000_000.0, 1.0, 1.0]);
    });
    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&project.to_string()).unwrap_err()
    else {
        panic!("out-of-range bounds must fail admission");
    };
    assert_eq!(error.diagnostic().code, diagnostic_code::INVALID_COMPONENT);
    assert!(error.diagnostic().path.ends_with(".bounds"));
}

/// Voxel-instance quaternion tolerance follows the Engine scene-admission
/// rule. A rotation within the old downstream 0.002 drift but outside the
/// Engine 0.001 tolerance is now rejected instead of silently admitted.
#[test]
fn voxel_instance_quaternion_tolerance_follows_engine_rule() {
    // norm^2 == 1.00075^2 ~= 1.0015006: inside the retired downstream
    // tolerance (0.002), outside the Engine tolerance (0.001).
    let drifted = mutated_project(|project| {
        project["scenes"][0]["voxelInstances"][0]["rotation"] = json!([0.0, 0.0, 0.0, 1.00075]);
    });
    let RuntimeError::StoredProject(error) =
        GameRuntime::from_stored_project(&drifted.to_string()).unwrap_err()
    else {
        panic!("a quaternion outside the Engine unit tolerance must fail admission");
    };
    assert_eq!(
        error.diagnostic().code,
        diagnostic_code::INVALID_VOXEL_INSTANCE
    );
    assert!(
        error.diagnostic().message.contains("normalized quaternion"),
        "Engine rejection should name the unit-quaternion rule, got {}",
        error.diagnostic().message
    );

    // A rotation inside the Engine tolerance still admits (norm^2 deviation
    // 1.0004^2 - 1 ~= 0.0008 < 0.001).
    let acceptable = mutated_project(|project| {
        project["scenes"][0]["voxelInstances"][0]["rotation"] = json!([0.0, 0.0, 0.0, 1.0004]);
    });
    assert!(GameRuntime::from_stored_project(&acceptable.to_string()).is_ok());
}

/// The canonical project keeps admitting: the probe route never rejects
/// well-formed content.
#[test]
fn canonical_project_still_admits() {
    assert!(GameRuntime::from_stored_project(PROJECT).is_ok());
}
