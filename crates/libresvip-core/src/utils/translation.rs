//! Minimal gettext `.mo` reader and translation chain.
//!
//! Mirrors `libresvip.utils.translation.gettext_lazy` plus
//! `libresvip.extension.manager.merge_translation`: the core catalog is tried
//! first and each plugin catalog acts as a fallback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// One parsed `.mo` catalog.
#[derive(Clone, Debug, Default)]
pub struct Translation {
    entries: HashMap<Vec<u8>, Vec<u8>>,
}

fn read_u32(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    let array: [u8; 4] = slice.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(array)
    } else {
        u32::from_be_bytes(array)
    })
}

impl Translation {
    /// Parses a GNU `.mo` file.
    pub fn load_mo(bytes: &[u8]) -> Option<Self> {
        let magic = read_u32(bytes, 0, true)?;
        let little_endian = match magic {
            0x9504_12DE => true,
            0xDE12_0495 => false,
            _ => return None,
        };
        let count = read_u32(bytes, 8, little_endian)? as usize;
        let originals_offset = read_u32(bytes, 12, little_endian)? as usize;
        let translations_offset = read_u32(bytes, 16, little_endian)? as usize;

        let mut entries = HashMap::with_capacity(count);
        for index in 0..count {
            let original_length = read_u32(bytes, originals_offset + index * 8, little_endian)? as usize;
            let original_position =
                read_u32(bytes, originals_offset + index * 8 + 4, little_endian)? as usize;
            let translation_length =
                read_u32(bytes, translations_offset + index * 8, little_endian)? as usize;
            let translation_position =
                read_u32(bytes, translations_offset + index * 8 + 4, little_endian)? as usize;
            let original = bytes.get(original_position..original_position + original_length)?;
            let translation =
                bytes.get(translation_position..translation_position + translation_length)?;
            entries.insert(original.to_vec(), translation.to_vec());
        }
        Some(Self { entries })
    }

    pub fn from_file(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        Self::load_mo(&bytes)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `gettext(message)`; returns the message itself when untranslated.
    pub fn gettext(&self, message: &str) -> String {
        match self.entries.get(message.as_bytes()) {
            Some(value) => String::from_utf8_lossy(value).into_owned(),
            None => message.to_owned(),
        }
    }

    /// `pgettext(context, message)`.
    pub fn pgettext(&self, context: &str, message: &str) -> String {
        let key = format!("{context}\u{4}{message}");
        match self.entries.get(key.as_bytes()) {
            Some(value) => String::from_utf8_lossy(value).into_owned(),
            None => message.to_owned(),
        }
    }
}

/// A chain of catalogs, queried in order (core, then plugin, then middleware…).
#[derive(Clone, Debug, Default)]
pub struct TranslationChain {
    catalogs: Vec<Arc<Translation>>,
}

impl TranslationChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, catalog: Arc<Translation>) {
        if !catalog.is_empty() {
            self.catalogs.push(catalog);
        }
    }

    pub fn extend(&mut self, other: TranslationChain) {
        for catalog in other.catalogs {
            self.push(catalog);
        }
    }

    pub fn gettext(&self, message: &str) -> String {
        if message.is_empty() {
            return String::new();
        }
        for catalog in &self.catalogs {
            if let Some(value) = catalog.entries.get(message.as_bytes()) {
                return String::from_utf8_lossy(value).into_owned();
            }
        }
        message.to_owned()
    }

    pub fn pgettext(&self, context: &str, message: &str) -> String {
        if message.is_empty() {
            return String::new();
        }
        let key = format!("{context}\u{4}{message}");
        for catalog in &self.catalogs {
            if let Some(value) = catalog.entries.get(key.as_bytes()) {
                return String::from_utf8_lossy(value).into_owned();
            }
        }
        message.to_owned()
    }
}

/// Directory holding catalogs: `res/locales/<lang>/LC_MESSAGES/<plugin>.mo`.
pub fn locales_dir() -> PathBuf {
    crate::core::constants::res_dir().join("locales")
}

/// Catalogs already parsed, keyed by `"<plugin_id>:<lang>"` (core uses the
/// id `"core"`). `gettext_lazy` sits on per-note paths (svp export), so the
/// `.mo` files must not be re-read and re-parsed on every call.
fn cached_catalog(key: &str, load: impl FnOnce() -> Translation) -> Arc<Translation> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Translation>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    guard
        .entry(key.to_owned())
        .or_insert_with(|| Arc::new(load()))
        .clone()
}

/// Loads the core catalog for a language (cached).
pub fn core_catalog(lang: &str) -> Arc<Translation> {
    cached_catalog(&format!("core:{lang}"), || {
        let path = locales_dir()
            .join(lang)
            .join("LC_MESSAGES")
            .join("core.mo");
        Translation::from_file(&path).unwrap_or_default()
    })
}

/// Loads one plugin's catalog (cached); the Python side merges every `.mo` in
/// the plugin's own `locales/<lang>/LC_MESSAGES` directory.
pub fn plugin_catalog(plugin_id: &str, lang: &str) -> Arc<Translation> {
    cached_catalog(&format!("{plugin_id}:{lang}"), || {
        let path = locales_dir()
            .join(lang)
            .join("LC_MESSAGES")
            .join(format!("{plugin_id}.mo"));
        Translation::from_file(&path).unwrap_or_default()
    })
}

/// Builds the translation chain used by the CLI:
/// `get_translation(lang, include_plugins=...)`.
///
/// `plugin_ids` must be supplied by the crate that owns the plugin registry
/// (core cannot depend on it).
pub fn get_translation(lang: &str, plugin_ids: &[&str]) -> TranslationChain {
    let mut chain = TranslationChain::new();
    chain.push(core_catalog(lang));
    for plugin_id in plugin_ids {
        chain.push(plugin_catalog(plugin_id, lang));
    }
    chain
}

/// Translation of one message using the current UI language.
pub fn gettext_lazy(message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }
    let lang = crate::core::config::read_settings(|settings| settings.language.value()).to_owned();
    core_catalog(&lang).gettext(message)
}

/// `pgettext_lazy`.
pub fn pgettext_lazy(context: &str, message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }
    let lang = crate::core::config::read_settings(|settings| settings.language.value()).to_owned();
    core_catalog(&lang).pgettext(context, message)
}
