//! Application state and the conversion pipeline glue.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use libresvip_core::core::config::{self, ConflictPolicy, ConversionMode, DarkMode, Settings};
use libresvip_core::options::Options;
use libresvip_middlewares::all_middlewares;
use libresvip_plugins::plugin_by_suffix;

use crate::convert::{self, JobConfig, JobResult};
use crate::i18n::{Lang, Translator};
use crate::state::{
    all_format_entries, dir_valid, input_formats, output_dir, output_file_name, output_formats,
    FieldState, FormatEntry, MiddlewareState, Task,
};
use crate::theme::{self, Palette};
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsTab {
    Basic,
    SavePath,
    Plugins,
    Updates,
}

/// A pending "file exists, overwrite?" question.
pub struct Conflict {
    pub index: usize,
    pub path: PathBuf,
    /// `(temporary file, destination)` pairs.
    pub targets: Vec<(PathBuf, PathBuf)>,
}

pub struct App {
    pub lang: Lang,
    pub translator: Translator,
    pub pal: Palette,
    pub settings: Settings,
    pub input_formats: Vec<FormatEntry>,
    pub output_formats: Vec<FormatEntry>,
    pub input_format: String,
    pub output_format: String,
    pub input_fields: Vec<FieldState>,
    pub output_fields: Vec<FieldState>,
    pub middlewares: Vec<MiddlewareState>,
    pub tasks: Vec<Task>,
    pub mode: ConversionMode,
    pub save_folder: String,
    pub save_folder_invalid: bool,
    pub busy: bool,
    pub running: usize,
    tx: Sender<(usize, JobResult)>,
    rx: Receiver<(usize, JobResult)>,
    pub pending_conflicts: VecDeque<Conflict>,
    pub conflict: Option<Conflict>,
    pub yes_to_all: bool,
    pub no_to_all: bool,
    pub show_settings: bool,
    pub settings_tab: SettingsTab,
    pub show_about: bool,
    /// `(heading, body)` of the error/warning detail window.
    pub detail: Option<(String, String)>,
    pub split_ratio: f32,
    pub app_icon: Option<egui::TextureHandle>,
    pub system_dark: bool,
    pub toast: Option<(String, f64)>,
    pub update_notice: Option<String>,
    pub input_info_open: bool,
    pub output_info_open: bool,
    pub task_toolbar_open: bool,
    pub input_expanded: bool,
    pub output_expanded: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_files: Vec<PathBuf>) -> Self {
        let settings = config::settings();
        let lang = Lang::from_settings(settings.language);
        let translator = Translator::new(lang);
        let system_dark = system_uses_dark_theme(&cc.egui_ctx);
        let dark = match settings.dark_mode {
            DarkMode::Dark => true,
            DarkMode::Light => false,
            DarkMode::System => system_dark,
        };
        let pal = if dark {
            Palette::dark()
        } else {
            Palette::light()
        };
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, &pal);

        let input_formats = input_formats();
        let output_formats = output_formats();
        let input_format = settings
            .last_input_format
            .clone()
            .filter(|value| input_formats.iter().any(|entry| entry.suffix == *value))
            .or_else(|| input_formats.first().map(|entry| entry.suffix.to_owned()))
            .unwrap_or_default();
        let output_format = settings
            .last_output_format
            .clone()
            .filter(|value| output_formats.iter().any(|entry| entry.suffix == *value))
            .or_else(|| output_formats.first().map(|entry| entry.suffix.to_owned()))
            .unwrap_or_default();

        let (tx, rx) = channel();
        let mut app = Self {
            lang,
            translator,
            pal,
            settings,
            input_formats,
            output_formats,
            input_format,
            output_format,
            input_fields: Vec::new(),
            output_fields: Vec::new(),
            middlewares: Vec::new(),
            tasks: Vec::new(),
            mode: ConversionMode::Direct,
            save_folder: String::new(),
            save_folder_invalid: false,
            busy: false,
            running: 0,
            tx,
            rx,
            pending_conflicts: VecDeque::new(),
            conflict: None,
            yes_to_all: false,
            no_to_all: false,
            show_settings: false,
            settings_tab: SettingsTab::Basic,
            show_about: false,
            detail: None,
            split_ratio: 0.5,
            app_icon: load_texture(&cc.egui_ctx),
            system_dark,
            toast: None,
            update_notice: None,
            input_info_open: false,
            output_info_open: false,
            task_toolbar_open: false,
            input_expanded: true,
            output_expanded: true,
        };
        // Python writes `save_folder` as a pickled `pathlib.WindowsPath`, which
        // the YAML reader reduces to an empty path; treat that as "same folder".
        if app.settings.save_folder.as_os_str().is_empty() {
            app.settings.save_folder = PathBuf::from(".");
        }
        app.save_folder = app
            .settings
            .save_folder
            .to_string_lossy()
            .into_owned();
        app.reload_middlewares();
        app.reload_input_fields();
        app.reload_output_fields();
        // Files passed on the command line behave like a drop: they are added
        // to the task list (and auto-detect switches the input format).
        app.add_paths(initial_files);
        app
    }

    // -- translation -------------------------------------------------------

    pub fn t(&self, en: &'static str) -> String {
        self.translator.t(en)
    }

    pub fn td(&self, text: &str) -> String {
        self.translator.t_dyn(text)
    }

    // -- formats -----------------------------------------------------------

    pub fn input_entry(&self) -> Option<&FormatEntry> {
        self.input_formats
            .iter()
            .find(|entry| entry.suffix == self.input_format)
    }

    pub fn output_entry(&self) -> Option<&FormatEntry> {
        self.output_formats
            .iter()
            .find(|entry| entry.suffix == self.output_format)
    }

    /// `TaskManager.output_ext`
    pub fn output_ext(&self) -> String {
        if self.settings.auto_set_output_extension && !self.output_format.is_empty() {
            format!(".{}", self.output_format)
        } else {
            String::new()
        }
    }

    pub fn set_input_format(&mut self, value: String) {
        if value == self.input_format {
            return;
        }
        let suffixes: Vec<String> = self
            .input_formats
            .iter()
            .find(|entry| entry.suffix == value)
            .map(|entry| entry.suffixes.iter().map(|s| s.to_lowercase()).collect())
            .unwrap_or_default();
        if self.settings.reset_tasks_on_input_change {
            // `more_itertools.rstrip`: drop trailing tasks that do not match.
            let mut keep = self.tasks.len();
            while keep > 0 {
                let task = &self.tasks[keep - 1];
                let path = task.path.to_string_lossy().to_lowercase();
                let matches = suffixes
                    .iter()
                    .any(|suffix| path.ends_with(&format!(".{suffix}")));
                if matches {
                    break;
                }
                keep -= 1;
            }
            self.tasks.truncate(keep);
        }
        self.input_format = value;
        self.settings.last_input_format = Some(self.input_format.clone());
        self.reload_input_fields();
    }

    pub fn set_output_format(&mut self, value: String) {
        if value == self.output_format {
            return;
        }
        self.output_format = value;
        self.settings.last_output_format = Some(self.output_format.clone());
        self.reload_output_fields();
        // `output_format_changed` is connected to `reset_output_ext`.
        let ext = self.output_ext();
        for task in &mut self.tasks {
            task.ext = ext.clone();
        }
    }

    pub fn swap_formats(&mut self) {
        let input = self.input_format.clone();
        let output = self.output_format.clone();
        let can_swap = self
            .input_formats
            .iter()
            .any(|entry| entry.suffix == output)
            && self
                .output_formats
                .iter()
                .any(|entry| entry.suffix == input);
        if can_swap {
            self.set_input_format(output);
            self.set_output_format(input);
        }
    }

    fn reload_input_fields(&mut self) {
        self.input_fields = plugin_fields(&self.input_format, true);
    }

    fn reload_output_fields(&mut self) {
        self.output_fields = plugin_fields(&self.output_format, false);
    }

    pub fn reload_middlewares(&mut self) {
        let mut states = Vec::new();
        for middleware in all_middlewares() {
            let info = middleware.info();
            let fields = middleware
                .process_options()
                .iter()
                .map(FieldState::from_def)
                .collect();
            states.push(MiddlewareState {
                id: info.id,
                name: info.name,
                description: info.description,
                enabled: false,
                fields,
            });
        }
        self.middlewares = states;
    }

    // -- tasks -------------------------------------------------------------

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let ext = self.output_ext();
        let mut last_suffix = None;
        for path in paths {
            last_suffix = path
                .extension()
                .map(|ext| ext.to_string_lossy().to_lowercase());
            self.tasks.push(Task::new(path, ext.clone()));
        }
        if self.settings.auto_detect_input_format {
            if let Some(suffix) = last_suffix {
                if let Some(plugin) = plugin_by_suffix(&suffix) {
                    let detected = plugin.info().suffix();
                    if detected != self.input_format {
                        self.set_input_format(detected.to_owned());
                        self.set_output_format(self.output_format.clone());
                    }
                }
            }
        }
    }

    pub fn clear_tasks(&mut self) {
        self.tasks.clear();
        self.pending_conflicts.clear();
        self.conflict = None;
    }

    pub fn remove_task(&mut self, index: usize) {
        if index < self.tasks.len() {
            let task = self.tasks.remove(index);
            if let Some(tmp) = task.tmp {
                let _ = std::fs::remove_dir_all(&tmp);
                let _ = std::fs::remove_file(&tmp);
            }
            self.pending_conflicts.retain(|conflict| conflict.index != index);
        }
    }

    pub fn reset_stems(&mut self) {
        for task in &mut self.tasks {
            task.stem = task
                .path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| task.name.clone());
        }
    }

    pub fn remove_other_extensions(&mut self) {
        let Some(entry) = self.input_entry() else {
            return;
        };
        let suffixes: Vec<String> = entry
            .suffixes
            .iter()
            .map(|suffix| suffix.to_lowercase())
            .collect();
        let mut index = 0;
        while index < self.tasks.len() {
            let extension = self.tasks[index]
                .path
                .extension()
                .map(|ext| ext.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if suffixes.iter().any(|suffix| *suffix == extension) {
                index += 1;
            } else {
                self.remove_task(index);
            }
        }
    }

    pub fn task_output_dir(&self, index: usize) -> PathBuf {
        output_dir(&self.settings.save_folder, &self.tasks[index])
    }

    /// `(temporary file, destination file)` pairs for one task.
    pub fn task_targets(&self, index: usize) -> Vec<(PathBuf, PathBuf)> {
        let task = &self.tasks[index];
        let dir = self.task_output_dir(index);
        let mut targets = Vec::new();
        match self.mode {
            ConversionMode::Split => {
                let Some(tmp) = &task.tmp else { return targets };
                let mut entries: Vec<PathBuf> = std::fs::read_dir(tmp)
                    .map(|entries| {
                        entries
                            .filter_map(|entry| entry.ok())
                            .map(|entry| entry.path())
                            .filter(|path| path.is_file())
                            .collect()
                    })
                    .unwrap_or_default();
                entries.sort_by_key(|path| numeric_stem(path));
                for (child_index, child) in entries.iter().enumerate() {
                    let name = format!("{}_{:02}{}", task.stem, child_index + 1, task.ext);
                    targets.push((child.clone(), dir.join(name)));
                }
            }
            _ => {
                if let Some(tmp) = &task.tmp {
                    targets.push((
                        tmp.clone(),
                        dir.join(output_file_name(&task.stem, &task.ext)),
                    ));
                }
            }
        }
        targets
    }

    #[allow(dead_code)] // mirrors `TaskManager.output_path_exists`
    pub fn output_path_exists(&self, index: usize) -> bool {
        self.task_targets(index)
            .iter()
            .any(|(_, target)| target.exists())
    }

    /// Copies the temporary output into the destination folder.
    pub fn move_to_output(&mut self, index: usize) -> bool {
        let targets = self.task_targets(index);
        if targets.is_empty() {
            return false;
        }
        let mut result = true;
        for (tmp, target) in targets {
            if let Some(parent) = target.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    self.tasks[index].error = write_error(&target, &error);
                    result = false;
                    continue;
                }
            }
            if let Err(error) = replace_file(&tmp, &target) {
                // Keep the converted result: the user can copy it manually when
                // the destination is locked or blocked.
                self.tasks[index].error = format!(
                    "{}\n\n转换结果已保留在临时文件：\n{}",
                    write_error(&target, &error),
                    tmp.display()
                );
                result = false;
            }
        }
        if result {
            // Everything is in place; drop the temporary copy.
            if let Some(tmp) = self.tasks[index].tmp.clone() {
                if tmp.is_dir() {
                    let _ = std::fs::remove_dir_all(&tmp);
                } else {
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }
        result
    }

    // -- conversion --------------------------------------------------------

    pub fn can_start(&self) -> bool {
        !self.tasks.is_empty() && !self.busy
    }

    pub fn start_conversion(&mut self) {
        if self.tasks.is_empty() || self.busy {
            return;
        }
        if self.input_format.is_empty() || self.output_format.is_empty() {
            let message = self.t("Please select input and output formats first.");
            for task in &mut self.tasks {
                task.reset_status();
                task.success = Some(false);
                task.error = message.clone();
            }
            return;
        }
        self.yes_to_all = false;
        self.no_to_all = false;
        self.pending_conflicts.clear();
        self.conflict = None;
        for task in &mut self.tasks {
            task.reset_status();
        }

        let config = JobConfig {
            mode: self.mode,
            input_format: self.input_format.clone(),
            output_format: self.output_format.clone(),
            input_options: collect_options(&self.input_fields),
            output_options: collect_options(&self.output_fields),
            middlewares: self
                .middlewares
                .iter()
                .filter(|middleware| middleware.enabled)
                .map(|middleware| (middleware.id, collect_options(&middleware.fields)))
                .collect(),
            max_track_count: self.settings.max_track_count,
        };

        let save_folder = self.settings.save_folder.clone();
        // Probe the destination, but never block on it: the probe is a
        // best-effort early warning, and a false alarm must not stop a
        // conversion that would have worked (the real write reports its own,
        // more precise error).
        let mut probe_warning: Option<String> = None;
        if let Some(task) = self.tasks.first() {
            let dir = output_dir(&save_folder, task);
            if let Err(message) = probe_writable(&dir) {
                probe_warning = Some(format!(
                    "输出文件夹可能不可写：{}（{}）",
                    dir.display(),
                    message
                ));
            }
        }

        let jobs: Vec<(usize, Vec<PathBuf>)> = match self.mode {
            ConversionMode::Merge => vec![(
                0,
                self.tasks.iter().map(|task| task.path.clone()).collect(),
            )],
            _ => self
                .tasks
                .iter()
                .enumerate()
                .map(|(index, task)| (index, vec![task.path.clone()]))
                .collect(),
        };
        if jobs.is_empty() {
            return;
        }
        self.toast = probe_warning.map(|message| (message, 0.0));

        let fallback_dir = self
            .tasks
            .first()
            .map(|task| output_dir(&save_folder, task));
        let temp_root = temp_root(fallback_dir.as_deref());
        let job_count = jobs.len();
        for (index, _) in &jobs {
            if let Some(task) = self.tasks.get_mut(*index) {
                task.running = true;
            }
        }
        self.running = job_count;
        self.busy = true;

        let queue = Arc::new(Mutex::new(jobs));
        let workers = if self.settings.multi_threaded_conversion {
            job_count.min(8)
        } else {
            1
        };
        for _ in 0..workers {
            let queue = Arc::clone(&queue);
            let config = config.clone();
            let tx = self.tx.clone();
            let temp_root = temp_root.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let mut queue = match queue.lock() {
                        Ok(queue) => queue,
                        Err(_) => break,
                    };
                    queue.pop()
                };
                let Some((index, paths)) = job else { break };
                let result = convert::run_job(&config, index, &paths, &temp_root);
                if tx.send((index, result)).is_err() {
                    break;
                }
            });
        }
    }

    pub fn poll_jobs(&mut self, ctx: &egui::Context) {
        loop {
            let message = self.rx.try_recv();
            let Ok((index, result)) = message else { break };
            self.running = self.running.saturating_sub(1);
            if index < self.tasks.len() {
                let task = &mut self.tasks[index];
                task.running = false;
                match result.error {
                    Some(error) => {
                        task.success = Some(false);
                        task.error = error;
                    }
                    None => {
                        task.success = Some(true);
                        task.warning = result.warnings;
                        task.tmp = result.tmp;
                    }
                }
            }
            if self.tasks.get(index).map(|task| task.success) == Some(Some(true)) {
                self.pending_conflicts.push_back(Conflict {
                    index,
                    path: self.tasks[index].path.clone(),
                    targets: self.task_targets(index),
                });
            }
            ctx.request_repaint();
        }
        self.resolve_conflicts();
        if self.running == 0
            && self.busy
            && self.pending_conflicts.is_empty()
            && self.conflict.is_none()
        {
            self.finish_conversion();
        }
    }

    fn resolve_conflicts(&mut self) {
        while self.conflict.is_none() {
            let Some(conflict) = self.pending_conflicts.pop_front() else {
                break;
            };
            if self.tasks.get(conflict.index).map(|task| task.success) != Some(Some(true)) {
                continue;
            }
            let exists = conflict.targets.iter().any(|(_, target)| target.exists());
            if !exists {
                let index = conflict.index;
                if self.move_to_output(index) {
                    self.tasks[index].warning = self.tasks[index].warning.clone();
                } else {
                    self.tasks[index].success = Some(false);
                }
                continue;
            }
            match self.settings.conflict_policy {
                ConflictPolicy::Overwrite => {
                    let index = conflict.index;
                    if !self.move_to_output(index) {
                        self.tasks[index].success = Some(false);
                    }
                }
                ConflictPolicy::Skip => {
                    self.tasks[conflict.index].skipped = true;
                }
                ConflictPolicy::Prompt => {
                    if self.yes_to_all {
                        let index = conflict.index;
                        if !self.move_to_output(index) {
                            self.tasks[index].success = Some(false);
                        }
                    } else if self.no_to_all {
                        self.tasks[conflict.index].skipped = true;
                    } else {
                        self.conflict = Some(conflict);
                    }
                }
            }
        }
    }

    pub fn answer_conflict(&mut self, overwrite: bool, all: bool) {
        let Some(conflict) = self.conflict.take() else {
            return;
        };
        if all {
            if overwrite {
                self.yes_to_all = true;
            } else {
                self.no_to_all = true;
            }
        }
        if overwrite {
            if !self.move_to_output(conflict.index) {
                self.tasks[conflict.index].success = Some(false);
            }
        } else {
            self.tasks[conflict.index].skipped = true;
        }
        self.resolve_conflicts();
    }

    fn finish_conversion(&mut self) {
        self.busy = false;
        self.running = 0;
        if self.settings.open_save_folder_on_completion {
            let success = self
                .tasks
                .iter()
                .position(|task| task.success == Some(true) && !task.skipped);
            if let Some(index) = success {
                let dir = self.task_output_dir(index);
                open_path(&dir);
            }
        }
    }

    pub fn open_output(&self, index: usize, folder_only: bool) {
        let dir = self.task_output_dir(index);
        if folder_only {
            open_path(&dir);
        } else {
            let task = &self.tasks[index];
            let path = dir.join(output_file_name(&task.stem, &task.ext));
            open_path(&path);
        }
    }

    // -- settings ----------------------------------------------------------

    pub fn persist_settings(&mut self) {
        self.settings.save_folder = PathBuf::from(&self.save_folder);
        self.settings.language = self.lang.to_settings();
        config::update_settings(|settings| *settings = self.settings.clone());
        let _ = config::save_settings();
    }

    pub fn set_language(&mut self, lang: Lang) {
        self.lang = lang;
        self.translator = Translator::new(lang);
        self.settings.language = lang.to_settings();
        self.persist_settings();
    }

    pub fn set_theme(&mut self, mode: DarkMode, ctx: &egui::Context) {
        self.settings.dark_mode = mode;
        let dark = match mode {
            DarkMode::Dark => true,
            DarkMode::Light => false,
            DarkMode::System => system_uses_dark_theme(ctx),
        };
        self.pal = if dark {
            Palette::dark()
        } else {
            Palette::light()
        };
        theme::apply(ctx, &self.pal);
        self.persist_settings();
    }

    pub fn apply_save_folder(&mut self) {
        if dir_valid(&self.save_folder) {
            self.settings.save_folder = PathBuf::from(&self.save_folder);
            self.save_folder_invalid = false;
            self.persist_settings();
        } else {
            self.save_folder = self.settings.save_folder.to_string_lossy().into_owned();
            self.save_folder_invalid = true;
        }
    }

    pub fn toggle_plugin(&mut self, id: &'static str) {
        if let Some(position) = self
            .settings
            .disabled_plugins
            .iter()
            .position(|plugin| plugin == id)
        {
            self.settings.disabled_plugins.remove(position);
        } else {
            self.settings.disabled_plugins.push(id.to_owned());
        }
        self.input_formats = input_formats();
        self.output_formats = output_formats();
        if !self
            .input_formats
            .iter()
            .any(|entry| entry.suffix == self.input_format)
        {
            self.input_format = self
                .input_formats
                .first()
                .map(|entry| entry.suffix.to_owned())
                .unwrap_or_default();
            self.reload_input_fields();
        }
        if !self
            .output_formats
            .iter()
            .any(|entry| entry.suffix == self.output_format)
        {
            self.output_format = self
                .output_formats
                .first()
                .map(|entry| entry.suffix.to_owned())
                .unwrap_or_default();
            self.reload_output_fields();
        }
        self.persist_settings();
    }

    /// Follows the OS colour scheme while the "System" theme is selected.
    /// The value comes from winit (`ThemeChanged`), so no polling is needed.
    pub fn sync_system_theme(&mut self, ctx: &egui::Context) {
        if self.settings.dark_mode != DarkMode::System {
            return;
        }
        let dark = system_uses_dark_theme(ctx);
        if dark != self.system_dark {
            self.system_dark = dark;
            self.pal = if dark {
                Palette::dark()
            } else {
                Palette::light()
            };
            theme::apply(ctx, &self.pal);
        }
    }

    pub fn plugin_entries(&self) -> Vec<FormatEntry> {
        all_format_entries()
    }

    /// "Import Projects (Ctrl+O)" — native multi-file picker.
    pub fn open_files(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        if let Some(entry) = self.input_entry() {
            let filter = format!(
                "{} ({})",
                self.td(entry.file_format),
                entry.suffixes_display()
            );
            let extensions: Vec<String> = entry
                .suffixes
                .iter()
                .map(|suffix| (*suffix).to_owned())
                .collect();
            if !extensions.is_empty() {
                dialog = dialog.add_filter(filter, &extensions);
            }
        }
        dialog = dialog.add_filter("All Files", &["*"]);
        if let Some(files) = dialog.pick_files() {
            self.add_paths(files);
        }
    }

    /// "Choose Output Folder"
    pub fn choose_folder(&mut self) {
        let start = std::path::PathBuf::from(&self.save_folder);
        let mut dialog = rfd::FileDialog::new();
        if start.is_dir() {
            dialog = dialog.set_directory(&start);
        }
        if let Some(folder) = dialog.pick_folder() {
            self.save_folder = folder.to_string_lossy().into_owned();
            self.settings.save_folder = folder;
            self.save_folder_invalid = false;
            self.persist_settings();
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|input| input.time);
        self.sync_system_theme(ctx);
        self.poll_jobs(ctx);
        ui::draw(self, ctx);
        // Simplify the task list while a conversion is running.
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some((_, expires)) = &mut self.toast {
            if *expires <= 0.0 {
                // A toast posted outside `update` starts its 8 second timer here.
                *expires = now + 8.0;
            }
            if now > *expires {
                self.toast = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_settings();
    }
}

fn plugin_fields(format: &str, input: bool) -> Vec<FieldState> {
    let Some(plugin) = libresvip_plugins::plugin_by_value(format) else {
        return Vec::new();
    };
    let defs = if input {
        plugin.input_options()
    } else {
        plugin.output_options()
    };
    defs.iter().map(FieldState::from_def).collect()
}

fn collect_options(fields: &[FieldState]) -> Options {
    let mut options = Options::new();
    for field in fields {
        options.insert(field.key, field.to_value());
    }
    options
}

fn numeric_stem(path: &Path) -> u64 {
    path.file_stem()
        .and_then(|stem| stem.to_string_lossy().parse::<u64>().ok())
        .unwrap_or(u64::MAX)
}

/// Formats a write failure together with the target path; a bare
/// `os error 5` tells the user nothing about *what* could not be written.
fn write_error(target: &Path, error: &std::io::Error) -> String {
    let mut message = format!("无法写入 {}：{}", target.display(), error);
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => message.push_str(
            "\n\n目标位置没有写权限，或者该文件正被其他程序占用。\n\
             请在“输出设置”里换一个文件夹，或关闭正在使用该文件的程序。",
        ),
        std::io::ErrorKind::NotFound => {
            message.push_str("\n\n找不到目标文件夹，请检查“输出设置”里的路径。")
        }
        _ => {}
    }
    message
}

