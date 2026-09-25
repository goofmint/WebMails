// Throwaway spike harness for tasks 0.3–0.6 (GitHub issues #3–#6). See
// `SPIKE.md` at the worktree root: mode is chosen by the `ELUMA_SPIKE`
// env var, and this branch (`spike/m0-harness`) is never merged to `main`.
mod spike;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            spike::common::switch_service,
            spike::sp3::report_unread,
            spike::sp4::spike_log,
        ])
        .setup(|app| spike::setup(app))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_is_defined() {
        // `run()` starts the Tauri event loop and never returns in a real
        // application, so it cannot be called here. This test only checks
        // that the function exists with the expected signature.
        let _ = run as fn();
    }
}
