//! The setup questions (`setup::Prompter`) as full-screen pages: arrow-key menus, masked key
//! field, spinner while the key is tested. Same questions and rules as the plain CLI.
use super::{LOGO, MIN_HEIGHT, MIN_WIDTH, motion, spinner, theme};
use crate::setup::{Choice, Picked, Prompter};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use rust_i18n::t;
use std::io::{Error, ErrorKind, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tachyonfx::{EffectManager, Interpolation, fx};

/// Messages kept under the question (key test results, warnings).
const LOG_LINES: usize = 4;

pub struct TuiPrompter<'a> {
    term: &'a mut DefaultTerminal,
    log: Vec<(String, Style)>,
    fx: EffectManager<()>,
}

/// A one-line text field. `masked` shows one dot per character, never the value.
#[derive(Default)]
struct Input {
    value: String,
    masked: bool,
}

impl Input {
    /// Applies a key or a paste; true when it changed the value.
    fn edit(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Paste(s) => {
                self.value.extend(s.chars().filter(|c| !c.is_control()));
                true
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            }) if !modifiers.contains(KeyModifiers::CONTROL) => {
                self.value.push(*c);
                true
            }
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                ..
            }) => self.value.pop().is_some(),
            _ => false,
        }
    }

    fn line(&self, placeholder: Option<&str>) -> Line<'static> {
        let shown = if self.masked {
            "•".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        };
        let mut spans = vec![Span::styled("› ", theme::accent()), Span::raw(shown)];
        spans.push(Span::styled("▏", theme::accent()));
        if self.value.is_empty()
            && let Some(p) = placeholder
        {
            spans.push(Span::styled(p.to_string(), theme::dim()));
        }
        Line::from(spans)
    }
}

fn key(ev: &Event) -> Option<KeyEvent> {
    match ev {
        Event::Key(k) if k.kind != KeyEventKind::Release => Some(*k),
        _ => None,
    }
}

fn cancelled() -> Error {
    Error::new(ErrorKind::Interrupted, t!("tui.cancelled").into_owned())
}

/// `width` columns centered horizontally, `height` rows from the top of `area`.
fn column(area: Rect, width: u16) -> Rect {
    let [c] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    c
}

/// Draws a page: logo (when tall enough), question, body, messages, key hints. Returns the
/// body's area, the region the transition plays on.
fn page(
    f: &mut Frame,
    question: &str,
    log: &[(String, Style)],
    hints: &str,
    body_rows: u16,
    body: impl FnOnce(&mut Frame, Rect),
) -> Option<Rect> {
    let area = f.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        f.render_widget(
            Paragraph::new(t!("tui.too_small").into_owned()).wrap(Wrap { trim: true }),
            area,
        );
        return None;
    }
    let tall = area.height >= 30;
    let header_rows = if tall { 5 } else { 2 };
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(header_rows),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let col = column(main, 84);
    if tall {
        let mut lines: Vec<Line> = vec![Line::raw("")];
        lines.extend(LOGO.iter().map(|l| Line::styled(*l, theme::accent())));
        f.render_widget(Paragraph::new(lines), column(header, 84));
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("▌liberarr ", theme::accent()),
                Span::styled(t!("tui.setup").into_owned(), theme::dim()),
            ])),
            column(header, 84),
        );
    }
    let q = Paragraph::new(Line::styled(question.to_string(), theme::accent()))
        .wrap(Wrap { trim: true });
    // Approximate wrapped height (word wrap may need one more row: hence the +1).
    let width = col.width.saturating_sub(4).max(1) as usize;
    let q_rows = (question.chars().count() / width + 1).min(6) as u16;
    let log_rows: u16 = log
        .iter()
        .map(|(m, _)| (m.chars().count() / width + 1) as u16)
        .sum();
    let [card, msgs] = Layout::vertical([
        Constraint::Length(q_rows + 1 + body_rows + 2),
        // One row of space above and below the messages.
        Constraint::Length(log_rows + 2),
    ])
    .areas(col);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::dim())
        .padding(Padding::horizontal(1));
    let inner = block.inner(card);
    f.render_widget(block, card);
    let [q_area, _, body_area] = Layout::vertical([
        Constraint::Length(q_rows),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);
    f.render_widget(q, q_area);
    body(f, body_area);
    let lines: Vec<Line> = log
        .iter()
        .map(|(m, s)| Line::styled(m.clone(), *s))
        .collect();
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        msgs.inner(ratatui::layout::Margin::new(1, 1)),
    );
    f.render_widget(
        Paragraph::new(Line::styled(hints.to_string(), theme::dim())),
        footer,
    );
    Some(card)
}

