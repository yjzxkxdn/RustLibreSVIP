//! Command implementations, mirroring `libresvip.cli.commands.*`.

use std::path::{Path, PathBuf};

use clap::{Arg, Command};
use libresvip_core::core::config;
use libresvip_core::core::warning_types::take_typed_warnings;
use libresvip_core::model::base::Project;
use libresvip_core::options::{OptionFieldDef, Options};
use libresvip_core::plugin::Middleware;
use libresvip_middlewares::ported_middlewares;
use libresvip_plugins::capabilities::ALL_CAPABILITIES;
use libresvip_plugins::{enabled_plugins, output_suffixes, plugin_by_suffix, supported_suffixes};

use crate::output::{display_width, echo, eecho, stdout_is_terminal, Table};
use crate::prompt::{console_width, prompt_fields, wrap, Prompter};
use crate::translator::{format_msg, t};

const PROGRAM: &str = "libresvip-cli";

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run() -> i32 {
    let matches = build_cli().get_matches();
    match matches.subcommand() {
        Some(("conf", sub)) => match sub.subcommand() {
            Some(("list", _)) => conf_list(),
            Some(("set", set)) => {
                let name = set.get_one::<String>("name").cloned().unwrap_or_default();
                let value = set.get_one::<String>("value").cloned().unwrap_or_default();
                conf_set(&name, &value)
            }
            _ => 0,
        },
        Some(("plugin", sub)) => match sub.subcommand() {
            Some(("list", _)) => plugin_list(),
            Some(("capabilities", caps)) => plugin_capabilities(caps.get_flag("json")),
            Some(("detail", detail)) => {
                let name = detail
                    .get_one::<String>("plugin_name")
                    .cloned()
                    .unwrap_or_default();
                plugin_detail(&name)
            }
            Some(("toggle", toggle)) => {
                let identifier = toggle
                    .get_one::<String>("identifier")
                    .cloned()
                    .unwrap_or_default();
                plugin_toggle(&identifier)
            }
            _ => 0,
        },
        Some(("proj", sub)) => match sub.subcommand() {
            Some(("convert", convert)) => {
                let in_path = path_arg(convert, "in_path");
                let out_path = path_arg(convert, "out_path");
                let strict_warnings = convert.get_flag("strict_warnings");
                let extra_middlewares: Vec<String> = convert
                    .get_many::<String>("middleware")
                    .map(|values| values.cloned().collect())
                    .unwrap_or_default();
                proj_convert(&in_path, &out_path, strict_warnings, &extra_middlewares)
            }
            Some(("split", split)) => {
                let in_path = path_arg(split, "in_path");
                let out_dir = path_arg(split, "out_dir");
                let output_ext = split
                    .get_one::<String>("output_ext")
                    .cloned()
                    .unwrap_or_else(|| "ust".to_owned());
                let max_track_count = split
                    .get_one::<i64>("max_track_count")
                    .copied()
                    .unwrap_or(1);
                proj_split(&in_path, &out_dir, &output_ext, max_track_count)
            }
            Some(("batch", batch)) => {
                let in_dir = path_arg(batch, "in_dir");
                let out_dir = path_arg(batch, "out_dir");
                let to = batch
                    .get_one::<String>("to")
                    .cloned()
                    .unwrap_or_default();
                let strict_warnings = batch.get_flag("strict_warnings");
                proj_batch(&in_dir, &out_dir, &to, strict_warnings)
            }
            Some(("merge", merge)) => {
                let in_paths: Vec<PathBuf> = merge
                    .get_many::<String>("in_paths")
                    .map(|values| values.map(PathBuf::from).collect())
                    .unwrap_or_default();
                let out_path = merge
                    .get_one::<String>("out_path")
                    .cloned()
                    .unwrap_or_default();
                proj_merge(&in_paths, &out_path)
            }
            _ => 0,
        },
        #[cfg(feature = "rpc")]
        Some(("rpc", sub)) => match sub.subcommand() {
            Some(("server", server)) => {
                let host = server
                    .get_one::<String>("host")
                    .cloned()
                    .unwrap_or_else(|| "127.0.0.1".to_owned());
                let port = server.get_one::<u16>("port").copied().unwrap_or(15150);
                crate::rpc::run_server(&host, port)
            }
            _ => 0,
        },
        Some(("mcp", sub)) => match sub.subcommand() {
            Some(("serve", _)) => crate::mcp::run_stdio(),
            _ => 0,
        },
        _ => 0,
    }
}

