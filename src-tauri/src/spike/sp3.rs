//! SP3 — remote IPC capability spike (tasks.md Task 0.5, design §6.3 SP3,
//! design §2.2.6 `agent_bridge`).
//!
//! Declares `report_unread` (via `build.rs`'s `AppManifest::commands`),
//! grants it at runtime to exactly one webview-label + remote-origin
//! combination via `CapabilityBuilder`, and creates three debug-only
//! children to probe the combinations design's Task 0.5 / SP3 spike asks
//! for:
//!
//! - `svc-allowed` at the service origin — capability covers both label
//!   and origin → invoke should **succeed**.
//! - `svc-denied` at the same service origin, but a different label that
//!   the capability never names → invoke should be **denied** (label
//!   mismatch), even though the origin matches.
//! - `svc-allowed-other-origin` — its label *is* included in the same
//!   capability as `svc-allowed`, but it loads a different origin the
//!   capability's `.remote()` pattern does not match → invoke should
//!   still be **denied** (origin mismatch), showing that label alone is
//!   not enough.
//!
//! `report_unread` itself and its `build.rs` declaration are **not**
//! gated behind `cfg(debug_assertions)` (design §2.2.6 declares it
//! unconditionally); only the three verification windows are, per the
//! CodeRabbit SP3 plan ("検証用ウィンドウはデバッグビルドだけに含めます").

use tauri::ipc::CapabilityBuilder;
use tauri::{App, Manager, WebviewUrl};

use super::common::{self, ChildInfo, HostState};

const LABEL_ALLOWED: &str = "svc-allowed";
const LABEL_DENIED: &str = "svc-denied";
const LABEL_ALLOWED_OTHER_ORIGIN: &str = "svc-allowed-other-origin";

/// The real webmail origin under test. Overridable so the owner can point
/// this at whatever service they are validating against without editing
/// code; the default is a plausible webmail-shaped origin for a run with
/// no override set, not a claim that a specific account works there.
fn service_origin() -> String {
    std::env::var("ELUMA_SPIKE_SP3_SERVICE_ORIGIN")
        .unwrap_or_else(|_| "https://mail.google.com".to_string())
}

/// A second, unrelated origin used only to prove the capability's
/// `.remote()` pattern — not just the webview label — is enforced.
fn other_origin() -> String {
    std::env::var("ELUMA_SPIKE_SP3_OTHER_ORIGIN")
        .unwrap_or_else(|_| "https://example.com".to_string())
}

/// `report_unread(count)` — design §2.2.6's command, simplified for the
/// spike to a single `count: u32` in place of the full `UnreadReportDto`.
/// Logs the calling webview's label and URL plus the count (confirming
/// the `tauri::Webview` parameter yields the calling label — the third
/// bullet of tasks.md Task 0.5), and returns the label so the caller can
/// see it came back correctly.
#[tauri::command]
pub fn report_unread(webview: tauri::Webview, count: u32) -> String {
    let url = webview
        .url()
        .map(|u| u.to_string())
        .unwrap_or_else(|err| format!("<unavailable: {err}>"));
    println!(
        "[spike-sp3] report_unread called: label={} url={url} count={count}",
        webview.label()
    );
    webview.label().to_string()
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(debug_assertions) {
        return Err(
            "ELUMA_SPIKE=sp3 creates debug-only verification windows (tasks.md Task 0.5); \
             build without --release to run it"
                .into(),
        );
    }

    let service_origin = service_origin();
    let other_origin = other_origin();

    let window = tauri::window::WindowBuilder::new(app, "main")
        .title("Eluma spike — sp3 (remote IPC capability)")
        .inner_size(1200.0, 800.0)
        .build()?;

    // The one runtime capability under test: `allow-report-unread`,
    // scoped to the service origin and to exactly the two "allowed"
    // labels. `svc-denied` is deliberately never named here.
    app.add_capability(
        CapabilityBuilder::new("spike-svc-allowed")
            .remote(format!("{service_origin}/*"))
            .webview(LABEL_ALLOWED)
            .webview(LABEL_ALLOWED_OTHER_ORIGIN)
            .permission("allow-report-unread"),
    )?;

    println!(
        "[spike-sp3] runtime capability added: remote={service_origin}/* webviews=[{LABEL_ALLOWED}, {LABEL_ALLOWED_OTHER_ORIGIN}] permission=allow-report-unread"
    );
    println!("[spike-sp3] {LABEL_DENIED} gets NO capability at all (label-mismatch case)");
    println!(
        "[spike-sp3] {LABEL_ALLOWED_OTHER_ORIGIN} loads {other_origin} — label matches the capability, origin does not (origin-mismatch case)"
    );

    let specs = [
        (
            LABEL_ALLOWED,
            service_origin.clone(),
            "allowed label, service origin -- expect invoke to SUCCEED",
        ),
        (
            LABEL_DENIED,
            service_origin.clone(),
            "different label, service origin -- expect invoke DENIED (label mismatch)",
        ),
        (
            LABEL_ALLOWED_OTHER_ORIGIN,
            other_origin.clone(),
            "allowed-label pattern, different origin -- expect invoke DENIED (origin mismatch)",
        ),
    ];

    let mut children_info = Vec::with_capacity(specs.len());
    let mut services = Vec::with_capacity(specs.len());
    for (i, (label, url, note)) in specs.iter().enumerate() {
        // The page title doubles as an on-screen readout of which case
        // this window is, for the owner's devtools-based manual check.
        let init = format!(
            "console.log({msg}); document.title = {label_js};",
            msg = common::js_string(&format!("[spike-sp3] {label}: {note}")),
            label_js = common::js_string(label),
        );
        let (pos, size) = common::geometry(&window, i, 0)?;
        let webview = window.add_child(
            common::base_child_builder(label, WebviewUrl::External(url.parse()?))
                .initialization_script(init),
            pos,
            size,
        )?;
        children_info.push(ChildInfo {
            label: (*label).to_string(),
            display: (*label).to_string(),
            url: url.clone(),
        });
        services.push(webview);
    }

    let window_size = common::logical_window_size(&window)?;
    let shell_init = common::shell_init_script("sp3", &children_info, 0);
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
        "[spike-sp3] from each webview's devtools console, run: \
         window.__TAURI_INTERNALS__.invoke('report_unread', {{ count: 3 }})"
    );

    Ok(())
}
