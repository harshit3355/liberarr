//! Language selection for the CLI text (`locales/en.yml`, `locales/fr.yml`, embedded at compile
//! time by `rust_i18n::i18n!` in `main.rs`). English is the default and the fallback.
use crate::config::parse_env;
use liberarr_core::lang::Language;
use std::path::Path;

/// `LIBERARR_LANG` in `<home>/.env`, else the first non-empty of `LC_ALL`, `LC_MESSAGES`, `LANG`,
/// else English. An unsupported language is English. Only that file is read: a missing or
/// incomplete `.env` is fine.
pub fn ui_language(home: &Path) -> Language {
    let file = std::fs::read_to_string(home.join(".env"))
        .map_or_else(|_| Default::default(), |t| parse_env(&t));
    resolve(file.get("LIBERARR_LANG").map(String::as_str), |k| {
        std::env::var(k).ok()
    })
}

/// Same as [`ui_language`] when no home is known yet (before `LIBERARR_HOME` resolves).
pub fn env_language() -> Language {
    resolve(None, |k| std::env::var(k).ok())
}

fn resolve(configured: Option<&str>, env: impl Fn(&str) -> Option<String>) -> Language {
    let first = |v: Option<&str>| {
        v.map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    first(configured)
        .or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .find_map(|k| first(env(k).as_deref()))
        })
        .and_then(|v| Language::parse(&v))
        .unwrap_or(Language::En)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    fn keys(catalog: &str) -> BTreeSet<&str> {
        catalog
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once(':').map(|(k, _)| k.trim()))
            .collect()
    }

    /// A key missing from one catalog would reach the user as the bare key.
    #[test]
    fn catalogs_have_the_same_keys() {
        let en = keys(include_str!("../locales/en.yml"));
        let fr = keys(include_str!("../locales/fr.yml"));
        assert!(!en.is_empty());
        assert_eq!(en.difference(&fr).collect::<Vec<_>>(), Vec::<&&str>::new());
        assert_eq!(fr.difference(&en).collect::<Vec<_>>(), Vec::<&&str>::new());
    }
}
