fn main() {
    // Cargo workspace checks must not depend on ignored package outputs. Release
    // builds keep the complete configuration so Tauri still requires the real,
    // hash-recorded sidecar and resource tree prepared by beforeBuildCommand.
    if std::env::var_os("TAURI_CONFIG").is_none()
        && std::env::var("PROFILE").as_deref() != Ok("release")
    {
        std::env::set_var(
            "TAURI_CONFIG",
            r#"{"bundle":{"externalBin":[],"resources":[]}}"#,
        );
    }
    tauri_build::build()
}