/// Creates `dir` if needed and verifies that a new file can be created there.
/// The error names the exact step and path, because a bare "access denied"
/// does not say whether the folder or the file name is the problem.
fn probe_writable(dir: &Path) -> Result<(), String> {
    if let Err(error) = std::fs::create_dir_all(dir) {
        return Err(format!("无法创建文件夹：{error}"));
    }
    let probe = dir.join(format!("libresvip-write-probe-{}.tmp", std::process::id()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(error) => Err(format!(
            "写入测试文件 {} 失败：{error}",
            probe.display()
        )),
    }
}

/// Replaces `target` with `tmp`, retrying briefly: anti-virus scanners and
/// other applications can hold the destination open for a fraction of a second
/// right after it is created or written.
fn replace_file(tmp: &Path, target: &Path) -> std::io::Result<()> {
    let mut last_error: Option<std::io::Error> = None;
    for attempt in 0..4 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(150 * attempt));
        }
        if target.exists() {
            if let Err(error) = std::fs::remove_file(target) {
                last_error = Some(error);
                continue;
            }
        }
        match std::fs::copy(tmp, target) {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("unknown write error")))
}

fn temp_root(fallback_dir: Option<&Path>) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!("libresvip-gui-{}-{counter}", std::process::id());
    // %TEMP% is the natural place, but restricted environments (sandboxes,
    // redirected TEMP) can deny it; fall back to the user's local app data and
    // finally to a hidden folder next to the output.
    let mut candidates = vec![std::env::temp_dir().join(&name)];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("LibreSVIP").join("tmp").join(&name));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        candidates.push(
            PathBuf::from(profile)
                .join("AppData")
                .join("Local")
                .join("LibreSVIP")
                .join("tmp")
                .join(&name),
        );
    }
    if let Some(dir) = fallback_dir {
        candidates.push(dir.join(format!(".libresvip-tmp-{name}")));
    }
    for candidate in &candidates {
        if std::fs::create_dir_all(candidate).is_ok() {
            return candidate.clone();
        }
    }
    // Nothing worked; hand back the first choice so the error names it.
    candidates.remove(0)
}

