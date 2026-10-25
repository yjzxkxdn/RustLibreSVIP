//! Modal windows: about, settings, error details, overwrite prompt, format info.

use eframe::egui::{
    self, Align, Align2, Layout, Rect, RichText, ScrollArea, Sense, Ui, Vec2,
};
use libresvip_core::core::config::{ConflictPolicy, DarkMode};

use crate::app::{open_url, App, SettingsTab};
use crate::i18n::Lang;
use crate::state::FormatEntry;
use crate::theme::Palette;
use crate::ui::titlebar::{
    AUTHOR_URL, DOCS_URL, LIBRESVIP_AUTHOR_LABEL, LIBRESVIP_AUTHOR_URL, LIBRESVIP_URL,
    PROJECT_AUTHOR, PROJECT_GITHUB_URL, REPO_URL, UTAFORMATIX3_AUTHOR_URL, UTAFORMATIX3_URL,
};
use crate::widgets;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn show(app: &mut App, ctx: &egui::Context) {
    about(app, ctx);
    settings(app, ctx);
    detail(app, ctx);
    conflict(app, ctx);
    format_info(app, ctx);
    notice(app, ctx);
    toast(app, ctx);
}

fn about(app: &mut App, ctx: &egui::Context) {
    if !app.show_about {
        return;
    }
    let pal = app.pal;
    let title = app.t("About");
    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.horizontal(|ui| {
                if let Some(texture) = &app.app_icon {
                    ui.add(
                        egui::Image::from_texture(egui::load::SizedTexture::new(
                            texture.id(),
                            Vec2::splat(64.0),
                        ))
                        .fit_to_exact_size(Vec2::splat(64.0)),
                    );
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new("RustLibreSVIP").size(24.0).strong());
                    ui.label(
                        RichText::new(app.t(
                            "Rust implementation based on LibreSVIP 2.9.2 and UtaFormatix3",
                        ))
                        .size(Palette::body())
                        .color(pal.muted),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{}{VERSION} (LibreSVIP {} + UtaFormatix3 {})",
                            app.t("Version: "),
                            libresvip_core::VERSION,
                            libresvip_core::UTAFORMATIX3_REVISION
                        ))
                            .size(Palette::body()),
                    );
                    ui.label(
                        RichText::new(app.t(
                            "Based on LibreSVIP 2.9.2 (MIT) and UtaFormatix3 (Apache-2.0; design and algorithms only)",
                        ))
                            .size(Palette::body())
                            .color(pal.muted),
                    );
                    ui.add_space(2.0);
                    // One credit line per project: this one, LibreSVIP, UtaFormatix3.
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(app.t("Project author: "))
                                .size(Palette::body())
                                .color(pal.muted),
                        );
                        ui.label(RichText::new(PROJECT_AUTHOR).size(Palette::body()));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("LibreSVIP 2.9.2 · MIT · ")
                                .size(Palette::body())
                                .color(pal.muted),
                        );
                        if ui
                            .link(RichText::new(LIBRESVIP_AUTHOR_LABEL).size(Palette::body()))
                            .clicked()
                        {
                            open_url(LIBRESVIP_AUTHOR_URL);
                        }
                        ui.label(
                            RichText::new(format!(
                                " · {}",
                                app.t("format plugins and the hub model")
                            ))
                            .size(Palette::body())
                            .color(pal.muted),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "UtaFormatix3 {} · Apache-2.0 · ",
                                libresvip_core::UTAFORMATIX3_REVISION
                            ))
                            .size(Palette::body())
                            .color(pal.muted),
                        );
                        if ui.link(RichText::new("SderColin").size(Palette::body())).clicked() {
                            open_url(UTAFORMATIX3_AUTHOR_URL);
                        }
                        ui.label(
                            RichText::new(format!(
                                " · {}",
                                app.t("capability matrix, structured warnings and pitch semantics")
                            ))
                            .size(Palette::body())
                            .color(pal.muted),
                        );
                    });
                });
            });
            ui.add_space(8.0);
            ui.label(app.t("One engine for 43 singing synthesis formats: LibreSVIP's format plugins and hub model, plus UtaFormatix3's capability matrix, structured warnings and pitch semantics."));
            ui.add_space(6.0);
            ui.label(app.t("All people should have the right and freedom to choose. That's why we're committed to giving you a second chance to keep your creations free from the constraints of platforms and coterie."));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(app.t("This project:"))
                        .size(Palette::body())
                        .color(pal.muted),
                );
                if ui.link(app.t("Author's Profile")).clicked() {
                    open_url(AUTHOR_URL);
                }
                if ui.link("GitHub").clicked() {
                    open_url(PROJECT_GITHUB_URL);
                }
                if ui.link(app.t("Repo URL")).clicked() {
                    open_url(REPO_URL);
                }
                if ui.link(app.t("Documentation (F1)")).clicked() {
                    open_url(DOCS_URL);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::outlined_button(
                        ui,
                        &pal,
                        &app.t("Close"),
                        Vec2::new(90.0, 32.0),
                        true,
                        true,
                    )
                    .clicked()
                    {
                        app.show_about = false;
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(app.t("Upstreams:"))
                        .size(Palette::body())
                        .color(pal.muted),
                );
                if ui.link("LibreSVIP").clicked() {
                    open_url(LIBRESVIP_URL);
                }
                if ui.link("UtaFormatix3").clicked() {
                    open_url(UTAFORMATIX3_URL);
                }
            });
        });
    if !open {
        app.show_about = false;
    }
}