fn path_arg(matches: &clap::ArgMatches, name: &str) -> PathBuf {
    matches
        .get_one::<String>(name)
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn unimplemented_command(name: &str) -> i32 {
    eecho(format!("{name} is not implemented yet"));
    1
}

// ---------------------------------------------------------------------------
// clap definition (runtime-built so help strings can be translated)
// ---------------------------------------------------------------------------

fn build_cli() -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(PROGRAM)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("install_completion")
                .long("install-completion")
                .num_args(0)
                .help("Install completion for the current shell."),
        )
        .arg(
            Arg::new("show_completion")
                .long("show-completion")
                .num_args(0)
                .help("Show completion for the current shell."),
        )
        .subcommand(
            Command::new("conf")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(Command::new("list"))
                .subcommand(
                    Command::new("set")
                        .override_usage(format!("{PROGRAM} conf set [OPTIONS] NAME VALUE"))
                        .arg(Arg::new("name").required(true))
                        .arg(Arg::new("value").required(true)),
                ),
        )
        .subcommand(
            Command::new("mcp")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("serve")
                        .about("Expose project load, middleware, and save as an MCP server on stdio."),
                ),
        )
        .subcommand(
            Command::new("plugin")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("toggle")
                        .override_usage(format!("{PROGRAM} plugin toggle [OPTIONS] IDENTIFIER"))
                        .arg(Arg::new("identifier").required(true)),
                )
                .subcommand(Command::new("list"))
                .subcommand(
                    Command::new("capabilities").about(
                        t("List what each format can carry (curves, phonemes, vibrato, …)."),
                    )
                    .arg(
                        Arg::new("json")
                            .long("json")
                            .num_args(0)
                            .help(t("Emit machine-readable JSON instead of a table.")),
                    ),
                )
                .subcommand(
                    Command::new("detail")
                        .override_usage(format!("{PROGRAM} plugin detail [OPTIONS] PLUGIN_NAME"))
                        .arg(Arg::new("plugin_name").required(true)),
                ),
        )
        .subcommand(
            Command::new("proj")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("convert")
                        .about(t("Convert a file from one format to another."))
                        .override_usage(format!("{PROGRAM} proj convert [OPTIONS] IN_PATH OUT_PATH"))
                        .arg(Arg::new("in_path").required(true))
                        .arg(Arg::new("out_path").required(true))
                        .arg(
                            Arg::new("strict_warnings")
                                .long("strict-warnings")
                                .num_args(0)
                                .help(t("Fail the conversion if it produced any warning.")),
                        )
                        .arg(
                            Arg::new("middleware")
                                .long("middleware")
                                .num_args(1)
                                .action(clap::ArgAction::Append)
                                .help(t(
                                    "Run an extra opt-in middleware by id (repeatable; see `middleware list`).",
                                )),
                        ),
                )
                .subcommand(
                    Command::new("split")
                        .override_usage(format!("{PROGRAM} proj split [OPTIONS] IN_PATH OUT_DIR"))
                        .arg(Arg::new("in_path").required(true))
                        .arg(Arg::new("out_dir").required(true))
                        .arg(
                            Arg::new("output_ext")
                                .long("output-ext")
                                .num_args(1)
                                .default_value("ust")
                                .help(t("Output format")),
                        )
                        .arg(
                            Arg::new("max_track_count")
                                .long("max-track-count")
                                .num_args(1)
                                .value_parser(clap::value_parser!(i64))
                                .default_value("1")
                                .help(t("Maximum track count per file")),
                        ),
                )
                .subcommand(
                    Command::new("batch")
                        .about(t("Convert every supported project file in a directory tree."))
                        .override_usage(format!("{PROGRAM} proj batch [OPTIONS] IN_DIR OUT_DIR"))
                        .arg(Arg::new("in_dir").required(true))
                        .arg(Arg::new("out_dir").required(true))
                        .arg(
                            Arg::new("to")
                                .long("to")
                                .num_args(1)
                                .required(true)
                                .help(t("Target format, e.g. ust / svp / json")),
                        )
                        .arg(
                            Arg::new("strict_warnings")
                                .long("strict-warnings")
                                .num_args(0)
                                .help(t("Fail each file that produced warnings.")),
                        ),
                )
                .subcommand(
                    Command::new("merge")
                        .override_usage(format!("{PROGRAM} proj merge [OPTIONS] [IN_PATHS]..."))
                        .arg(Arg::new("in_paths").num_args(1..))
                        .arg(
                            Arg::new("out_path")
                                .long("out-path")
                                .num_args(1)
                                .default_value(""),
                        ),
                ),
        );

    // `rpc server` needs tonic + tokio + prost (~2 MiB of code and a 1 MiB icon
    // table); build with `--no-default-features` to leave it out entirely.
    #[cfg(feature = "rpc")]
    {
        command = command.subcommand(
            Command::new("rpc")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("server")
                        .override_usage(format!("{PROGRAM} rpc server [OPTIONS]"))
                        .arg(
                            Arg::new("host")
                                .long("host")
                                .num_args(1)
                                .default_value("127.0.0.1"),
                        )
                        .arg(
                            Arg::new("port")
                                .long("port")
                                .num_args(1)
                                .value_parser(clap::value_parser!(u16))
                                .default_value("15150"),
                        ),
                ),
        );
    }

    command
}