fn load_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?;
    let image = image.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    Some(ctx.load_texture(
        "libresvip-icon",
        egui::ColorImage::from_rgba_unmultiplied(size, &pixels),
        egui::TextureOptions::LINEAR,
    ))
}

/// `Qt.styleHints.colorScheme`.
///
/// winit already tracks the Windows "app mode" preference and reports changes
/// through `WindowEvent::ThemeChanged`, which egui exposes as
/// `RawInput::system_theme`. Reading it here avoids spawning `reg.exe` (a
/// console window used to flash briefly on every poll, and anti-virus tools
/// dislike a GUI process shelling out to `reg`).
pub fn system_uses_dark_theme(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input
            .raw
            .system_theme
            .map(|theme| theme == egui::Theme::Dark)
            .unwrap_or(false)
    })
}

/// `url_opener.open_path`
pub fn open_path(path: &Path) {
    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("explorer");
        command.arg(path);
        hide_console(&mut command);
        let _ = command.spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    {
        // `rundll32` is a GUI process: unlike `cmd /C start` it never flashes
        // a console window.
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", url]);
        hide_console(&mut command);
        let _ = command.spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// Suppresses the console window a child process would otherwise create
/// (`CREATE_NO_WINDOW`); the GUI has no console of its own in release builds.
#[cfg(windows)]
fn hide_console(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}
