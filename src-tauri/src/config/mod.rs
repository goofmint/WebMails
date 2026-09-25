//! Load, validate and (on first launch) create `config.toml`
//! (design.md §2.2.1, SPEC.md §6).
//!
//! Every key in the file is required. A missing or invalid key fails with
//! a [`ConfigError`] that names the file, the key and the reason; nothing
//! is ever filled in from a default (design.md §2.2.1, §10, §5.1).
//!
//! `apply` — atomic, comment-preserving edits with `toml_edit` — is Task
//! 1.3 (design.md §2.2.1) and is not implemented here.

mod model;
mod validate;

pub use model::{Config, ConfigError, IconSource, ProfileName, ServiceConfig, ServiceId, Settings};
pub use validate::parse;

use std::io;
use std::path::Path;

fn io_error(file: &Path, err: io::Error) -> ConfigError {
    ConfigError {
        file: file.to_path_buf(),
        key: None,
        reason: err.to_string(),
    }
}

/// Reads and validates the configuration file at `path`.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|err| io_error(path, err))?;
    parse(&text, path)
}

/// Writes a complete configuration file for first launch: every settings
/// key set explicitly to the value documented in SPEC.md §6, and no
/// services (see [`Config::initial`]). Creates the parent directory if
/// needed.
///
/// The content is written and synced to a temporary file in the same
/// directory first, then published with `hard_link`, which fails if a file
/// already exists at `path`. So an existing file is never overwritten, and
/// a failed write never leaves a partial `config.toml` behind.
pub fn write_initial(path: &Path) -> Result<Config, ConfigError> {
    let config = Config::initial();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| io_error(path, err))?;
    }

    let text = toml::to_string(&config).map_err(|err| ConfigError {
        file: path.to_path_buf(),
        key: None,
        reason: err.to_string(),
    })?;

    let tmp = temp_path(path)?;
    let published = std::fs::File::create(&tmp)
        .and_then(|mut file| {
            use std::io::Write as _;
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::hard_link(&tmp, path));
    let cleanup = std::fs::remove_file(&tmp);
    published.map_err(|err| io_error(path, err))?;
    cleanup.map_err(|err| io_error(&tmp, err))?;

    Ok(config)
}

/// Temporary file used by [`write_initial`]: same directory as `path`, so
/// publishing it is a same-filesystem link.
fn temp_path(path: &Path) -> Result<std::path::PathBuf, ConfigError> {
    let Some(file_name) = path.file_name() else {
        return Err(ConfigError {
            file: path.to_path_buf(),
            key: None,
            reason: "config path has no file name".to_string(),
        });
    };
    let mut name = file_name.to_os_string();
    name.push(".init.tmp");
    Ok(path.with_file_name(name))
}

/// Loads the configuration at `path`, writing the initial file first if
/// none exists yet. Any error other than the file being absent — a
/// permission error, a malformed existing file — is returned as-is; an
/// invalid existing file is never repaired or overwritten.
pub fn load_or_init(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text, path),
        Err(err) if err.kind() == io::ErrorKind::NotFound => write_initial(path),
        Err(err) => Err(io_error(path, err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn config_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("config.toml")
    }

    #[test]
    fn write_initial_then_load_round_trips_to_config_initial() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);

        let written = write_initial(&path).expect("write_initial should succeed");
        assert_eq!(written, Config::initial());

        let loaded = load(&path).expect("load should succeed");
        assert_eq!(loaded, Config::initial());
    }

    #[test]
    fn write_initial_creates_the_parent_directory() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("nested").join("config.toml");

        write_initial(&path).expect("write_initial should create the parent directory");
        assert!(path.exists());
    }

    #[test]
    fn write_initial_never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);
        fs::write(&path, "not touched").expect("seed an existing file");

        let err =
            write_initial(&path).expect_err("write_initial must not overwrite an existing file");
        assert_eq!(err.key, None);
        assert_eq!(fs::read_to_string(&path).expect("read back"), "not touched");
    }

    #[test]
    fn write_initial_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);

        write_initial(&path).expect("write_initial should succeed");
        let names: Vec<_> = fs::read_dir(dir.path())
            .expect("list dir")
            .map(|entry| entry.expect("dir entry").file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("config.toml")]);
    }

    #[test]
    fn write_initial_on_an_existing_file_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);
        fs::write(&path, "not touched").expect("seed an existing file");

        write_initial(&path).expect_err("write_initial must not overwrite an existing file");
        assert!(!dir.path().join("config.toml.init.tmp").exists());
    }

    #[test]
    fn load_fails_when_the_file_is_missing() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);

        let err = load(&path).expect_err("load should fail when the file does not exist");
        assert_eq!(err.file, path);
    }

    #[test]
    fn load_or_init_creates_the_file_only_when_missing() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);

        let created = load_or_init(&path).expect("load_or_init should create the file");
        assert_eq!(created, Config::initial());
        assert!(path.exists());

        let loaded_again =
            load_or_init(&path).expect("load_or_init should load the now-existing file");
        assert_eq!(loaded_again, Config::initial());
    }

    #[test]
    fn load_or_init_returns_the_error_for_an_invalid_existing_file_unchanged() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = config_path(&dir);
        fs::write(&path, "version = 1\n").expect("seed an invalid config file");

        let err = load_or_init(&path).expect_err("load_or_init must not repair an invalid file");
        assert_eq!(err.key.as_deref(), Some("settings"));
        assert_eq!(
            fs::read_to_string(&path).expect("read back"),
            "version = 1\n"
        );
    }
}