// ---------------------------------------------------------------------------
// Error rendering (typer/rich style for parameter errors)
// ---------------------------------------------------------------------------

fn param_error(param: &str, message: &str, usage: &str, command: &str) -> i32 {
    eprintln!("Usage: {usage}");
    eprintln!("Try '{PROGRAM} {command} --help' for help.");
    let total_width = console_width().saturating_sub(1);
    let inner = total_width.saturating_sub(2);
    let content_width = inner.saturating_sub(2);
    let title = "─ Error ";
    let mut top = String::from("┌");
    top.push_str(title);
    top.push_str(&"─".repeat(inner.saturating_sub(display_width(title))));
    top.push('┐');
    eprintln!("{top}");
    let full = format!("Invalid value for '{param}': {message}");
    for line in wrap(&full, content_width) {
        let padding = content_width.saturating_sub(display_width(&line));
        eprintln!("│ {line}{} │", " ".repeat(padding));
    }
    eprintln!("└{}┘", "─".repeat(inner));
    2
}

fn runtime_error(message: &str) -> i32 {
    eprintln!("Error: {message}");
    1
}

// ---------------------------------------------------------------------------
// conf
// ---------------------------------------------------------------------------

fn conf_list() -> i32 {
    for (name, value) in config::settings().dump_fields() {
        echo(format!("{name}: {value}"));
    }
    0
}

fn conf_set(name: &str, value: &str) -> i32 {
    if name != "language" {
        return param_error(
            "NAME",
            &format_msg(&t("Setting {} is not supported in cli mode."), &[name]),
            &format!("{PROGRAM} conf set [OPTIONS] NAME VALUE"),
            "conf set",
        );
    }
    if let Err(error) = config::set_language(value) {
        return runtime_error(&error.to_string());
    }
    echo(format_msg(&t("Set {} to {}"), &[name, value]));
    0
}

// ---------------------------------------------------------------------------
// plugin
// ---------------------------------------------------------------------------

