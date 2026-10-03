fn main() {
    // Declares the app's own command so a capability can grant it to the
    // remote page ("allow-show-notification" in capabilities/default.json).
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&["show_notification", "reveal_download"])),
    )
    .expect("failed to run tauri-build");
}
