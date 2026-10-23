//! A `configparser.ConfigParser` compatible reader for the VSQ text payload.
//!
//! The plugin embeds an INI document in MIDI meta text events. Python reads it
//! with `configparser` (option names lower-cased, values stripped, `[section]`
//! headers, `=`/`:` delimiters, comment lines). Only reading is needed — the
//! generator writes the document by hand, exactly like `vsq_generator.py`.

use libresvip_core::error::{LibreSvipError, Result};

#[derive(Clone, Debug, Default)]
pub struct Ini {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    pub fn parse(text: &str) -> Result<Self> {
        let mut ini = Self::default();
        let mut current: Option<usize> = None;
        let mut last_option: Option<(usize, usize)> = None;
        for raw_line in text.split('\n') {
            let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(header) = trimmed
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                let name = header.trim().to_owned();
                if ini.sections.iter().any(|(section, _)| *section == name) {
                    return Err(LibreSvipError::Parse(format!(
                        "while reading from '<string>': section '{name}' already exists"
                    )));
                }
                ini.sections.push((name, Vec::new()));
                current = Some(ini.sections.len() - 1);
                last_option = None;
                continue;
            }
            if trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            // Continuation line of a multi line value (configparser default).
            if line.starts_with(' ') || line.starts_with('\t') {
                if let (Some(section), Some((_, option))) = (current, last_option) {
                    if let Some(entry) = ini.sections[section].1.get_mut(option) {
                        if !entry.1.is_empty() {
                            entry.1.push('\n');
                        }
                        entry.1.push_str(trimmed);
                    }
                    continue;
                }
            }
            let section_index = current.ok_or_else(|| {
                LibreSvipError::Parse("file contains no section headers".to_owned())
            })?;
            let delimiter = line
                .char_indices()
                .find(|(_, character)| *character == '=' || *character == ':')
                .map(|(index, _)| index)
                .ok_or_else(|| {
                    LibreSvipError::Parse(format!(
                        "source contains parsing errors: '{trimmed}'"
                    ))
                })?;
            let key = line[..delimiter].trim().to_lowercase();
            let value = line[delimiter + 1..].trim().to_owned();
            let options = &mut ini.sections[section_index].1;
            if options.iter().any(|(name, _)| *name == key) {
                return Err(LibreSvipError::Parse(format!(
                    "while reading from '<string>': option '{key}' in section '{}' already exists",
                    ini.sections[section_index].0
                )));
            }
            options.push((key, value));
            last_option = Some((section_index, options.len() - 1));
        }
        Ok(ini)
    }

    pub fn has_section(&self, name: &str) -> bool {
        self.sections.iter().any(|(section, _)| section == name)
    }

    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|(name, _)| name.as_str())
    }

    pub fn get(&self, section: &str, option: &str) -> Option<&str> {
        let option = option.to_lowercase();
        self.sections
            .iter()
            .find(|(name, _)| name == section)
            .and_then(|(_, options)| {
                options
                    .iter()
                    .find(|(name, _)| *name == option)
                    .map(|(_, value)| value.as_str())
            })
    }

    /// `SectionProxy.items()` — insertion order, lower-cased keys.
    pub fn items(&self, section: &str) -> Vec<(&str, &str)> {
        self.sections
            .iter()
            .find(|(name, _)| name == section)
            .map(|(_, options)| {
                options
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }
}