fn settings(app: &mut App, ctx: &egui::Context) {
    if !app.show_settings {
        return;
    }
    let pal = app.pal;
    let title = app.t("Settings");
    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(660.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    for tab in [
                        SettingsTab::Basic,
                        SettingsTab::SavePath,
                        SettingsTab::Plugins,
                        SettingsTab::Updates,
                    ] {
                        let selected = app.settings_tab == tab;
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(170.0, 34.0),
                            Sense::click(),
                        );
                        if selected {
                            ui.painter().rect_filled(
                                rect,
                                0.0,
                                pal.primary.gamma_multiply(0.14),
                            );
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 0.0, pal.hover);
                        }
                        let label = crate::ui::titlebar::settings_tab_name(app, tab);
                        ui.painter().text(
                            egui::pos2(rect.left() + 12.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            egui::FontId::proportional(Palette::body()),
                            if selected { pal.primary } else { pal.text },
                        );
                        if response.clicked() {
                            app.settings_tab = tab;
                        }
                    }
                });
                widgets::vertical_separator(ui, &pal, 320.0);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(440.0);
                    ui.set_min_height(360.0);
                    match app.settings_tab {
                        SettingsTab::Basic => basic_tab(app, ui, &pal),
                        SettingsTab::SavePath => save_path_tab(app, ui, &pal),
                        SettingsTab::Plugins => plugins_tab(app, ui, &pal),
                        SettingsTab::Updates => updates_tab(app, ui, &pal),
                    }
                });
            });
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::outlined_button(
                    ui,
                    &pal,
                    &app.t("OK"),
                    Vec2::new(90.0, 32.0),
                    true,
                    true,
                )
                .clicked()
                {
                    app.show_settings = false;
                }
            });
        });
    if !open {
        app.show_settings = false;
    }
}