impl<'a> TuiPrompter<'a> {
    pub fn new(term: &'a mut DefaultTerminal) -> Self {
        Self {
            term,
            log: Vec::new(),
            fx: EffectManager::default(),
        }
    }

    fn push(&mut self, line: &str, style: Style) {
        self.log.push((line.to_string(), style));
        let extra = self.log.len().saturating_sub(LOG_LINES);
        self.log.drain(..extra);
    }

    /// Draws `view` and feeds it events until `on` returns an answer. A new page fades in.
    /// Ctrl+C or Esc cancels the setup.
    fn interact<S, T>(
        &mut self,
        question: &str,
        hints: &str,
        rows: u16,
        state: &mut S,
        view: impl Fn(&S, &mut Frame, Rect),
        mut on: impl FnMut(&mut S, &Event) -> Option<T>,
    ) -> Result<T> {
        if motion() {
            self.fx
                .add_effect(fx::fade_from_fg(theme::FADE, (300, Interpolation::QuadOut)));
        }
        let mut last = Instant::now();
        loop {
            let elapsed = last.elapsed();
            last = Instant::now();
            let log = self.log.clone();
            let fxm = &mut self.fx;
            self.term.draw(|f| {
                if let Some(card) = page(f, question, &log, hints, rows, |f, r| view(state, f, r)) {
                    fxm.process_effects(elapsed.into(), f.buffer_mut(), card);
                }
            })?;
            let wait = if self.fx.is_running() { 16 } else { 250 };
            if !event::poll(Duration::from_millis(wait))? {
                continue;
            }
            let ev = event::read()?;
            if let Some(k) = key(&ev) {
                let ctrl_c =
                    k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL);
                if ctrl_c || k.code == KeyCode::Esc {
                    return Err(cancelled());
                }
            }
            if let Some(answer) = on(state, &ev) {
                return Ok(answer);
            }
        }
    }

    /// A text field until a non-empty value is entered, or `default` on an empty Enter.
    fn field(&mut self, question: &str, default: Option<&str>, masked: bool) -> Result<String> {
        let mut input = Input {
            value: String::new(),
            masked,
        };
        let hints = t!("tui.hints_field").into_owned();
        loop {
            let answer = self.interact(
                question,
                &hints,
                1,
                &mut input,
                |i, f, r| f.render_widget(Paragraph::new(i.line(default)), r),
                |i, ev| {
                    if key(ev).is_some_and(|k| k.code == KeyCode::Enter) {
                        return Some(std::mem::take(&mut i.value));
                    }
                    i.edit(ev);
                    None
                },
            )?;
            let answer = answer.trim().to_string();
            match (answer.is_empty(), default) {
                (false, _) => return Ok(answer),
                (true, Some(d)) => return Ok(d.to_string()),
                (true, None) => self.push(&t!("init.value_required"), theme::warn()),
            }
        }
    }
}

struct Menu {
    sel: usize,
    typed: Input,
}

