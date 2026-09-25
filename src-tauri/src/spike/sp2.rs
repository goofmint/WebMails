//! SP2 — data store isolation spike (tasks.md Task 0.4, design §6.3 SP2,
//! design §2.2.3 `profile`).
//!
//! Two children, both loading `https://www.icloud.com/`, each with its
//! own UUIDv5-derived profile: `data_store_identifier` on macOS,
//! `data_directory` on Windows. `incognito` is never set. Resizing is not
//! handled for this mode's children beyond the shared shell relayout
//! (the SP2 task itself does not exercise resize).

use tauri::{App, Manager, WebviewUrl};

use super::common::{self, ChildInfo, HostState};
use super::data_store;

const ICLOUD_MAIL_URL: &str = "https://www.icloud.com/";
const SLOTS: [(&str, &str); 2] = [("isolated-a", "isolated:a"), ("isolated-b", "isolated:b")];

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let window = tauri::window::WindowBuilder::new(app, "main")
        .title("Eluma spike — sp2 (data store isolation)")
        .inner_size(1200.0, 800.0)
        .build()?;

    let mut children_info = Vec::with_capacity(SLOTS.len());
    let mut services = Vec::with_capacity(SLOTS.len());
    for (i, (label, key)) in SLOTS.iter().enumerate() {
        let uuid = data_store::derive(key);

        let mut builder =
            common::base_child_builder(label, WebviewUrl::External(ICLOUD_MAIL_URL.parse()?));

        #[cfg(target_os = "macos")]
        {
            builder = builder.data_store_identifier(*uuid.as_bytes());
            println!("[spike-sp2] {label} (key={key}) macOS data_store_identifier uuid={uuid}");
        }
        #[cfg(windows)]
        {
            let app_data_dir = app.path().app_data_dir()?;
            let dir = data_store::windows_data_directory(&app_data_dir, uuid);
            println!(
                "[spike-sp2] {label} (key={key}) windows data_directory uuid={uuid} path={}",
                dir.display()
            );
            builder = builder.data_directory(dir);
        }
        // `incognito` is intentionally never called here — an isolated,
        // persistent profile is the entire point of this spike. Eluma
        // targets macOS and Windows only (design §10 / Constraints in
        // docs/spikes/SP2.md), so no other platform branch is provided.

        let (pos, size) = common::geometry(&window, i, 0)?;
        let webview = window.add_child(builder, pos, size)?;

        children_info.push(ChildInfo {
            label: (*label).to_string(),
            display: format!("{label} ({key})"),
            url: ICLOUD_MAIL_URL.to_string(),
        });
        services.push(webview);
    }

    let window_size = common::logical_window_size(&window)?;
    let shell_init = common::shell_init_script("sp2", &children_info, 0);
    let shell = window.add_child(
        common::base_child_builder("shell", WebviewUrl::App("spike/shell.html".into()))
            .initialization_script(shell_init),
        tauri::LogicalPosition::new(0.0, 0.0),
        tauri::LogicalSize::new(common::SIDEBAR_WIDTH, window_size.height),
    )?;

    app.manage(HostState {
        window: window.clone(),
        shell,
        services,
        active_index: std::sync::Mutex::new(0),
    });
    common::relayout(&app.state::<HostState>())?;
    common::install_relayout_handler(app.handle().clone(), &window);

    Ok(())
}
