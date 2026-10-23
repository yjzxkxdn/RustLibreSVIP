//! Global translation chain (`libresvip.cli.__init__` sets
//! `translation.singleton_translation = get_translation()`).

use std::sync::OnceLock;

use libresvip_core::core::config;
use libresvip_core::utils::translation::{get_translation, TranslationChain};

struct Translator {
    chain: TranslationChain,
}

static TRANSLATOR: OnceLock<Translator> = OnceLock::new();

fn translator() -> &'static Translator {
    TRANSLATOR.get_or_init(|| {
        let lang = config::read_settings(|settings| settings.language.value()).to_owned();
        let mut ids: Vec<&str> = libresvip_plugins::plugin_ids();
        let middleware_ids = libresvip_middlewares::middleware_ids();
        ids.extend(middleware_ids.iter().copied());
        Translator {
            chain: get_translation(&lang, &ids),
        }
    })
}

/// `_()` — translates a message using the merged catalogs.
pub fn t(message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }
    translator().chain.gettext(message)
}

/// `pgettext()`.
pub fn pt(context: &str, message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }
    translator().chain.pgettext(context, message)
}

/// Python `str.format` with successive `{}` placeholders.
pub fn format_msg(template: &str, args: &[&str]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut remaining = template;
    for arg in args {
        match remaining.find("{}") {
            Some(position) => {
                result.push_str(&remaining[..position]);
                result.push_str(arg);
                remaining = &remaining[position + 2..];
            }
            None => break,
        }
    }
    result.push_str(remaining);
    result
}