impl Prompter for TuiPrompter<'_> {
    fn is_interactive(&self) -> bool {
        true
    }

    fn choose(
        &mut self,
        question: &str,
        options: &[Choice],
        default: Option<usize>,
    ) -> Result<Picked> {
        let mut menu = Menu {
            sel: default.unwrap_or(0).min(options.len().saturating_sub(1)),
            typed: Input::default(),
        };
        let rows = options.len().min(12) as u16 + 2;
        let hints = t!("tui.hints_menu").into_owned();
        let typed_label = t!("tui.or_type").into_owned();
        self.interact(
            question,
            &hints,
            rows,
            &mut menu,
            |m, f, r| {
                let visible = r.height.saturating_sub(2) as usize;
                let first = m.sel.saturating_sub(visible.saturating_sub(1));
                let mut lines: Vec<Line> = options
                    .iter()
                    .enumerate()
                    .skip(first)
                    .take(visible)
                    .map(|(i, o)| {
                        let mark = if i == m.sel { "› " } else { "  " };
                        let style = if i == m.sel {
                            theme::selected()
                        } else {
                            Style::new()
                        };
                        let mut spans = vec![Span::styled(format!("{mark}{}", o.value), style)];
                        if !o.note.is_empty() {
                            spans.push(Span::styled(format!("  {}", o.note), theme::dim()));
                        }
                        Line::from(spans)
                    })
                    .collect();
                lines.push(Line::raw(""));
                let mut typed = m.typed.line(Some(&typed_label));
                typed.spans.insert(0, Span::raw(""));
                lines.push(typed);
                f.render_widget(Paragraph::new(lines), r);
            },
            |m, ev| {
                let k = key(ev);
                match k.map(|k| k.code) {
                    Some(KeyCode::Up) => m.sel = m.sel.saturating_sub(1),
                    Some(KeyCode::Down) => m.sel = (m.sel + 1).min(options.len().saturating_sub(1)),
                    Some(KeyCode::Home) => m.sel = 0,
                    Some(KeyCode::End) => m.sel = options.len().saturating_sub(1),
                    Some(KeyCode::Enter) => {
                        let typed = m.typed.value.trim();
                        if typed.is_empty() {
                            return (!options.is_empty()).then_some(Picked::Option(m.sel));
                        }
                        // Same rule as the plain CLI: a typed value matching an option picks it.
                        return Some(
                            match options
                                .iter()
                                .position(|o| o.value.eq_ignore_ascii_case(typed))
                            {
                                Some(i) => Picked::Option(i),
                                None => Picked::Text(typed.to_string()),
                            },
                        );
                    }
                    _ => {
                        m.typed.edit(ev);
                    }
                }
                None
            },
        )
    }

    fn text(&mut self, question: &str, default: Option<&str>) -> Result<String> {
        self.field(question, default, false)
    }

    fn secret(&mut self, question: &str) -> Result<String> {
        self.field(question, None, true)
    }

    fn confirm(&mut self, question: &str, default: bool) -> Result<bool> {
        let mut yes = default;
        let hints = t!("tui.hints_confirm").into_owned();
        let (y, n) = (t!("tui.yes").into_owned(), t!("tui.no").into_owned());
        self.interact(
            question,
            &hints,
            1,
            &mut yes,
            |yes, f, r| {
                let button = |label: &str, on: bool| {
                    let style = if on { theme::selected() } else { theme::dim() };
                    Span::styled(format!(" {label} "), style)
                };
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        button(&y, *yes),
                        Span::raw("   "),
                        button(&n, !*yes),
                    ])),
                    r,
                );
            },
            |yes, ev| match key(ev).map(|k| k.code) {
                Some(KeyCode::Left | KeyCode::Right | KeyCode::Tab) => {
                    *yes = !*yes;
                    None
                }
                Some(KeyCode::Char('y' | 'Y' | 'o' | 'O')) => Some(true),
                Some(KeyCode::Char('n' | 'N')) => Some(false),
                Some(KeyCode::Enter) => Some(*yes),
                _ => None,
            },
        )
    }

    fn info(&mut self, line: &str) {
        self.push(line, theme::ok());
    }

    fn warn(&mut self, line: &str) {
        self.push(line, theme::warn());
    }

    /// `work` runs on this thread; a scoped thread animates the spinner meanwhile.
    fn busy(&mut self, label: &str, work: &mut dyn FnMut()) {
        let stop = AtomicBool::new(false);
        let log = self.log.clone();
        let term = &mut *self.term;
        std::thread::scope(|s| {
            let stop = &stop;
            s.spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let line = format!("{} {label}", spinner());
                    let _ = term.draw(|f| {
                        page(f, &line, &log, "", 0, |_, _| {});
                    });
                    std::thread::sleep(Duration::from_millis(80));
                }
            });
            work();
            stop.store(true, Ordering::Relaxed);
        });
    }
}

/// The last page of a setup: what was saved (and the admin password, shown once).
pub fn done(term: &mut DefaultTerminal, lines: &[String]) -> Result<()> {
    let hints = t!("tui.hints_continue").into_owned();
    let body: Vec<Line> = lines.iter().map(|l| Line::raw(l.clone())).collect();
    let rows = body.len() as u16 * 2 + 1;
    let mut p = TuiPrompter::new(term);
    p.interact(
        &t!("tui.setup_done"),
        &hints,
        rows,
        &mut (),
        |_, f, r| f.render_widget(Paragraph::new(body.clone()).wrap(Wrap { trim: true }), r),
        |_, ev| {
            key(ev)
                .is_some_and(|k| k.code == KeyCode::Enter)
                .then_some(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::Input;

    /// A typed key is drawn as dots only.
    #[test]
    fn masked_field_never_shows_the_value() {
        let input = Input {
            value: "sk-secret-123".into(),
            masked: true,
        };
        let shown: String = input
            .line(None)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            !shown.contains("sk-secret-123") && !shown.contains("secret"),
            "{shown}"
        );
        assert_eq!(shown.matches('•').count(), 13);
    }
}