fn basic_tab(app: &mut App, ui: &mut Ui, pal: &Palette) {
    ui.label(RichText::new(app.t("Conversion Settings")).size(18.0));
    ui.add_space(4.0);
    let mut auto_detect = app.settings.auto_detect_input_format;
    let label = app.t("Auto-Detect Input File Type");
    if widgets::labelled_switch(ui, pal, &label, &mut auto_detect, Palette::body()).changed() {
        app.settings.auto_detect_input_format = auto_detect;
        app.persist_settings();
    }
    let mut reset_tasks = app.settings.reset_tasks_on_input_change;
    let label = app.t("Reset Task List When Changing Input File Type");
    if widgets::labelled_switch(ui, pal, &label, &mut reset_tasks, Palette::body()).changed() {
        app.settings.reset_tasks_on_input_change = reset_tasks;
        app.persist_settings();
    }
    let mut auto_ext = app.settings.auto_set_output_extension;
    let label = app.t("Set Output File Extension Automatically");
    if widgets::labelled_switch(ui, pal, &label, &mut auto_ext, Palette::body()).changed() {
        app.settings.auto_set_output_extension = auto_ext;
        let ext = app.output_ext();
        for task in &mut app.tasks {
            task.ext = ext.clone();
        }
        app.persist_settings();
    }
    let mut multi = app.settings.multi_threaded_conversion;
    let label = app.t("Multi-Threaded Conversion");
    if widgets::labelled_switch(ui, pal, &label, &mut multi, Palette::body()).changed() {
        app.settings.multi_threaded_conversion = multi;
        app.persist_settings();
    }
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    // Theme
    ui.horizontal(|ui| {
        ui.label(format!("{}：", app.t("Theme")));
        for (mode, label) in [
            (DarkMode::Light, app.t("Light")),
            (DarkMode::Dark, app.t("Dark")),
            (DarkMode::System, app.t("System")),
        ] {
            let selected = app.settings.dark_mode == mode;
            if widgets::chip(ui, pal, &label, selected).clicked() && !selected {
                let ctx = ui.ctx().clone();
                app.set_theme(mode, &ctx);
            }
        }
    });
    // Language
    ui.horizontal(|ui| {
        ui.label(format!("{}：", app.t("Language")));
        for lang in Lang::all() {
            let selected = app.lang == lang;
            if widgets::chip(ui, pal, lang.native_name(), selected).clicked() && !selected {
                app.set_language(lang);
            }
        }
    });
}

fn save_path_tab(app: &mut App, ui: &mut Ui, pal: &Palette) {
    ui.label(RichText::new(app.t("Output Settings")).size(18.0));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let label = app.t("Output Folder");
        let response = widgets::text_field(
            ui,
            pal,
            ui.id().with("settings-save-folder"),
            &label,
            &mut app.save_folder,
            Vec2::new(ui.available_width() - 100.0, 40.0),
        );
        if response.lost_focus() {
            app.apply_save_folder();
        }
        if widgets::outlined_button(
            ui,
            pal,
            &app.t("Browse ..."),
            Vec2::new(90.0, 40.0),
            true,
            false,
        )
        .clicked()
        {
            app.choose_folder();
        }
    });
    if app.save_folder_invalid {
        ui.label(
            RichText::new(app.t("The path does not exist or is not a directory."))
                .color(pal.error)
                .size(Palette::small()),
        );
    }
    ui.add_space(6.0);

    let mut policy = app.settings.conflict_policy;
    ui.horizontal(|ui| {
        ui.label(format!("{}：", app.t("Deal with Conflicts")));
        for (value, label) in [
            (ConflictPolicy::Prompt, app.t("Prompt")),
            (ConflictPolicy::Overwrite, app.t("Overwrite")),
            (ConflictPolicy::Skip, app.t("Skip")),
        ] {
            if widgets::chip(ui, pal, &label, policy == value).clicked() {
                policy = value;
            }
        }
    });
    if policy != app.settings.conflict_policy {
        app.settings.conflict_policy = policy;
        app.persist_settings();
    }
    let mut open_folder = app.settings.open_save_folder_on_completion;
    let label = app.t("Open Output Folder When Done");
    if widgets::labelled_switch(ui, pal, &label, &mut open_folder, Palette::body()).changed() {
        app.settings.open_save_folder_on_completion = open_folder;
        app.persist_settings();
    }

    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new(app.t("Folder Presets")).size(16.0));
    let presets = app.settings.folder_presets.clone();
    let mut remove: Option<usize> = None;
    for (index, preset) in presets.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(preset.to_string_lossy());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::icon_button(ui, pal, crate::icons::Icon::Trash, 26.0, None, true)
                    .clicked()
                {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        app.settings.folder_presets.remove(index);
        app.persist_settings();
    }
    if widgets::outlined_button(
        ui,
        pal,
        &app.t("Click to add a folder preset"),
        Vec2::new(ui.available_width(), 32.0),
        true,
        false,
    )
    .clicked()
    {
        app.settings.folder_presets.push(std::path::PathBuf::from(&app.save_folder));
        app.persist_settings();
    }
}

