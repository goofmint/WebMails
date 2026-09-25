//! SP1 — multiwebview host spike (tasks.md Task 0.3, design §6.3 SP1).
//!
//! One `shell` child plus 4 local test service children (`svc-0`..`svc-3`,
//! `public/spike/service.html`). One slot can be pointed at a real webmail
//! URL via `ELUMA_SPIKE_SP1_URL` (see `SPIKE.md`). Setting
//! `ELUMA_SPIKE_AUTO_RESIZE=1` switches every service child to
//! `WebviewBuilder::auto_resize()` instead of the manual `set_size` calls
//! `common::relayout` otherwise makes on resize, for the S8 comparison in
//! `docs/spikes/SP1.md` — comparison only, no other mode uses this.

use tauri::{App, Manager, WebviewUrl};

use super::common::{self, ChildInfo, HostState};

const LABELS: [&str; 4] = ["svc-0", "svc-1", "svc-2", "svc-3"];

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let window = tauri::window::WindowBuilder::new(app, "main")
        .title("Eluma spike — sp1 (multiwebview)")
        .inner_size(1000.0, 700.0)
        .build()?;

    // One slot can be pointed at a real webmail URL for a closer-to-real
    // check; the rest stay on the local test page (design's Task 0.3:
    // "one service child slot switchable to a real webmail URL").
    let override_url = std::env::var("ELUMA_SPIKE_SP1_URL").ok();
    let override_index = LABELS.len() - 1;
    // S8 comparison only (docs/spikes/SP1.md) — every other mode, and the
    // default sp1 run, uses manual relayout via `common::geometry`.
    let auto_resize = std::env::var("ELUMA_SPIKE_AUTO_RESIZE").as_deref() == Ok("1");

    let mut children_info = Vec::with_capacity(LABELS.len());
    let mut services = Vec::with_capacity(LABELS.len());
    for (i, label) in LABELS.iter().enumerate() {
        let (url, display) = if i == override_index {
            match &override_url {
                Some(real_url) => (WebviewUrl::External(real_url.parse()?), real_url.clone()),
                None => (
                    WebviewUrl::App("spike/service.html".into()),
                    format!("spike/service.html (n={i})"),
                ),
            }
        } else {
            (
                WebviewUrl::App("spike/service.html".into()),
                format!("spike/service.html (n={i})"),
            )
        };

        let init = format!(
            "window.__ELUMA_SPIKE_SERVICE__ = {{\"index\":{i},\"label\":{label}}};",
            label = common::js_string(label),
        );
        let mut builder = common::base_child_builder(label, url).initialization_script(init);
        if auto_resize {
            builder = builder.auto_resize();
        }
        let (pos, size) = common::geometry(&window, i, 0)?;
        let webview = window.add_child(builder, pos, size)?;

        children_info.push(ChildInfo {
            label: (*label).to_string(),
            display: (*label).to_string(),
            url: display,
        });
        services.push(webview);
    }

    let window_size = common::logical_window_size(&window)?;
    let shell_init = common::shell_init_script("sp1", &children_info, 0);
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

    println!(
        "[spike-sp1] shell + {} service children ready (labels: {LABELS:?}); \
         ELUMA_SPIKE_SP1_URL={override_url:?} ELUMA_SPIKE_AUTO_RESIZE={auto_resize}",
        LABELS.len(),
    );

    Ok(())
}
