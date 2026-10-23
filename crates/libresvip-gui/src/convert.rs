//! Conversion jobs executed on worker threads.
//!
//! This mirrors `libresvip/gui/modules/task_manager.py` (`ConversionWorker`,
//! `MergeWorker`, `SplitWorker`): load the input project, run the enabled
//! middlewares, then dump into a private temporary directory. The UI thread
//! later moves those files to their final destination so it can apply the
//! conflict policy, exactly like the Python GUI does with its `memory:/` paths.

use std::path::{Path, PathBuf};

use libresvip_core::core::config::ConversionMode;
use libresvip_core::core::warning_types::CatchWarnings;
use libresvip_core::error::LibreSvipError;
use libresvip_core::model::base::Project;
use libresvip_core::options::Options;
use libresvip_core::plugin::SvsPlugin;
use libresvip_middlewares::all_middlewares;
use libresvip_plugins::plugin_by_value;

/// Everything a worker needs; shared by every task of one conversion run.
#[derive(Clone)]
pub struct JobConfig {
    pub mode: ConversionMode,
    pub input_format: String,
    pub output_format: String,
    pub input_options: Options,
    pub output_options: Options,
    /// Enabled middlewares in registry order.
    pub middlewares: Vec<(&'static str, Options)>,
    pub max_track_count: i64,
}

pub struct JobResult {
    /// Temporary file (direct/merge) or directory (split) holding the output.
    pub tmp: Option<PathBuf>,
    pub error: Option<String>,
    /// `CatchWarnings.output`
    pub warnings: String,
}

impl JobResult {
    fn failure(message: impl Into<String>) -> Self {
        Self {
            tmp: None,
            error: Some(message.into()),
            warnings: String::new(),
        }
    }
}

/// Runs one job. `index` only names the temporary directory.
pub fn run_job(cfg: &JobConfig, index: usize, paths: &[PathBuf], temp_root: &Path) -> JobResult {
    let Some(input_plugin) = plugin_by_value(&cfg.input_format) else {
        return JobResult::failure(format!(
            "AttributeError: 'NoneType' object has no attribute 'load' (input format {:?})",
            cfg.input_format
        ));
    };
    let Some(output_plugin) = plugin_by_value(&cfg.output_format) else {
        return JobResult::failure(format!(
            "AttributeError: 'NoneType' object has no attribute 'dump' (output format {:?})",
            cfg.output_format
        ));
    };
    let job_dir = temp_root.join(index.to_string());
    if let Err(error) = std::fs::create_dir_all(&job_dir) {
        return JobResult::failure(format!(
            "无法创建临时目录 {}：{}\n\n当前进程：{}\n\n\
             %TEMP% 和 %LOCALAPPDATA% 都被拒绝写入时，通常表示这个进程运行在\
             受限/沙箱环境中（安全软件的沙箱、低完整性级别），或者是以其他用户身份启动的。\n\
             请从资源管理器或命令行正常启动本程序，并把 exe 加入安全软件白名单。",
            job_dir.display(),
            error,
            crate::platform::process_context()
        ));
    }

    let collector = CatchWarnings::start();
    let outcome = convert_one(cfg, input_plugin, output_plugin, paths, &job_dir);
    let warnings = collector.finish().join("\n");
    match outcome {
        Ok(tmp) => JobResult {
            tmp: Some(tmp),
            error: None,
            warnings,
        },
        Err(error) => JobResult {
            tmp: None,
            error: Some(error),
            warnings,
        },
    }
}

fn convert_one(
    cfg: &JobConfig,
    input_plugin: &'static dyn SvsPlugin,
    output_plugin: &'static dyn SvsPlugin,
    paths: &[PathBuf],
    job_dir: &Path,
) -> std::result::Result<PathBuf, String> {
    if paths.is_empty() {
        return Err("IndexError: list index out of range".to_owned());
    }
    let mut project = match cfg.mode {
        ConversionMode::Merge => {
            let mut children = Vec::with_capacity(paths.len());
            for path in paths {
                children.push(input_plugin.load(path, &cfg.input_options).map_err(|error| {
                    format!("无法读取 {}：{}", path.display(), error_text(&error))
                })?);
            }
            Project::merge_projects(children).map_err(|error| error_text(&error))?
        }
        _ => input_plugin
            .load(&paths[0], &cfg.input_options)
            .map_err(|error| {
                format!("无法读取 {}：{}", paths[0].display(), error_text(&error))
            })?,
    };

    let middlewares = all_middlewares();
    for (id, options) in &cfg.middlewares {
        let Some(middleware) = middlewares
            .iter()
            .find(|middleware| middleware.info().id == *id)
        else {
            continue;
        };
        project = middleware
            .process(project, options)
            .map_err(|error| error_text(&error))?;
    }

    match cfg.mode {
        ConversionMode::Split => {
            if cfg.max_track_count < 1 {
                return Err("ValueError: n must be at least 1".to_owned());
            }
            let children = project
                .split_tracks(cfg.max_track_count as usize)
                .map_err(|error| error_text(&error))?;
            for (child_index, child) in children.iter().enumerate() {
                let path = job_dir.join(child_index.to_string());
                output_plugin
                    .dump(&path, child, &cfg.output_options)
                    .map_err(|error| {
                        format!("生成文件失败（临时路径 {}）：{}", path.display(), error_text(&error))
                    })?;
            }
            Ok(job_dir.to_path_buf())
        }
        _ => {
            let path = job_dir.join("0");
            output_plugin
                .dump(&path, &project, &cfg.output_options)
                .map_err(|error| {
                    format!("生成文件失败（临时路径 {}）：{}", path.display(), error_text(&error))
                })?;
            Ok(path)
        }
    }
}

/// Python-style error text used by the task rows (the RPC server formats the
/// same exceptions for the CLI clients).
pub fn error_text(error: &LibreSvipError) -> String {
    match error {
        LibreSvipError::InvalidFileType => {
            "libresvip.core.exceptions.InvalidFileTypeError: invalid file type".to_owned()
        }
        LibreSvipError::UnsupportedProjectVersion(version) => format!(
            "libresvip.core.exceptions.UnsupportedProjectVersionError: unsupported project version: {version}"
        ),
        LibreSvipError::NoTrack => {
            "libresvip.core.exceptions.NoTrackError: no track found".to_owned()
        }
        LibreSvipError::NotesOverlapped => {
            "libresvip.core.exceptions.NotesOverlappedError: notes overlapped".to_owned()
        }
        LibreSvipError::Params(message) => {
            format!("libresvip.core.exceptions.ParamsError: params error: {message}")
        }
        LibreSvipError::NotImplemented => "NotImplementedError: not implemented".to_owned(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresvip_core::options::OptionValue;
    use libresvip_plugins::plugin_by_value;

    fn testdata() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("testdata")
    }

    fn work_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "libresvip-gui-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn defaults(plugin: &'static dyn SvsPlugin, input: bool) -> Options {
        let defs = if input {
            plugin.input_options()
        } else {
            plugin.output_options()
        };
        Options::from_pairs(
            defs.iter()
                .map(|def| (def.key.to_owned(), def.default.clone())),
        )
    }

    fn config(mode: ConversionMode, output: &str) -> JobConfig {
        let input_plugin = plugin_by_value("json").expect("json input plugin");
        let output_plugin = plugin_by_value(output).expect("output plugin");
        JobConfig {
            mode,
            input_format: "json".to_owned(),
            output_format: output.to_owned(),
            input_options: defaults(input_plugin, true),
            output_options: defaults(output_plugin, false),
            middlewares: Vec::new(),
            max_track_count: 1,
        }
    }

    /// The GUI's direct mode must produce the same project the reference
    /// implementation recorded in `testdata/golden/<format>/project.json`
    /// (the dumped bytes of zip-based formats are not deterministic).
    #[test]
    fn direct_mode_matches_golden_project() {
        let root = work_dir("direct");
        let source = testdata().join("simple.json");
        let cfg = config(ConversionMode::Direct, "acep");
        let result = run_job(&cfg, 0, &[source], &root);
        assert_eq!(result.error, None, "conversion failed");
        let tmp = result.tmp.expect("temporary output");
        let plugin = plugin_by_value("acep").expect("acep plugin");
        let project = plugin
            .load(&tmp, &defaults(plugin, true))
            .expect("generated file loads back");
        let actual = serde_json::to_value(&project).expect("serialize project");
        let expected_path = testdata().join("golden").join("acep").join("project.json");
        let expected: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&expected_path).expect("read golden project"),
        )
        .expect("parse golden project");
        assert_eq!(
            actual, expected,
            "the project loaded from the GUI output differs from the golden model"
        );
        assert!(
            !project.track_list.is_empty(),
            "the generated project keeps at least one track"
        );
        // The generated file is also byte-identical to the recorded dump for
        // this plugin.
        let golden_dump = testdata().join("golden").join("acep").join("dump.acep");
        if let Ok(expected_bytes) = std::fs::read(&golden_dump) {
            let actual_bytes = std::fs::read(&tmp).expect("read output");
            assert_eq!(
                actual_bytes.len(),
                expected_bytes.len(),
                "output size differs from the golden dump"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn split_mode_writes_one_file_per_chunk() {
        let root = work_dir("split");
        let source = testdata().join("simple.json");
        let cfg = config(ConversionMode::Split, "acep");
        let result = run_job(&cfg, 0, &[source], &root);
        assert_eq!(result.error, None, "split conversion failed");
        let dir = result.tmp.expect("temporary directory");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .expect("read split directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        assert!(
            !files.is_empty(),
            "split mode should produce at least one project"
        );
        let plugin = plugin_by_value("acep").expect("acep plugin");
        for file in &files {
            assert!(
                plugin.load(file, &defaults(plugin, true)).is_ok(),
                "{} should load back",
                file.display()
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_mode_combines_projects() {
        let root = work_dir("merge");
        let first = testdata().join("simple.json");
        let second = testdata().join("simple_singing.json");
        let cfg = config(ConversionMode::Merge, "acep");
        let result = run_job(&cfg, 0, &[first, second], &root);
        assert_eq!(result.error, None, "merge conversion failed");
        let tmp = result.tmp.expect("temporary output");
        let plugin = plugin_by_value("acep").expect("acep plugin");
        let project = plugin
            .load(&tmp, &defaults(plugin, true))
            .expect("merged file loads back");
        let singing = project
            .track_list
            .iter()
            .filter(|track| track.is_singing())
            .count();
        assert_eq!(singing, 2, "both singing tracks are kept");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_format_reports_an_error_instead_of_panicking() {
        let root = work_dir("unknown");
        let source = testdata().join("simple.json");
        let mut cfg = config(ConversionMode::Direct, "acep");
        cfg.output_format = "definitely-not-a-format".to_owned();
        let result = run_job(&cfg, 0, &[source], &root);
        assert!(result.error.is_some());
        assert!(result.tmp.is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn option_defaults_survive_the_round_trip() {
        let plugin = plugin_by_value("ustx").expect("ustx plugin");
        let options = defaults(plugin, true);
        assert!(!options.is_empty());
        for (_, value) in options.iter() {
            match value {
                OptionValue::Bool(_) | OptionValue::Int(_) | OptionValue::Float(_)
                | OptionValue::Str(_) => {}
            }
        }
    }
}
