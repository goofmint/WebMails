fn main() {
    // Declare the spike's app commands so tauri-build autogenerates their
    // ACL permissions (`allow-<kebab-command>` / `deny-<kebab-command>`,
    // confirmed by reading tauri-build 2.6.3's
    // `autogenerate_command_permissions` in
    // ~/.cargo/registry/.../tauri-utils-2.9.3/src/acl/build.rs, which does
    // `command.replace('_', '-')` then `format!("allow-{slugified}")`).
    // After building once, the generated permission is visible in
    // `src-tauri/gen/schemas/acl-manifests.json` under `__app-acl__`.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["switch_service", "report_unread", "spike_log"]),
    ))
    .expect("failed to run tauri-build for the spike harness");
}
