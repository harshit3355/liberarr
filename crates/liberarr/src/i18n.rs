//! Language selection for the CLI text (`locales/en.yml`, `locales/fr.yml`, embedded at compile
//! time by `rust_i18n::i18n!` in `main.rs`). English is the default and the fallback.
use crate::config::parse_env;
use liberarr_core::lang::Language;
use std::path::Path;

/// `LIBERARR_LANG` in `<home>/.env`, else the OS locale. Only that file is read: a missing or
/// incomplete `.env` is fine.
pub fn ui_language(home: &Path) -> Language {
    let file = std::fs::read_to_string(home.join(".env"))
        .map_or_else(|_| Default::default(), |t| parse_env(&t));
    language(file.get("LIBERARR_LANG").map(String::as_str))
}

/// The CLI language: `configured` (`LIBERARR_LANG`) when it names a supported language, else the
/// first non-empty of `LC_ALL`, `LC_MESSAGES`, `LANG` (POSIX precedence) when that one is
/// supported, else English. Also the language used before `LIBERARR_HOME` resolves, with `None`.
pub fn language(configured: Option<&str>) -> Language {
    resolve(configured, |k| std::env::var(k).ok())
}

fn resolve(configured: Option<&str>, env: impl Fn(&str) -> Option<String>) -> Language {
    let first = |v: Option<&str>| {
        v.map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    configured
        .and_then(Language::parse)
        .or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .find_map(|k| first(env(k).as_deref()))
                .and_then(|v| Language::parse(&v))
        })
        .unwrap_or(Language::En)
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use liberarr_core::lang::Language::{En, Fr};
    use std::collections::BTreeSet;

    #[test]
    fn language_resolution() {
        let env = |pairs: &[(&str, &str)]| {
            let pairs: Vec<_> = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            move |k: &str| pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
        };
        let fr_env = &[("LC_ALL", ""), ("LANG", "fr_FR.UTF-8")][..];
        for (configured, vars, want) in [
            (None, fr_env, Fr),
            (None, &[("LC_ALL", "C"), ("LANG", "fr_FR")][..], En),
            (Some("fr"), &[][..], Fr),
            (Some("de"), &[][..], En),
            (Some("de"), &[("LANG", "fr_FR")][..], Fr),
            (None, &[][..], En),
        ] {
            assert_eq!(
                resolve(configured, env(vars)),
                want,
                "{configured:?} {vars:?}"
            );
        }
    }

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
