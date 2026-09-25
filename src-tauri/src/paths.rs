//! Application config and data directories.
//!
//! Directories are resolved through Tauri's path resolver
//! (`app_config_dir`, `app_data_dir`), never hard-coded, and never
//! created here (design.md §9.2).

use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::error::{AppError, AppResult};

/// Filename for the application's configuration file, joined onto the
/// directory returned by [`config_dir`].
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Filename for the application's persisted state, joined onto the
/// directory returned by [`data_dir`].
pub const STATE_FILE_NAME: &str = "state.json";

/// Resolves the directory Eluma stores its configuration in.
pub fn config_dir<R: tauri::Runtime>(app: &impl Manager<R>) -> AppResult<PathBuf> {
    app.path()
        .app_config_dir()
        .map_err(|err| AppError::Path(format!("app config dir: {err}")))
}

/// Resolves the directory Eluma stores its runtime state and logs in.
pub fn data_dir<R: tauri::Runtime>(app: &impl Manager<R>) -> AppResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|err| AppError::Path(format!("app data dir: {err}")))
}

/// Joins the configuration filename onto a config directory.
pub fn config_file(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE_NAME)
}

/// Joins the state filename onto a data directory.
pub fn state_file(dir: &Path) -> PathBuf {
    dir.join(STATE_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_file_joins_filename_onto_dir() {
        let dir = Path::new("/tmp/eluma/config");
        assert_eq!(config_file(dir), dir.join("config.toml"));
    }

    #[test]
    fn state_file_joins_filename_onto_dir() {
        let dir = Path::new("/tmp/eluma/data");
        assert_eq!(state_file(dir), dir.join("state.json"));
    }
}
