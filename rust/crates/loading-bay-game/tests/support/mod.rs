pub fn strip_future_gameplay_mechanics_state(snapshot: &mut serde_json::Value) {
    downgrade_player_controller_entities(snapshot);
    snapshot["entities"]["registeredComponents"] = serde_json::json!([]);
    snapshot["entities"]["entities"]
        .as_array_mut()
        .unwrap()
        .retain(|entity| {
            !entity["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("Inventory weapon "))
        });
    for inventory in snapshot["inventories"].as_array_mut().unwrap() {
        inventory.as_object_mut().unwrap().remove("weaponEntities");
    }
}

fn downgrade_player_controller_entities(snapshot: &mut serde_json::Value) {
    let controllers = snapshot["playerControllers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for controller in controllers {
        let entity_id = controller["entity"].as_u64().unwrap();
        let standing_height = controller["canonicalStandingHeight"].as_f64().unwrap();
        let radius = controller["canonicalRadius"].as_f64().unwrap();
        let eye_height = controller["traversal"]["eyeHeight"].as_f64().unwrap();
        let eye_offset = controller["eyeOffsetFromCenter"].as_f64().unwrap();
        let center_lift = eye_height - eye_offset;
        let authored_half_height = standing_height * 0.5 - center_lift;
        let entity = snapshot["entities"]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entity| entity["id"] == entity_id)
            .unwrap();
        if entity["kinematic"].is_null() {
            entity["transform"]["translation"][1] = serde_json::json!(
                entity["transform"]["translation"][1].as_f64().unwrap() - center_lift
            );
            entity["kinematic"] = serde_json::json!({
                "halfExtents": [radius, authored_half_height, radius],
                "velocity": [0.0, 0.0, 0.0]
            });
        }
    }
}
