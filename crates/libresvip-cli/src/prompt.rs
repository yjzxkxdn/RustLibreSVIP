//! Interactive prompts, mirroring `rich.prompt` and
//! `libresvip.cli.prompt.prompt_fields`.

use std::io::{BufRead, Write};

use libresvip_core::core::config;
use libresvip_core::options::{OptionFieldDef, OptionKind, OptionValue, Options};
use libresvip_core::utils::text::supported_charset_names;

pub use crate::output::console_width;
use crate::output::display_width;
use crate::output::wrap_fold;

/// Word wrap matching `rich`'s default (break at spaces, hard-break long words).
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for piece in text.split('\n') {
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        for word in piece.split(' ') {
            let word_width = display_width(word);
            if word_width == 0 {
                continue;
            }
            if current.is_empty() {
                let mut remainder = word;
                while display_width(remainder) > width {
                    // Hard-break a word that cannot fit on a single line.
                    let mut chunk = String::new();
                    let mut chunk_width = 0usize;
                    let mut taken = 0usize;
                    for character in remainder.chars() {
                        let character_width = display_width(&character.to_string());
                        if chunk_width + character_width > width {
                            break;
                        }
                        chunk.push(character);
                        chunk_width += character_width;
                        taken += character.len_utf8();
                    }
                    lines.push(chunk);
                    remainder = &remainder[taken..];
                }
                current.push_str(remainder);
                current_width = display_width(&current);
            } else if current_width + 1 + word_width <= width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
            } else {
                lines.push(std::mem::take(&mut current));
                current_width = 0;
                if word_width > width {
                    let mut remainder = word;
                    while display_width(remainder) > width {
                        let mut chunk = String::new();
                        let mut chunk_width = 0usize;
                        let mut taken = 0usize;
                        for character in remainder.chars() {
                            let character_width = display_width(&character.to_string());
                            if chunk_width + character_width > width {
                                break;
                            }
                            chunk.push(character);
                            chunk_width += character_width;
                            taken += character.len_utf8();
                        }
                        if taken == 0 {
                            break;
                        }
                        lines.push(chunk);
                        remainder = &remainder[taken..];
                    }
                    current.push_str(remainder);
                    current_width = display_width(&current);
                } else {
                    current.push_str(word);
                    current_width = word_width;
                }
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}
/// Interactive prompt reader.
pub struct Prompter {
    stdin: std::io::Stdin,
}

impl Default for Prompter {
    fn default() -> Self {
        Self::new()
    }
}

impl Prompter {
    pub fn new() -> Self {
        Self {
            stdin: std::io::stdin(),
        }
    }

    fn read_line(&mut self) -> Option<String> {
        let mut buffer = String::new();
        let mut lock = self.stdin.lock();
        match lock.read_line(&mut buffer) {
            Ok(0) => None,
            Ok(_) => Some(buffer.trim_end_matches(['\r', '\n']).to_owned()),
            Err(_) => None,
        }
    }

    /// `rich.Prompt.ask(prompt, choices=..., default=...)`.
    pub fn ask(&mut self, prompt: &str, choices: Option<&[&str]>, default: Option<&str>) -> String {
        loop {
            self.render_prompt(prompt, choices, default);
            let Some(answer) = self.read_line() else {
                return default.unwrap_or("").to_owned();
            };
            let answer = answer.trim().to_owned();
            if answer.is_empty() {
                if let Some(default) = default {
                    return default.to_owned();
                }
                self.invalid("Please enter a value");
                continue;
            }
            if let Some(choices) = choices {
                if choices.iter().any(|choice| *choice == answer) {
                    return answer;
                }
                self.invalid("Please select one of the available options");
                continue;
            }
            return answer;
        }
    }

    /// `rich.Confirm.ask(prompt, default=...)`.
    pub fn confirm(&mut self, prompt: &str, default: bool) -> bool {
        let default_text = if default { "y" } else { "n" };
        loop {
            self.render_prompt(prompt, Some(&["y", "n"]), Some(default_text));
            let Some(answer) = self.read_line() else {
                return default;
            };
            let answer = answer.trim().to_ascii_lowercase();
            if answer.is_empty() {
                return default;
            }
            match answer.as_str() {
                "y" | "yes" | "t" | "true" | "on" | "1" => return true,
                "n" | "no" | "f" | "false" | "off" | "0" => return false,
                _ => self.invalid("Please enter Y or N"),
            }
        }
    }

    /// `rich.IntPrompt.ask`.
    pub fn ask_int(&mut self, prompt: &str, default: Option<i64>) -> i64 {
        let default_text = default.map(|value| value.to_string());
        loop {
            self.render_prompt(prompt, None, default_text.as_deref());
            let Some(answer) = self.read_line() else {
                return default.unwrap_or(0);
            };
            let answer = answer.trim();
            if answer.is_empty() {
                if let Some(default) = default {
                    return default;
                }
                self.invalid("Please enter a valid integer");
                continue;
            }
            match answer.parse::<i64>() {
                Ok(value) => return value,
                Err(_) => self.invalid("Please enter a valid integer"),
            }
        }
    }

    /// `rich.FloatPrompt.ask`.
    pub fn ask_float(&mut self, prompt: &str, default: Option<f64>) -> f64 {
        self.ask_float_with(prompt, default, true)
    }

    /// Float prompt whose default is used for empty input but not rendered —
    /// pydantic keeps `Field(default=5)` as an int, and rich's FloatPrompt then
    /// shows no `(5)` while still returning it for an empty answer.
    pub fn ask_float_with(
        &mut self,
        prompt: &str,
        default: Option<f64>,
        show_default: bool,
    ) -> f64 {
        let default_text = if show_default {
            default.map(libresvip_core::options::format_float)
        } else {
            None
        };
        loop {
            self.render_prompt(prompt, None, default_text.as_deref());
            let Some(answer) = self.read_line() else {
                return default.unwrap_or(0.0);
            };
            let answer = answer.trim();
            if answer.is_empty() {
                if let Some(default) = default {
                    return default;
                }
                self.invalid("Please enter a valid number");
                continue;
            }
            match answer.parse::<f64>() {
                Ok(value) => return value,
                Err(_) => self.invalid("Please enter a valid number"),
            }
        }
    }

    fn render_prompt(&self, prompt: &str, choices: Option<&[&str]>, default: Option<&str>) {
        let mut text = prompt.to_owned();
        if let Some(choices) = choices {
            text.push_str(" [");
            text.push_str(&choices.join("/"));
            text.push(']');
        }
        if let Some(default) = default {
            text.push_str(&format!(" ({default})"));
        }
        text.push_str(": ");
        // rich wraps at `console.width - 1` in this (non-tty) environment, the
        // same effective width its panels use.
        let width = console_width().saturating_sub(1);
        // Short prompts are printed verbatim (rich does not re-wrap them), which
        // also preserves the trailing space after the colon.
        let lines = if display_width(&text) <= width {
            vec![text.clone()]
        } else {
            wrap_fold(&text, width)
        };
        let mut stdout = std::io::stdout();
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                let _ = stdout.write_all(b"\n");
            }
            let _ = stdout.write_all(line.as_bytes());
        }
        let _ = stdout.flush();
    }

    fn invalid(&self, message: &str) {
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(b"\n");
        let _ = stdout.write_all(message.as_bytes());
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
    }
}

