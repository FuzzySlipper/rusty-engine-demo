pub fn strip_future_gameplay_mechanics_state(snapshot: &mut serde_json::Value) {
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
