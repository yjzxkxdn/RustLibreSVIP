//! Port of `libresvip.core.constants`.

use std::path::PathBuf;

pub const PACKAGE_NAME: &str = "libresvip";

// Lyric related constants.
pub const DEFAULT_PHONEME: &str = "la";
pub const DEFAULT_ENGLISH_LYRIC: &str = DEFAULT_PHONEME;
pub const DEFAULT_SPANISH_LYRIC: &str = DEFAULT_ENGLISH_LYRIC;
pub const DEFAULT_CHINESE_LYRIC: &str = "啦";
pub const DEFAULT_JAPANESE_LYRIC: &str = "ラ";
pub const DEFAULT_KOREAN_LYRIC: &str = "라";

// MIDI related constants.
pub const DEFAULT_BPM: f64 = 120.0;
pub const KEY_IN_OCTAVE: i64 = 12;
pub const TICKS_IN_BEAT: i64 = 480;
pub const MIN_BREAK_LENGTH_BETWEEN_PITCH_SECTIONS: i64 = TICKS_IN_BEAT;

/// Application directories, matching `platformdirs.AppDirs("libresvip")`.
///
/// On Windows `platformdirs` uses `%LOCALAPPDATA%\\libresvip\\libresvip` for
/// both `user_config_path` and `user_data_dir` (app name + app author are both
/// derived from the package name).
#[derive(Clone, Debug)]
pub struct AppDirs {
    pub user_config_path: PathBuf,
    pub user_data_path: PathBuf,
    pub user_cache_path: PathBuf,
    pub user_log_path: PathBuf,
}

impl AppDirs {
    pub fn new() -> Self {
        let app_name = PACKAGE_NAME;
        #[cfg(target_os = "windows")]
        {
            let local_app_data = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    let home = std::env::var_os("USERPROFILE")
                        .map(PathBuf::from)
                        .unwrap_or_default();
                    home.join("AppData").join("Local")
                });
            let base = local_app_data.join(app_name).join(app_name);
            Self {
                user_config_path: base.clone(),
                user_data_path: base.clone(),
                user_cache_path: base.join("Cache"),
                user_log_path: base.join("Logs"),
            }
        }
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            Self {
                user_config_path: home.join("Library/Application Support").join(app_name),
                user_data_path: home.join("Library/Application Support").join(app_name),
                user_cache_path: home.join("Library/Caches").join(app_name),
                user_log_path: home.join("Library/Logs").join(app_name),
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            let config_home = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            let data_home = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"));
            let cache_home = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".cache"));
            Self {
                user_config_path: config_home.join(app_name),
                user_data_path: data_home.join(app_name),
                user_cache_path: cache_home.join(app_name),
                user_log_path: cache_home.join(app_name).join("log"),
            }
        }
    }
}

impl Default for AppDirs {
    fn default() -> Self {
        Self::new()
    }
}

/// Global application directories (lazily initialised).
pub fn app_dirs() -> &'static AppDirs {
    use std::sync::OnceLock;
    static DIRS: OnceLock<AppDirs> = OnceLock::new();
    DIRS.get_or_init(AppDirs::new)
}

/// Directory holding bundled resources (`libresvip/res`).
///
/// The Rust binary locates it next to the executable (`res/`), which is where
/// the build script copies the Python resource tree.
pub fn res_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LIBRESVIP_RES_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            let candidate = root.join("res");
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets")
        .join("res")
}
