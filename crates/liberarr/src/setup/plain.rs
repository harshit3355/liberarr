//! Plain CLI front-end: line prompts on stderr, hidden input for secrets on a terminal, piped
//! answers otherwise. No colors; the spinner is a `\r` animation, only on a terminal.
use super::{Choice, Picked, Prompter};
use crate::commands::ask;
use rust_i18n::t;
use std::io::{ErrorKind, IsTerminal, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

pub struct PlainPrompter;

/// Wraps `ask`: a closed pipe means no one can answer, so point to the non-interactive mode.
fn read(label: &str, hidden: bool) -> Result<String> {
    ask(label, hidden).map_err(|e| {
        if e.kind() == ErrorKind::UnexpectedEof && !std::io::stdin().is_terminal() {
            std::io::Error::new(ErrorKind::UnexpectedEof, t!("setup.no_input").into_owned())
        } else {
            e
        }
    })
}

fn label(question: &str, default: Option<&str>) -> String {
    match default {
        Some(d) => t!("init.prompt_default", question = question, default = d).into_owned(),
        None => t!("init.prompt", question = question).into_owned(),
    }
}

/// No animation without a terminal, or when the user asked for no decoration.
fn animated() -> bool {
    std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
        && std::env::var("TERM").is_ok_and(|t| t != "dumb")
}

impl Prompter for PlainPrompter {
    fn is_interactive(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn choose(
        &mut self,
        question: &str,
        options: &[Choice],
        default: Option<usize>,
    ) -> Result<Picked> {
        eprintln!("{question}");
        for (i, o) in options.iter().enumerate() {
            match o.note.is_empty() {
                true => eprintln!("  {}) {}", i + 1, o.value),
                false => eprintln!("  {}) {} — {}", i + 1, o.value, o.note),
            }
        }
        let shown = default.map(|d| (d + 1).to_string());
        let prompt = label(&t!("setup.choice"), shown.as_deref());
        loop {
            let v = read(&prompt, false)?;
            if v.is_empty() {
                match default {
                    Some(d) => return Ok(Picked::Option(d)),
                    None => eprintln!("{}", t!("init.value_required")),
                }
            } else if let Ok(n) = v.parse::<usize>() {
                if (1..=options.len()).contains(&n) {
                    return Ok(Picked::Option(n - 1));
                }
                eprintln!("{}", t!("setup.choice_invalid", max = options.len()));
            } else if let Some(i) = options
                .iter()
                .position(|o| o.value.eq_ignore_ascii_case(&v))
            {
                return Ok(Picked::Option(i));
            } else {
                return Ok(Picked::Text(v));
            }
        }
    }

    fn text(&mut self, question: &str, default: Option<&str>) -> Result<String> {
        let prompt = label(question, default);
        loop {
            let v = read(&prompt, false)?;
            match (v.is_empty(), default) {
                (false, _) => return Ok(v),
                (true, Some(d)) => return Ok(d.to_string()),
                (true, None) => eprintln!("{}", t!("init.value_required")),
            }
        }
    }

    fn secret(&mut self, question: &str) -> Result<String> {
        let prompt = label(question, None);
        loop {
            let v = read(&prompt, true)?;
            if !v.is_empty() {
                return Ok(v);
            }
            eprintln!("{}", t!("init.value_required"));
        }
    }

    fn confirm(&mut self, question: &str, default: bool) -> Result<bool> {
        let prompt = label(question, Some(if default { "Y/n" } else { "y/N" }));
        loop {
            match read(&prompt, false)?.to_lowercase().as_str() {
                "" => return Ok(default),
                "y" | "yes" | "o" | "oui" => return Ok(true),
                "n" | "no" | "non" => return Ok(false),
                _ => {}
            }
        }
    }

    fn info(&mut self, line: &str) {
        println!("{line}");
    }

    fn warn(&mut self, line: &str) {
        eprintln!("{line}");
    }

    fn busy(&mut self, text: &str, work: &mut dyn FnMut()) {
        if !animated() {
            return work();
        }
        let stop = Arc::new(AtomicBool::new(false));
        let spinner = {
            let (stop, text) = (stop.clone(), text.to_string());
            std::thread::spawn(move || {
                for frame in "|/-\\".chars().cycle() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    eprint!("\r{frame} {text}");
                    std::thread::sleep(Duration::from_millis(100));
                }
                // Erase the line.
                eprint!("\r{}\r", " ".repeat(text.chars().count() + 2));
            })
        };
        work();
        stop.store(true, Ordering::Relaxed);
        let _ = spinner.join();
    }
}
