//! The connection test behind `init`: the model list of an API provider, or the login of the
//! `claude` CLI.
use liberarr_core::ai::Provider;
use liberarr_infra::model_list::list_models;
use rust_i18n::t;

/// True when `claude auth status` exits 0 within a few seconds. Output is discarded: no token is read.
fn claude_connected() -> bool {
    let Ok(mut child) = std::process::Command::new("claude")
        .args(["auth", "status"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    for _ in 0..50 {
        match child.try_wait() {
            Ok(Some(s)) => return s.success(),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => return false,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

/// The [`super::Checker`] of the real command: blocks on `rt`.
pub fn connect(
    rt: &tokio::runtime::Runtime,
    provider: Provider,
    url: &str,
    key: &str,
) -> Result<Vec<String>, String> {
    if provider == Provider::Claude {
        return if claude_connected() {
            Ok(Vec::new())
        } else {
            Err(t!("setup.claude_not_ready").into_owned())
        };
    }
    rt.block_on(list_models(provider, url, key))
        .map_err(|e| e.to_string())
}