/// `libresvip.cli.prompt.prompt_fields`.
pub fn prompt_fields(
    schema: &[OptionFieldDef],
    prompter: &mut Prompter,
    translate: &dyn Fn(&str) -> String,
) -> Options {
    let mut options = Options::new();
    for (index, field) in schema.iter().enumerate() {
        let title = format!("{}. {}", index + 1, translate(field.title));
        let default = &field.default;
        if field.key == "lyric_replacement_preset_name" {
            let groups = config::read_settings(|settings| settings.lyric_replace_rules_groups());
            let choices: Vec<&str> = groups.iter().map(String::as_str).collect();
            let default_text = default.to_display_string();
            let answer = prompter.ask(&title, Some(&choices), Some(&default_text));
            options.insert(field.key, OptionValue::Str(answer));
            continue;
        }
        if matches!(field.key, "encoding" | "lyric_encoding") {
            let choices = supported_charset_names();
            let default_text = default.to_display_string();
            let answer = prompter.ask(&title, Some(choices), Some(&default_text));
            options.insert(field.key, OptionValue::Str(answer));
            continue;
        }
        match &field.kind {
            OptionKind::Enum { variants, .. } => {
                let choices: Vec<&str> = variants.iter().map(|variant| variant.value).collect();
                let default_text = default.to_display_string();
                let answer = prompter.ask(&title, Some(&choices), Some(&default_text));
                options.insert(field.key, OptionValue::Str(answer));
            }
            OptionKind::Bool => {
                let answer = prompter.confirm(&title, default_bool(default));
                options.insert(field.key, OptionValue::Bool(answer));
            }
            OptionKind::Int => {
                let answer = prompter.ask_int(&title, Some(default_int(default)));
                options.insert(field.key, OptionValue::Int(answer));
            }
            OptionKind::Float => {
                // `Field(default=5)` keeps an int default in pydantic, and rich's
                // FloatPrompt then renders no `(default)` at all.
                let (default_value, show_default) = match default {
                    OptionValue::Int(value) => (Some(*value as f64), false),
                    other => (Some(default_float(other)), true),
                };
                let answer = prompter.ask_float_with(&title, default_value, show_default);
                options.insert(field.key, OptionValue::Float(answer));
            }
            OptionKind::Str => {
                let default_text = default.to_display_string();
                let answer = prompter.ask(&title, None, Some(&default_text));
                options.insert(field.key, OptionValue::Str(answer));
            }
            OptionKind::Opaque { .. } | OptionKind::Complex { .. } => {
                let default_text = default.to_display_string();
                let answer = prompter.ask(&title, None, Some(&default_text));
                options.insert(field.key, OptionValue::Str(answer));
            }
        }
    }
    options
}

fn default_bool(value: &OptionValue) -> bool {
    matches!(value, OptionValue::Bool(true))
}

fn default_int(value: &OptionValue) -> i64 {
    match value {
        OptionValue::Int(value) => *value,
        OptionValue::Float(value) => *value as i64,
        OptionValue::Bool(value) => i64::from(*value),
        OptionValue::Str(value) => value.parse().unwrap_or(0),
    }
}

fn default_float(value: &OptionValue) -> f64 {
    match value {
        OptionValue::Float(value) => *value,
        OptionValue::Int(value) => *value as f64,
        OptionValue::Bool(value) => f64::from(u8::from(*value)),
        OptionValue::Str(value) => value.parse().unwrap_or(0.0),
    }
}