fn plugins_tab(app: &mut App, ui: &mut Ui, pal: &Palette) {
    ui.label(RichText::new(app.t("Format Provider Plugins")).size(18.0));
    ui.add_space(4.0);
    let entries = app.plugin_entries();
    ScrollArea::vertical()
        .max_height(330.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in &entries {
                ui.horizontal(|ui| {
                    let mut enabled = !app
                        .settings
                        .disabled_plugins
                        .iter()
                        .any(|plugin| plugin == entry.id);
                    if widgets::switch(ui, pal, &mut enabled).changed() {
                        app.toggle_plugin(entry.id);
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} (*.{})",
                            app.td(entry.file_format),
                            entry.suffix
                        ))
                        .size(Palette::body()),
                    );
                });
            }
        });
}

fn updates_tab(app: &mut App, ui: &mut Ui, pal: &Palette) {
    ui.label(RichText::new(app.t("Updates Settings")).size(18.0));
    ui.add_space(4.0);
    let mut value = app.settings.auto_check_for_updates;
    let label = app.t("Auto Check for Updates");
    if widgets::labelled_switch(ui, pal, &label, &mut value, Palette::body()).changed() {
        app.settings.auto_check_for_updates = value;
        app.persist_settings();
    }
    ui.add_space(8.0);
    if widgets::outlined_button(
        ui,
        pal,
        &app.t("Check for Updates"),
        Vec2::new(160.0, 34.0),
        true,
        false,
    )
    .clicked()
    {
        app.update_notice = Some(
            app.t("This build of RustLibreSVIP cannot check for updates online."),
        );
    }
}

fn detail(app: &mut App, ctx: &egui::Context) {
    let Some((heading, body)) = app.detail.clone() else {
        return;
    };
    let pal = app.pal;
    let title = heading.clone();
    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(Vec2::new(680.0, 340.0))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(600.0);
            ui.add_space(4.0);
            ScrollArea::vertical()
                .max_height(220.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(body.as_str())
                                .monospace()
                                .size(Palette::body() * 0.95),
                        )
                        .selectable(true)
                        .wrap(),
                    );
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::outlined_button(
                    ui,
                    &pal,
                    &app.t("Copy error message"),
                    Vec2::new(180.0, 32.0),
                    true,
                    false,
                )
                .clicked()
                {
                    ui.ctx().copy_text(body.clone());
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::outlined_button(
                        ui,
                        &pal,
                        &app.t("Close"),
                        Vec2::new(90.0, 32.0),
                        true,
                        true,
                    )
                    .clicked()
                    {
                        app.detail = None;
                    }
                });
            });
        });
    if !open {
        app.detail = None;
    }
}

fn conflict(app: &mut App, ctx: &egui::Context) {
    let Some((path, target)) = app
        .conflict
        .as_ref()
        .map(|conflict| (conflict.path.clone(), conflict.targets.first().map(|(_, target)| target.clone())))
    else {
        return;
    };
    let pal = app.pal;
    let title = app.t("Do you want to overwrite the file?");
    let mut answer: Option<(bool, bool)> = None;
    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(420.0);
            let target = target
                .as_ref()
                .map(|target| target.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.label(RichText::new(format!("{target} already exists. Overwrite?"))
                .size(Palette::body() * 1.1));
            ui.add_space(4.0);
            ui.label(
                RichText::new(path.to_string_lossy())
                    .size(Palette::small())
                    .color(pal.muted),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::outlined_button(ui, &pal, &app.t("Yes"), Vec2::new(80.0, 32.0), true, true)
                    .clicked()
                {
                    answer = Some((true, false));
                }
                if widgets::outlined_button(ui, &pal, &app.t("No"), Vec2::new(80.0, 32.0), true, false)
                    .clicked()
                {
                    answer = Some((false, false));
                }
                if widgets::outlined_button(
                    ui,
                    &pal,
                    &app.t("Yes to All"),
                    Vec2::new(110.0, 32.0),
                    true,
                    false,
                )
                .clicked()
                {
                    answer = Some((true, true));
                }
                if widgets::outlined_button(
                    ui,
                    &pal,
                    &app.t("No to All"),
                    Vec2::new(110.0, 32.0),
                    true,
                    false,
                )
                .clicked()
                {
                    answer = Some((false, true));
                }
            });
        });
    if let Some((overwrite, all)) = answer {
        app.answer_conflict(overwrite, all);
    } else if !open {
        app.answer_conflict(false, false);
    }
}

