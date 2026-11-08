//! `liberarr init --non-interactive`: the provider settings come from the process environment
//! (`LIBERARR_{TRANSCRIBE,LLM}_{PROVIDER,BASE_URL,API_KEY,MODEL}`), never from the command line.
//! Same rules as the questions: a value already set in the `.env` wins and is never overwritten.
use super::{
    CONTENT_LANGUAGE, Plan, STEPS, SetupResult, current, current_provider, routing_var, secrets,
    valid_url,
};
use crate::config::{is_unset, parse_routing};
use crate::providers;
use liberarr_core::ai::{Provider, capability, supports};
use liberarr_core::lang::Language;
use rust_i18n::t;
use std::collections::HashMap;

pub fn from_env(
    existing: &HashMap<String, String>,
    ui: Language,
    var: impl Fn(&str) -> Option<String>,
) -> SetupResult<Plan> {
    let mut plan = secrets(existing)?;
    // The environment's value, unless unset or a placeholder. A control character (a newline
    // would add lines to the .env) is refused. Never printed.
    let from_env = |k: &str| -> SetupResult<Option<String>> {
        let v = var(k)
            .map(|v| v.trim().to_string())
            .filter(|v| !is_unset(v));
        match v {
            Some(v) if v.chars().any(char::is_control) => {
                Err(t!("setup.nonint_value_invalid", var = k)
                    .into_owned()
                    .into())
            }
            v => Ok(v),
        }
    };
    let missing = |k: &str| -> Box<dyn std::error::Error> {
        t!("setup.nonint_missing", var = k).into_owned().into()
    };
    for (step, [provider_var, url_var, key_var, model_var], _) in STEPS {
        let provider = match current_provider(existing, provider_var, url_var) {
            Some(p) => p,
            None => {
                let v = from_env(provider_var)?.ok_or_else(|| missing(provider_var))?;
                let p = Provider::parse(&v)
                    .filter(|p| supports(capability(*p, ""), step))
                    .ok_or_else(|| {
                        t!("setup.nonint_provider_invalid", var = provider_var).into_owned()
                    })?;
                plan.updates.push((provider_var, p.as_str().to_string()));
                p
            }
        };
        let (default_url, default_model) = providers::defaults(provider, step);
        let mut want = |k: &'static str, default: Option<&str>| -> SetupResult<()> {
            if current(existing, k).is_none() {
                let v = from_env(k)?
                    .or_else(|| default.map(str::to_string))
                    .ok_or_else(|| missing(k))?;
                if k == url_var && !valid_url(&v) {
                    return Err(t!("setup.nonint_url_invalid", var = k).into_owned().into());
                }
                plan.updates.push((k, v));
            }
            Ok(())
        };
        // The claude CLI has its own login: no URL, no key.
        if provider != Provider::Claude {
            want(url_var, default_url)?;
            want(key_var, None)?;
        }
        want(model_var, default_model)?;
        // Optional: absent or empty means no restriction, written as an empty value.
        let var = routing_var(step);
        if provider == Provider::OpenRouter && !existing.contains_key(var) {
            let list = match from_env(var)? {
                Some(v) => parse_routing(&v)
                    .map(|list| list.join(","))
                    .ok_or_else(|| t!("config.routing_invalid", var = var).into_owned())?,
                None => String::new(),
            };
            plan.updates.push((var, list));
        }
    }
    if current(existing, CONTENT_LANGUAGE).is_none() {
        let lang = match from_env(CONTENT_LANGUAGE)? {
            Some(v) => Language::parse(&v).ok_or_else(|| {
                t!("setup.nonint_language_invalid", var = CONTENT_LANGUAGE).into_owned()
            })?,
            None => ui,
        };
        plan.updates
            .push((CONTENT_LANGUAGE, lang.as_str().to_string()));
    }
    Ok(plan)
}
