//! Application config and data directories.
//!
//! Directories are resolved through Tauri's path resolver
//! (`app_config_dir`, `app_data_dir`), never hard-coded, and never
//! created here (design.md §9.2).

use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::config::ServiceId;
use crate::error::{AppError, AppResult};

/// Filename for the application's configuration file, joined onto the
/// directory returned by [`config_dir`].
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Filename for the application's persisted state, joined onto the
/// directory returned by [`data_dir`].
pub const STATE_FILE_NAME: &str = "state.json";

/// Directory name (joined onto [`data_dir`]) that holds every cached PNG
/// and installed file override (design.md §2.2.10, §2.2.12).
pub const ICONS_DIR_NAME: &str = "icons";

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

/// `{data_dir}/icons` (design.md §2.2.10): where every cached icon PNG and
/// installed file override lives.
pub fn icons_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(ICONS_DIR_NAME)
}

/// `{data_dir}/icons/<id>.png` — `id`'s cached, normalised icon (design.md
/// §2.2.10).
pub fn icon_cache_file(data_dir: &Path, id: &ServiceId) -> PathBuf {
    icons_dir(data_dir).join(format!("{id}.png"))
}

/// `icons/<id>.src`, relative to `data_dir` — where a user's file override
/// is installed (design.md §2.2.10, §2.2.12), and the exact value stored in
/// `config.toml`'s `icon = { source = "file", value = ... }` for that
/// override: always this same fixed, already-relative, already-inside-
/// `data_dir` path (satisfying `config::validate`'s "must stay inside
/// `data_dir`, no `..` components" rule by construction).
pub fn icon_override_relative_path(id: &ServiceId) -> PathBuf {
    PathBuf::from(ICONS_DIR_NAME).join(format!("{id}.src"))
}

/// `{data_dir}/icons/<id>.src` — [`icon_override_relative_path`] joined
/// onto `data_dir`, for actually reading/writing the installed file.
pub fn icon_override_file(data_dir: &Path, id: &ServiceId) -> PathBuf {
    data_dir.join(icon_override_relative_path(id))
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

    fn id(value: &str) -> ServiceId {
        ServiceId::new(value).expect("valid id")
    }

    #[test]
    fn icons_dir_joins_onto_data_dir() {
        let dir = Path::new("/tmp/eluma/data");
        assert_eq!(icons_dir(dir), dir.join("icons"));
    }

    #[test]
    fn icon_cache_file_is_id_dot_png_under_icons_dir() {
        let dir = Path::new("/tmp/eluma/data");
        assert_eq!(
            icon_cache_file(dir, &id("gmail")),
            dir.join("icons").join("gmail.png")
        );
    }

    #[test]
    fn icon_override_relative_path_is_id_dot_src_under_icons() {
        assert_eq!(
            icon_override_relative_path(&id("gmail")),
            Path::new("icons").join("gmail.src")
        );
    }

    #[test]
    fn icon_override_file_joins_relative_path_onto_data_dir() {
        let dir = Path::new("/tmp/eluma/data");
        assert_eq!(
            icon_override_file(dir, &id("gmail")),
            dir.join("icons").join("gmail.src")
        );
    }
}