fn format_info(app: &mut App, ctx: &egui::Context) {
    if !app.input_info_open && !app.output_info_open {
        return;
    }
    let is_input = app.input_info_open;
    let pal = app.pal;
    let entry: Option<FormatEntry> = if is_input {
        app.input_entry().cloned()
    } else {
        app.output_entry().cloned()
    };
    let title = entry
        .as_ref()
        .map(|entry| app.td(entry.name))
        .unwrap_or_default();
    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(380.0);
            if let Some(entry) = &entry {
                ui.label(RichText::new(app.td(entry.file_format)).size(17.0).strong());
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("({})", entry.suffixes_display()))
                        .size(Palette::small())
                        .color(pal.muted),
                );
                ui.add_space(8.0);
                ui.label(app.td(entry.description));
                // Capability matrix row (see libresvip_plugins::capabilities).
                if let Some(caps) = libresvip_plugins::capabilities::capabilities_by_id(entry.id) {
                    let mut lines: Vec<String> = Vec::new();
                    if !caps.features.is_empty() {
                        let features = caps
                            .features
                            .iter()
                            .map(|feature| feature.description())
                            .collect::<Vec<_>>()
                            .join(", ");
                        lines.push(format!("{} {}", app.t("Supports:"), features));
                    }
                    if caps.max_tracks_per_file == Some(1) {
                        lines.push(app.t("One track per output file.").to_owned());
                    }
                    if !lines.is_empty() {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(lines.join("\n"))
                                .size(Palette::small())
                                .color(pal.muted),
                        );
                    }
                }
                ui.add_space(8.0);
                // Provenance instead of a bare version number: this plugin is a
                // port of a plugin that had its own version (not the LibreSVIP
                // release number), and any deliberate deviation is listed below.
                ui.label(format!("{}{}", app.t("Version: "), entry.version));
                ui.label(format!(
                    "{}{} {}",
                    app.t("Migrated from: "),
                    app.t("original plugin"),
                    entry.version
                ));
                ui.label(format!("{}{}", app.t("Author: "), entry.author));
                if ui.link(entry.website).clicked() {
                    open_url(entry.website);
                }
                let deviations = crate::deviations::for_plugin(entry.id);
                ui.add_space(6.0);
                if deviations.is_empty() {
                    ui.label(
                        RichText::new(app.t("Modifications: none (plain port)"))
                            .size(Palette::small())
                            .color(pal.muted),
                    );
                } else {
                    ui.label(
                        RichText::new(format!(
                            "{}{}",
                            app.t("Modifications: "),
                            deviations.len()
                        ))
                        .size(Palette::small())
                        .color(pal.muted),
                    );
                    for deviation in deviations {
                        ui.label(
                            RichText::new(format!("· {}", app.t(deviation)))
                                .size(Palette::small())
                                .color(pal.muted),
                        );
                    }
                }
            }
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::outlined_button(
                    ui,
                    &pal,
                    &app.t("Close"),
                    Vec2::new(90.0, 32.0),
                    true,
                    true,
                )
                .clicked()
                {
                    app.input_info_open = false;
                    app.output_info_open = false;
                }
            });
        });
    if !open {
        app.input_info_open = false;
        app.output_info_open = false;
    }
}

fn notice(app: &mut App, ctx: &egui::Context) {
    if app.update_notice.is_none() {
        return;
    }
    let message = app.update_notice.clone().unwrap_or_default();
    let mut close = false;
    egui::Window::new(app.t("Check for Updates"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(message);
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(app.t("OK")).clicked() {
                    close = true;
                }
            });
        });
    if close {
        app.update_notice = None;
    }
}

fn toast(app: &mut App, ctx: &egui::Context) {
    let Some((message, _)) = app.toast.clone() else {
        return;
    };
    let pal = app.pal;
    egui::Area::new(egui::Id::new("toast"))
        .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(pal.text)
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_max_width(560.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(message)
                                .color(pal.surface)
                                .size(Palette::body()),
                        )
                        .wrap(),
                    );
                });
        });
}

fn _unused(_: &Rect) {}
