mod agent;
pub mod config;
pub mod error;
pub mod paths;
pub mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
