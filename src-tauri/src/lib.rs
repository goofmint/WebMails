mod agent;
pub mod config;
pub mod error;
pub mod paths;

// SP5 spike harness (docs/spikes/SP5.md, tasks.md Task 4.1). Throwaway:
// only compiled on `spike/sp5-notifications` when an `sp5-*` Cargo feature
// is enabled; never wired into the production app otherwise, and expected
// to be dropped along with this whole branch.
#[cfg(any(
    feature = "sp5-user-notify",
    feature = "sp5-notify-rust",
    feature = "sp5-baseline"
))]
mod spike;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    #[cfg(feature = "sp5-baseline")]
    {
        builder = builder.plugin(tauri_plugin_notification::init());
    }

    #[cfg(any(
        feature = "sp5-user-notify",
        feature = "sp5-notify-rust",
        feature = "sp5-baseline"
    ))]
    {
        builder = builder.setup(|app| {
            spike::setup(app)?;
            Ok(())
        });
    }

    builder
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