/// `plugin capabilities` — the format capability matrix, as a table or JSON.
fn plugin_capabilities(json: bool) -> i32 {
    if json {
        let entries: Vec<String> = ALL_CAPABILITIES
            .iter()
            .map(|(id, caps)| {
                let features: Vec<String> = caps
                    .features
                    .iter()
                    .map(|feature| format!("\"{}\"", feature.key()))
                    .collect();
                let max_tracks = caps
                    .max_tracks_per_file
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "null".to_owned());
                format!(
                    "{{\"id\":\"{id}\",\"features\":[{}],\"max_tracks_per_file\":{max_tracks}}}",
                    features.join(",")
                )
            })
            .collect();
        echo(format!("[{}]", entries.join(",")));
        return 0;
    }
    let margin = "  ";
    let headers = vec![
        t("Identifier"),
        t("Features"),
        t("Max tracks/file"),
    ];
    let rows: Vec<Vec<String>> = ALL_CAPABILITIES
        .iter()
        .map(|(id, caps)| {
            let features = if caps.features.is_empty() {
                "-".to_owned()
            } else {
                caps.features
                    .iter()
                    .map(|feature| feature.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let max_tracks = caps
                .max_tracks_per_file
                .map(|count| count.to_string())
                .unwrap_or_else(|| "-".to_owned());
            vec![
                format!("{id}{margin}"),
                format!("{features}{margin}"),
                format!("{max_tracks}{margin}"),
            ]
        })
        .collect();
    let table = Table::new(headers, rows, stdout_is_terminal());
    echo(table.render());
    0
}

fn plugin_list() -> i32 {
    let plugins = enabled_plugins();
    if plugins.is_empty() {
        echo(t("No plugins are currently installed."));
        return 0;
    }
    let margin = "  ";
    let headers = vec![
        t("No."),
        t("Name"),
        t("Version"),
        t("Author"),
        t("Identifier"),
        t("Applicable file format"),
    ];
    let mut rows = Vec::new();
    for (num, plugin) in plugins.iter().enumerate() {
        let info = plugin.info();
        let suffixes_str = info
            .suffixes
            .iter()
            .map(|suffix| format!("*.{suffix}"))
            .collect::<Vec<_>>()
            .join("; ");
        let format_desc = format!("{} ({suffixes_str})", t(info.file_format));
        let version = plugin.version();
        let version = if version.is_empty() { "N/A" } else { version };
        rows.push(vec![
            format!("[{}] ", num + 1),
            format!("{}{margin}", info.name),
            format!("{version}{margin}"),
            format!("{}{margin}", t(info.author)),
            format!("{}{margin}", info.id),
            format!("{format_desc}{margin}"),
        ]);
    }
    let table = Table::new(headers, rows, stdout_is_terminal());
    echo(table.render());
    0
}

fn plugin_detail(plugin_name: &str) -> i32 {
    let Some(plugin) = libresvip_plugins::all_plugins()
        .iter()
        .copied()
        .find(|plugin| plugin.info().id == plugin_name)
    else {
        eecho(format!("{}{}!", t("Cannot find plugin "), plugin_name));
        return 0;
    };
    let info = plugin.info();
    echo("");
    echo("--------------------------------------------------\n");
    echo(format!(
        "{}{}\t{}{}\t{}{}",
        t("Plugin: "),
        info.name,
        t("Version: "),
        plugin.version(),
        t("Author: "),
        t(info.author)
    ));
    if !info.website.is_empty() {
        echo(format!("\n{}{}", t("Website: "), info.website));
    }
    let suffixes_str = info
        .suffixes
        .iter()
        .map(|suffix| format!("*.{suffix}"))
        .collect::<Vec<_>>()
        .join("; ");
    let format_desc = format!("{} ({suffixes_str})", t(info.file_format));
    echo(format!(
        "\n{} {format_desc}.",
        t("This plugin is applicable to")
    ));
    let suffix = info.suffix().to_lowercase();
    echo(format_msg(
        &t("If you want to use this plugin, please specify '-i {}' (input) or '-o {}' (output) when converting."),
        &[&suffix, &suffix],
    ));
    if !info.description.is_empty() {
        echo(format!("\n{}\n{}", t("Description: "), t(info.description)));
    }
    let operations = [t("input"), t("output")];
    let option_sets = [plugin.input_options(), plugin.output_options()];
    for (operation, options) in operations.iter().zip(option_sets.iter()) {
        print_option_fields(operation, options);
    }
    echo("");
    echo("--------------------------------------------------\n");
    0
}

fn print_option_fields(operation: &str, options: &[OptionFieldDef]) {
    echo(format_msg(
        &t("This plugin supports the following {} conversion options:"),
        &[operation],
    ));
    for field in options {
        if matches!(field.kind, libresvip_core::options::OptionKind::Opaque { .. }) {
            // Python's `print_plugin_details` ignores annotations that are not
            // bool/int/float/str/Enum/BaseComplexModel (e.g. Color).
            continue;
        }
        let type_name = field.type_name();
        let description = field.description.map(t).unwrap_or_else(|| "None".to_owned());
        echo(format!(
            "\n  {} = {}    {}",
            t(field.title),
            type_name,
            description
        ));
        let default_repr = field
            .default_repr
            .clone()
            .unwrap_or_else(|| field.default.to_display_string());
        // Python prints the default before the enum values but *after* the
        // "Available fields" block of a BaseComplexModel.
        let is_complex = matches!(
            &field.kind,
            libresvip_core::options::OptionKind::Complex { .. }
        );
        if !is_complex {
            echo(format!("\t{}{}", t("Default: "), default_repr));
        }
        if let libresvip_core::options::OptionKind::Enum { variants, .. } = &field.kind {
            if !field.show_enum_values || variants.is_empty() {
                continue;
            }
            echo(format!("  {}", t("Available values:")));
            for variant in variants.iter() {
                echo(format!("\t{}\t=>\t{}", variant.value, t(variant.title)));
            }
        }
        if let libresvip_core::options::OptionKind::Complex { fields, .. } = &field.kind {
            echo(format!("  {}", t("Available fields:")));
            for inner in fields.iter() {
                echo(format!("    {} = {}", inner.name, inner.type_name));
            }
            echo(format!("\t{}{}", t("Default: "), default_repr));
        }
    }
}

fn plugin_toggle(identifier: &str) -> i32 {
    let known = libresvip_plugins::all_plugins()
        .iter()
        .any(|plugin| plugin.info().id == identifier);
    if known {
        config::update_settings(|settings| {
            settings.disabled_plugins.push(identifier.to_owned());
        });
    } else if config::read_settings(|settings| {
        settings
            .disabled_plugins
            .iter()
            .any(|item| item == identifier)
    }) {
        config::update_settings(|settings| {
            settings.disabled_plugins.retain(|item| item != identifier);
        });
    } else {
        echo(t("Unable to find the plugin."));
        return 0;
    }
    if let Err(error) = config::save_settings() {
        return runtime_error(&error.to_string());
    }
    if known {
        echo(t("The plugin is successfully disabled."));
    } else {
        echo(t("The plugin is successfully enabled."));
    }
    0
}

// ---------------------------------------------------------------------------
// proj helpers
// ---------------------------------------------------------------------------

/// `_extension` for a path.
fn extension_path(value: &Path) -> String {
    value
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// `_extension` for a bare extension string.
fn extension_str(value: &str) -> String {
    let wrapped = format!(".{value}");
    Path::new(&wrapped)
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn python_list(items: &[&str]) -> String {
    let inner = items
        .iter()
        .map(|item| format!("'{item}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

fn unsupported_extension(extension: &str, supported: &[&str]) -> String {
    format_msg(
        &t("Extension {} is not supported. Supported extensions are: {}"),
        &[extension, &python_list(supported)],
    )
}

fn validate_input_path(path: &Path) -> Option<i32> {
    if !path.exists() {
        return Some(param_error(
            "IN_PATH",
            &format!("File '{}' does not exist.", path.display()),
            &format!("{PROGRAM} {CONVERT_USAGE}"),
            "proj convert",
        ));
    }
    if path.is_dir() {
        return Some(param_error(
            "IN_PATH",
            &format!("File '{}' is a directory.", path.display()),
            &format!("{PROGRAM} {CONVERT_USAGE}"),
            "proj convert",
        ));
    }
    None
}

fn prompt_option_sets(
    schema: &[OptionFieldDef],
    prompter: &mut Prompter,
) -> Options {
    if schema.is_empty() {
        return Options::new();
    }
    prompt_fields(schema, prompter, &t)
}

fn prompt_middlewares(prompter: &mut Prompter) -> Vec<(&'static dyn Middleware, Options)> {
    let mut selected = Vec::new();
    // Only the ported middlewares are prompted: the Python CLI asks exactly
    // these six, and `--middleware` is how the Rust-port additions are chosen.
    for middleware in ported_middlewares() {
        let name = t(middleware.info().name);
        let question = format_msg(&t("Enable {} middleware?"), &[&name]);
        if prompter.confirm(&question, false) {
            let schema = middleware.process_options();
            if !schema.is_empty() {
                echo(t("Process Options: "));
            }
            let options = prompt_option_sets(&schema, prompter);
            selected.push((middleware, options));
        }
    }
    selected
}

fn apply_middlewares(
    project: Project,
    middlewares: &[(&'static dyn Middleware, Options)],
) -> Result<Project, String> {
    let mut project = project;
    for (middleware, options) in middlewares {
        project = middleware
            .process(project, options)
            .map_err(|error| error.to_string())?;
    }
    Ok(project)
}

/// `--middleware <id>`: runs one of the opt-in (Rust-port only) middlewares
/// with its default options, after the interactively selected ones.
fn explicit_middlewares(
    ids: &[String],
) -> Result<Vec<(&'static dyn Middleware, Options)>, String> {
    let mut selected = Vec::new();
    for id in ids {
        let Some(middleware) = libresvip_middlewares::middleware_by_id(id) else {
            return Err(format!("unknown middleware: {id}"));
        };
        let options = Options::from_pairs(
            middleware
                .process_options()
                .iter()
                .map(|def| (def.key.to_owned(), def.default.clone())),
        );
        selected.push((middleware, options));
    }
    Ok(selected)
}

// ---------------------------------------------------------------------------
// proj convert
// ---------------------------------------------------------------------------

const CONVERT_USAGE: &str = "proj convert [OPTIONS] IN_PATH OUT_PATH";

fn proj_convert(
    in_path: &Path,
    out_path: &Path,
    strict_warnings: bool,
    extra_middlewares: &[String],
) -> i32 {
    if let Some(code) = validate_input_path(in_path) {
        return code;
    }
    let input_ext = extension_path(in_path);
    let Some(input_plugin) = plugin_by_suffix(&input_ext) else {
        let supported = supported_suffixes();
        return param_error(
            "IN_PATH",
            &unsupported_extension(&input_ext, &supported),
            &format!("{PROGRAM} {CONVERT_USAGE}"),
            "proj convert",
        );
    };
    if !input_plugin.readable() {
        return runtime_error("plugin does not support reading");
    }
    let output_ext = extension_path(out_path);
    let Some(output_plugin) = plugin_by_suffix(&output_ext) else {
        let supported = output_suffixes();
        return param_error(
            "OUT_PATH",
            &unsupported_extension(&output_ext, &supported),
            &format!("{PROGRAM} {CONVERT_USAGE}"),
            "proj convert",
        );
    };
    if output_plugin.info().suffix() != output_ext {
        let supported = output_suffixes();
        return param_error(
            "OUT_PATH",
            &unsupported_extension(&output_ext, &supported),
            &format!("{PROGRAM} {CONVERT_USAGE}"),
            "proj convert",
        );
    }
    if !output_plugin.writable() {
        return runtime_error("plugin does not support writing");
    }

    let mut prompter = Prompter::new();
    let input_schema = input_plugin.input_options();
    if !input_schema.is_empty() {
        echo(t("Input Options: "));
    }
    let input_options = prompt_option_sets(&input_schema, &mut prompter);

    let project = match input_plugin.load(in_path, &input_options) {
        Ok(project) => project,
        Err(error) => return runtime_error(&error.to_string()),
    };
    // Warnings raised while parsing travel with the project from here on.
    let mut project = project;
    project.warnings.extend(take_typed_warnings());

    let mut middlewares = prompt_middlewares(&mut prompter);
    match explicit_middlewares(extra_middlewares) {
        Ok(mut extra) => middlewares.append(&mut extra),
        Err(message) => return runtime_error(&message),
    }
    let project = match apply_middlewares(project, &middlewares) {
        Ok(project) => project,
        Err(message) => return runtime_error(&message),
    };

    let output_schema = output_plugin.output_options();
    if !output_schema.is_empty() {
        echo(t("Output Options: "));
    }
    let output_options = prompt_option_sets(&output_schema, &mut prompter);
    let dump_result = output_plugin.dump(out_path, &project, &output_options);
    let late_warnings = take_typed_warnings();
    match dump_result {
        Ok(()) => {
            if strict_warnings
                && (!project.warnings.is_empty() || !late_warnings.is_empty())
            {
                return runtime_error(
                    "the conversion produced warnings and --strict-warnings is active",
                );
            }
            0
        }
        Err(error) => runtime_error(&error.to_string()),
    }
}

// ---------------------------------------------------------------------------
// proj split
// ---------------------------------------------------------------------------

fn proj_split(in_path: &Path, out_dir: &Path, output_ext: &str, max_track_count: i64) -> i32 {
    if let Some(code) = validate_input_path(in_path) {
        return code;
    }
    let input_ext = extension_path(in_path);
    let Some(input_plugin) = plugin_by_suffix(&input_ext) else {
        let supported = supported_suffixes();
        return param_error(
            "IN_PATH",
            &unsupported_extension(&input_ext, &supported),
            &format!("{PROGRAM} proj split [OPTIONS] IN_PATH OUT_DIR"),
            "proj split",
        );
    };
    let normalized_ext = extension_str(output_ext);
    let Some(output_plugin) = plugin_by_suffix(&normalized_ext) else {
        let supported = output_suffixes();
        return param_error(
            "OUTPUT_EXT",
            &unsupported_extension(&normalized_ext, &supported),
            &format!("{PROGRAM} proj split [OPTIONS] IN_PATH OUT_DIR"),
            "proj split",
        );
    };
    if output_plugin.info().suffix() != normalized_ext {
        let supported = output_suffixes();
        return param_error(
            "OUTPUT_EXT",
            &unsupported_extension(&normalized_ext, &supported),
            &format!("{PROGRAM} proj split [OPTIONS] IN_PATH OUT_DIR"),
            "proj split",
        );
    }

    let mut prompter = Prompter::new();
    let input_schema = input_plugin.input_options();
    if !input_schema.is_empty() {
        echo(t("Input Options: "));
    }
    let input_options = prompt_option_sets(&input_schema, &mut prompter);
    let root_project = match input_plugin.load(in_path, &input_options) {
        Ok(project) => project,
        Err(error) => return runtime_error(&error.to_string()),
    };
    let mut root_project = root_project;
    root_project.warnings.extend(take_typed_warnings());
    let sub_projects = match root_project.split_tracks(max_track_count.max(1) as usize) {
        Ok(projects) => projects,
        Err(error) => return runtime_error(&error.to_string()),
    };
    let middlewares = prompt_middlewares(&mut prompter);
    let output_schema = output_plugin.output_options();
    if !output_schema.is_empty() {
        echo(t("Output Options: "));
    }
    let output_options = prompt_option_sets(&output_schema, &mut prompter);
    let stem = in_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    for (index, project) in sub_projects.into_iter().enumerate() {
        let project = match apply_middlewares(project, &middlewares) {
            Ok(project) => project,
            Err(message) => return runtime_error(&message),
        };
        let file_name = format!("{stem}_{:02}.{output_ext}", index + 1);
        let target = out_dir.join(file_name);
        if let Err(error) = output_plugin.dump(&target, &project, &output_options) {
            return runtime_error(&error.to_string());
        }
    }
    0
}

// ---------------------------------------------------------------------------
// proj merge
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// proj batch (directory tree conversion)
// ---------------------------------------------------------------------------

/// Recursively collects files below `root`, sorted for stable output.
fn walk_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(|entry| entry.ok()).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn proj_batch(in_dir: &Path, out_dir: &Path, to: &str, strict_warnings: bool) -> i32 {
    if !in_dir.is_dir() {
        return param_error(
            "IN_DIR",
            &format!("'{}' is not a directory.", in_dir.display()),
            &format!("{PROGRAM} proj batch [OPTIONS] IN_DIR OUT_DIR"),
            "proj batch",
        );
    }
    let target_ext = extension_str(to);
    let Some(output_plugin) = plugin_by_suffix(&target_ext) else {
        let supported = output_suffixes();
        return param_error(
            "TO",
            &unsupported_extension(&target_ext, &supported),
            &format!("{PROGRAM} proj batch [OPTIONS] IN_DIR OUT_DIR"),
            "proj batch",
        );
    };
    if output_plugin.info().suffix() != target_ext || !output_plugin.writable() {
        return runtime_error("plugin does not support writing");
    }
    if let Err(error) = std::fs::create_dir_all(out_dir) {
        return runtime_error(&format!("cannot create {}: {error}", out_dir.display()));
    }

    let mut files = Vec::new();
    walk_files(in_dir, &mut files);
    let input_default = |plugin: &'static dyn libresvip_core::plugin::SvsPlugin| {
        Options::from_pairs(
            plugin
                .input_options()
                .into_iter()
                .map(|def| (def.key.to_owned(), def.default.clone())),
        )
    };
    let output_options = Options::from_pairs(
        output_plugin
            .output_options()
            .into_iter()
            .map(|def| (def.key.to_owned(), def.default.clone())),
    );

    let mut converted = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    for source in &files {
        let ext = extension_path(source);
        let Some(input_plugin) = plugin_by_suffix(&ext) else {
            skipped += 1;
            continue;
        };
        if !input_plugin.readable() {
            skipped += 1;
            continue;
        }
        let relative = source.strip_prefix(in_dir).unwrap_or(source);
        let target = out_dir.join(relative).with_extension(&target_ext);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let load_result = input_plugin.load(source, &input_default(input_plugin));
        let project = match load_result {
            Ok(project) => project,
            Err(error) => {
                failed += 1;
                eecho(format!("{}: {}", source.display(), error));
                continue;
            }
        };
        let mut project = project;
        project.warnings.extend(take_typed_warnings());
        let had_warnings = !project.warnings.is_empty();
        match output_plugin.dump(&target, &project, &output_options) {
            Ok(()) => {
                let late = take_typed_warnings();
                if strict_warnings && (had_warnings || !late.is_empty()) {
                    failed += 1;
                    let _ = std::fs::remove_file(&target);
                    eecho(format!(
                        "{}: warnings are errors (--strict-warnings)",
                        source.display()
                    ));
                } else {
                    converted += 1;
                }
            }
            Err(error) => {
                failed += 1;
                let _ = take_typed_warnings();
                eecho(format!("{}: {}", source.display(), error));
            }
        }
    }
    echo(format!(
        "{} converted, {} failed, {} skipped (unsupported extensions)",
        converted, failed, skipped
    ));
    i32::from(failed > 0)
}

fn proj_merge(in_paths: &[PathBuf], out_path: &str) -> i32 {
    let mut prompter = Prompter::new();
    let middlewares = prompt_middlewares(&mut prompter);
    let mut projects = Vec::new();
    for in_path in in_paths {
        echo(in_path.display().to_string());
        let input_ext = extension_path(in_path);
        let Some(input_plugin) = plugin_by_suffix(&input_ext) else {
            let supported = supported_suffixes();
            return param_error(
                "IN_PATH",
                &unsupported_extension(&input_ext, &supported),
                &format!("{PROGRAM} proj merge [OPTIONS] [IN_PATHS]..."),
                "proj merge",
            );
        };
        let input_schema = input_plugin.input_options();
        if !input_schema.is_empty() {
            echo(t("Input Options: "));
        }
        let input_options = prompt_option_sets(&input_schema, &mut prompter);
        let project = match input_plugin.load(in_path, &input_options) {
            Ok(project) => project,
            Err(error) => return runtime_error(&error.to_string()),
        };
        let mut project = project;
        project.warnings.extend(take_typed_warnings());
        match apply_middlewares(project, &middlewares) {
            Ok(project) => projects.push(project),
            Err(message) => return runtime_error(&message),
        }
    }
    let output_ext = extension_str(out_path);
    let Some(output_plugin) = plugin_by_suffix(&output_ext) else {
        let supported = output_suffixes();
        return param_error(
            "OUT_PATH",
            &unsupported_extension(&output_ext, &supported),
            &format!("{PROGRAM} proj merge [OPTIONS] [IN_PATHS]..."),
            "proj merge",
        );
    };
    if output_plugin.info().suffix() != output_ext {
        let supported = output_suffixes();
        return param_error(
            "OUT_PATH",
            &unsupported_extension(&output_ext, &supported),
            &format!("{PROGRAM} proj merge [OPTIONS] [IN_PATHS]..."),
            "proj merge",
        );
    }
    if projects.is_empty() {
        eecho(t("Invalid options"));
        return 0;
    }
    let project = match Project::merge_projects(projects) {
        Ok(project) => project,
        Err(error) => return runtime_error(&error.to_string()),
    };
    let output_schema = output_plugin.output_options();
    if !output_schema.is_empty() {
        echo(t("Output Options: "));
    }
    let output_options = prompt_option_sets(&output_schema, &mut prompter);
    match output_plugin.dump(Path::new(out_path), &project, &output_options) {
        Ok(()) => 0,
        Err(error) => runtime_error(&error.to_string()),
    }
}
