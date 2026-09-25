//! SP4 — WebView2 background throttling spike (tasks.md Task 0.6, design
//! §6.3 SP4, SPEC §17 Q1).
//!
//! One `sp4-page` child loading `public/spike/sp4.html`, a self-contained
//! test page with a 5s heartbeat timer, a second 5s timer that mutates
//! `<title>`, a `MutationObserver` on that mutation, and a
//! `visibilitychange` listener. Every event it logs is also forwarded
//! here via `spike_log`, so the owner can read what happened while the
//! window was minimized/offscreen/occluded without needing DevTools open
//! at the time.
//!
//! On Windows this child (like every other webview in every mode, via
//! `common::base_child_builder`) gets the design §2.2.4 browser-argument
//! constant. Setting `ELUMA_SPIKE_SP4_NO_ARGS=1` skips it for this one
//! child only, for the SP4 "control run, no constant" comparison
//! (`docs/spikes/SP4.md` procedure step 4) — see `SPIKE.md`.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

use tauri::{App, AppHandle, Manager, WebviewUrl};

use super::common::{self, ChildInfo, HostState};

const LABEL: &str = "sp4-page";

/// The append-only log file handle, managed as Tauri state so the
/// `spike_log` command can write to it from any invocation.
pub struct LogFile(Mutex<std::fs::File>);

/// `spike_log(line)` — appends one already-JSON-formatted log line (built
/// by `public/spike/sp4.html`) to stderr and to
/// `{app_data_dir}/logs/spike-sp4.log`. Errors are returned, never
/// swallowed, so a failing write is visible instead of silently losing
/// data (project rule: no fallback handling).
#[tauri::command]
pub fn spike_log(app: AppHandle, line: String) -> Result<(), String> {
    eprintln!("[spike-sp4] {line}");
    let state = app.state::<LogFile>();
    let mut file = state.0.lock().map_err(|err| err.to_string())?;
    writeln!(file, "{line}").map_err(|err| err.to_string())?;
    file.flush().map_err(|err| err.to_string())
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let logs_dir = app.path().app_data_dir()?.join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    let log_path = logs_dir.join("spike-sp4.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    app.manage(LogFile(Mutex::new(file)));
    println!(
        "[spike-sp4] logging to stderr and {} (also mirrored to the page's own localStorage)",
        log_path.display()
    );

    let window = tauri::window::WindowBuilder::new(app, "main")
        .title("Eluma spike — sp4 (WebView2 throttling)")
        .inner_size(900.0, 700.0)
        .build()?;

    // Only this one child can skip the Windows browser-argument constant,
    // and only when explicitly asked to, for the SP4 "control run, no
    // constant" comparison. Every other mode always goes through
    // `common::base_child_builder`, which never skips it.
    let no_args = std::env::var("ELUMA_SPIKE_SP4_NO_ARGS").as_deref() == Ok("1");
    println!(
        "[spike-sp4] ELUMA_SPIKE_SP4_NO_ARGS={no_args} (true = Windows browser-argument constant is NOT applied to {LABEL}, for the control-run comparison)"
    );

    let mut builder =
        tauri::webview::WebviewBuilder::new(LABEL, WebviewUrl::App("spike/sp4.html".into()));
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    }
    #[cfg(windows)]
    {
        if !no_args {
            builder = builder.additional_browser_args(common::WEBVIEW2_ARGS);
        }
    }
    #[cfg(not(windows))]
    {
        // `no_args` only affects the Windows branch above; referencing it
        // here keeps the binding used (and the intent documented) on
        // platforms where the constant never applied in the first place.
        let _ = no_args;
    }

    let (pos, size) = common::geometry(&window, 0, 0)?;
    let webview = window.add_child(builder, pos, size)?;

    let children_info = vec![ChildInfo {
        label: LABEL.to_string(),
        display: LABEL.to_string(),
        url: "spike/sp4.html".to_string(),
    }];
    let window_size = common::logical_window_size(&window)?;
    let shell_init = common::shell_init_script("sp4", &children_info, 0);
    let shell = window.add_child(
        common::base_child_builder("shell", WebviewUrl::App("spike/shell.html".into()))
            .initialization_script(shell_init),
        tauri::LogicalPosition::new(0.0, 0.0),
        tauri::LogicalSize::new(common::SIDEBAR_WIDTH, window_size.height),
    )?;

    app.manage(HostState {
        window: window.clone(),
        shell,
        services: vec![webview],
        active_index: std::sync::Mutex::new(0),
    });
    common::relayout(&app.state::<HostState>())?;
    common::install_relayout_handler(app.handle().clone(), &window);

    Ok(())
}
