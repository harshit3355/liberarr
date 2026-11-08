//! The full-screen app: recordings, detail tabs, pipeline status, rerun, device pull. Reads and
//! writes only through the use cases `App` already exposes (the web `Backend` and `pull`).
use super::text::{markdown, mindmap};
use super::{LOGO, MIN_HEIGHT, MIN_WIDTH, motion, spinner, theme};
use crate::commands::pull::{Transfer, mb, pull};
use crate::config::Config;
use crate::container::{self, App};
use liberarr_core::jobs::{JobInfo, JobQueue, JobStatus, Rerun, STEPS, rerun, run_job_once};
use liberarr_core::recordings::{RecordingItem, RecordingStatus};
use liberarr_core::summaries::MeetingSummary;
use liberarr_core::transcription::{Transcript, timestamp};
use liberarr_web::Backend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, Gauge, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};
use ratatui::{DefaultTerminal, Frame};
use rust_i18n::t;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tachyonfx::{EffectManager, Interpolation, Motion, fx};
use tokio::runtime::Handle;

const IDLE: Duration = Duration::from_secs(2);
/// The list and the open recording are reloaded this often.
const REFRESH: Duration = Duration::from_secs(1);
const TOAST: Duration = Duration::from_secs(5);

pub enum Exit {
    Quit,
    Settings,
}

type AnyError = Box<dyn std::error::Error>;

#[derive(Clone)]
enum Phase {
    Idle,
    Searching,
    Downloading(Transfer),
    /// The outcome lines, `true` when something failed.
    Finished(Vec<String>, bool),
}

struct PullState {
    running: bool,
    phase: Phase,
    downloaded: usize,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Summary,
    Transcript,
    Mindmap,
    Actions,
    Jobs,
}

const TABS: [Tab; 5] = [
    Tab::Summary,
    Tab::Transcript,
    Tab::Mindmap,
    Tab::Actions,
    Tab::Jobs,
];

struct Detail {
    id: i64,
    tab: usize,
    scroll: u16,
    action: usize,
    transcript: Option<Transcript>,
    summary: Option<MeetingSummary>,
    jobs: Vec<JobInfo>,
}

enum View {
    List,
    Detail(Box<Detail>),
    Pull,
}

struct Ui {
    app: Arc<App>,
    rt: Handle,
    items: Vec<RecordingItem>,
    list: ListState,
    filter: String,
    filtering: bool,
    view: View,
    /// Step menu of the rerun picker, open over the current view.
    picker: Option<usize>,
    toast: Option<(String, Style, Instant)>,
    pull: Arc<Mutex<PullState>>,
    worker_error: Arc<Mutex<Option<String>>>,
    fx: EffectManager<()>,
    transition: bool,
    refreshed: Instant,
}

/// Runs the app until the owner quits or opens the settings. The job worker runs in-process;
/// `serve` may run at the same time (claims are atomic, see `Db::reset_stale`).
pub fn run(term: &mut DefaultTerminal, cfg: Config) -> Result<Exit, AnyError> {
    let rt = tokio::runtime::Runtime::new()?;
    let warnings = container::warnings(&cfg);
    let app = Arc::new(App::open(cfg)?);
    rt.block_on(app.db.reset_stale())?;
    let worker_error = Arc::new(Mutex::new(None));
    let worker = {
        let (app, err) = (Arc::clone(&app), Arc::clone(&worker_error));
        rt.spawn(async move {
            loop {
                match run_job_once(&app.db, &*app).await {
                    Ok(true) => {}
                    Ok(false) => tokio::time::sleep(IDLE).await,
                    Err(e) => {
                        *lock(&err) = Some(e.to_string());
                        tokio::time::sleep(IDLE).await;
                    }
                }
            }
        })
    };
    let mut ui = Ui {
        app,
        rt: rt.handle().clone(),
        items: Vec::new(),
        list: ListState::default().with_selected(Some(0)),
        filter: String::new(),
        filtering: false,
        view: View::List,
        picker: None,
        toast: None,
        pull: Arc::new(Mutex::new(PullState {
            running: false,
            phase: Phase::Idle,
            downloaded: 0,
        })),
        worker_error,
        fx: EffectManager::default(),
        transition: true,
        refreshed: Instant::now(),
    };
    if !warnings.is_empty() {
        ui.toast(warnings.join(" "), theme::warn());
    }
    ui.reload()?;
    let exit = ui.event_loop(term);
    worker.abort();
    exit
}

fn key(ev: &Event) -> Option<KeyEvent> {
    match ev {
        Event::Key(k) if k.kind != KeyEventKind::Release => Some(*k),
        _ => None,
    }
}

fn local(t: jiff::Timestamp) -> String {
    t.to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

impl Ui {
    fn toast(&mut self, text: impl Into<String>, style: Style) {
        self.toast = Some((text.into(), style, Instant::now()));
    }

    fn visible(&self) -> Vec<&RecordingItem> {
        let f = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|i| {
                f.is_empty()
                    || i.title
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains(&f))
            })
            .collect()
    }

    fn selected_id(&self) -> Option<i64> {
        let v = self.visible();
        self.list.selected().and_then(|i| v.get(i)).map(|i| i.id)
    }

    /// Reloads the list, and the open recording.
    fn reload(&mut self) -> Result<(), AnyError> {
        self.items = self.rt.block_on(self.app.list())?;
        if let View::Detail(d) = &mut self.view {
            let id = d.id;
            d.jobs = self.rt.block_on(self.app.jobs(id))?;
            d.transcript = self.rt.block_on(self.app.transcript(id))?;
            d.summary = self.rt.block_on(self.app.summary(id))?;
        }
        let error = lock(&self.worker_error).take();
        if let Some(e) = error {
            self.toast(
                t!("tui.worker_error", error = e).into_owned(),
                theme::error(),
            );
        }
        self.refreshed = Instant::now();
        Ok(())
    }

    fn event_loop(&mut self, term: &mut DefaultTerminal) -> Result<Exit, AnyError> {
        let mut last = Instant::now();
        loop {
            if self.refreshed.elapsed() >= REFRESH {
                self.reload()?;
            }
            if self
                .toast
                .as_ref()
                .is_some_and(|(_, _, at)| at.elapsed() >= TOAST)
            {
                self.toast = None;
            }
            let elapsed = last.elapsed();
            last = Instant::now();
            term.draw(|f| self.draw(f, elapsed))?;
            // Fast frames while something moves, slower otherwise.
            let moving = self.fx.is_running()
                || lock(&self.pull).running
                || self
                    .items
                    .iter()
                    .any(|i| i.status == RecordingStatus::Processing);
            let wait = if moving { 50 } else { 250 };
            if !event::poll(Duration::from_millis(wait))? {
                continue;
            }
            let ev = event::read()?;
            let Some(k) = key(&ev) else { continue };
            if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(Exit::Quit);
            }
            if let Some(exit) = self.on_key(k)? {
                return Ok(exit);
            }
        }
    }

    fn open_view(&mut self, view: View) -> Result<(), AnyError> {
        self.view = view;
        self.transition = true;
        self.reload()
    }

    fn on_key(&mut self, k: KeyEvent) -> Result<Option<Exit>, AnyError> {
        if let Some(sel) = self.picker {
            match k.code {
                KeyCode::Up => self.picker = Some(sel.saturating_sub(1)),
                KeyCode::Down => self.picker = Some((sel + 1).min(STEPS.len() - 1)),
                KeyCode::Enter => {
                    self.picker = None;
                    self.rerun(STEPS[sel])?;
                }
                KeyCode::Esc => self.picker = None,
                _ => {}
            }
            return Ok(None);
        }
        if self.filtering {
            match k.code {
                KeyCode::Enter | KeyCode::Esc => self.filtering = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => self.filter.push(c),
                _ => {}
            }
            self.list.select(Some(0));
            return Ok(None);
        }
        match &mut self.view {
            View::List => match k.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(Some(Exit::Quit)),
                KeyCode::Up | KeyCode::Char('k') => self.list.select_previous(),
                KeyCode::Down | KeyCode::Char('j') => self.list.select_next(),
                KeyCode::Char('/') => self.filtering = true,
                KeyCode::Char('p') => self.open_view(View::Pull)?,
                KeyCode::Char('r') if self.selected_id().is_some() => self.picker = Some(0),
                KeyCode::Char('s') => return Ok(self.settings()),
                KeyCode::Enter => {
                    if let Some(id) = self.selected_id() {
                        self.open_view(View::Detail(Box::new(Detail {
                            id,
                            tab: 0,
                            scroll: 0,
                            action: 0,
                            transcript: None,
                            summary: None,
                            jobs: Vec::new(),
                        })))?;
                    }
                }
                _ => {}
            },
            View::Detail(d) => {
                let actions = d.summary.as_ref().map_or(0, |s| s.actions.len());
                let on_actions = TABS[d.tab] == Tab::Actions;
                match k.code {
                    KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => {
                        self.open_view(View::List)?;
                    }
                    KeyCode::Right | KeyCode::Tab => {
                        d.tab = (d.tab + 1) % TABS.len();
                        d.scroll = 0;
                        self.transition = true;
                    }
                    KeyCode::Left | KeyCode::BackTab => {
                        d.tab = (d.tab + TABS.len() - 1) % TABS.len();
                        d.scroll = 0;
                        self.transition = true;
                    }
                    KeyCode::Char(c @ '1'..='5') => {
                        d.tab = c as usize - '1' as usize;
                        d.scroll = 0;
                        self.transition = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') if on_actions => {
                        d.action = d.action.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') if on_actions => {
                        d.action = (d.action + 1).min(actions.saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => d.scroll = d.scroll.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => d.scroll = d.scroll.saturating_add(1),
                    KeyCode::PageUp => d.scroll = d.scroll.saturating_sub(10),
                    KeyCode::PageDown => d.scroll = d.scroll.saturating_add(10),
                    KeyCode::Char(' ') if on_actions && d.action < actions => {
                        let (id, index) = (d.id, d.action);
                        let done = d.summary.as_ref().is_some_and(|s| !s.actions[index].done);
                        let updated = self.rt.block_on(self.app.toggle_action(id, index, done));
                        match updated {
                            Ok(s) => {
                                if let View::Detail(d) = &mut self.view {
                                    d.summary = Some(s);
                                }
                            }
                            Err(e) => self.toast(e.to_string(), theme::error()),
                        }
                    }
                    KeyCode::Char('r') => self.picker = Some(0),
                    _ => {}
                }
            }
            View::Pull => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => self.open_view(View::List)?,
                KeyCode::Enter | KeyCode::Char('p') => self.start_pull(),
                _ => {}
            },
        }
        Ok(None)
    }

    fn current_id(&self) -> Option<i64> {
        match &self.view {
            View::Detail(d) => Some(d.id),
            _ => self.selected_id(),
        }
    }

    /// Same guard as the web API: a second run while the pipeline runs would call the AI twice.
    fn rerun(&mut self, step: &str) -> Result<(), AnyError> {
        let Some(id) = self.current_id() else {
            return Ok(());
        };
        if self.rt.block_on(rerun(&self.app.db, id, Some(step)))? == Rerun::Busy {
            self.toast(t!("tui.rerun_busy").into_owned(), theme::warn());
        } else {
            self.toast(
                t!("tui.rerun_queued", step = step).into_owned(),
                theme::ok(),
            );
        }
        self.reload()
    }

    /// The settings reopen the app with a new configuration: not while work is in progress.
    fn settings(&mut self) -> Option<Exit> {
        let working = lock(&self.pull).running
            || self
                .items
                .iter()
                .any(|i| i.status == RecordingStatus::Processing);
        if working {
            self.toast(t!("tui.settings_busy").into_owned(), theme::warn());
            return None;
        }
        Some(Exit::Settings)
    }

    /// Pulls on its own thread (Bluetooth can take minutes); the worker processes what arrives.
    fn start_pull(&mut self) {
        {
            let mut s = lock(&self.pull);
            if s.running {
                return;
            }
            *s = PullState {
                running: true,
                phase: Phase::Searching,
                downloaded: 0,
            };
        }
        let (app, rt, state) = (
            Arc::clone(&self.app),
            self.rt.clone(),
            Arc::clone(&self.pull),
        );
        std::thread::spawn(move || {
            let progress = Arc::clone(&state);
            let on = Arc::new(move |t: Option<&Transfer>| {
                let mut s = lock(&progress);
                match t {
                    Some(t) => s.phase = Phase::Downloading(t.clone()),
                    None => s.downloaded += 1,
                }
            });
            let outcome = rt.block_on(pull(&app, on));
            let (lines, failed) = match outcome {
                Ok(None) => (vec![t!("pull.no_device_linked").into_owned()], true),
                Ok(Some(r)) if r.no_device => {
                    (vec![t!("pull.no_device_in_range").into_owned()], true)
                }
                Ok(Some(r)) => {
                    let mut lines = vec![
                        t!(
                            "pull.summary",
                            pulled = r.pulled.len(),
                            skipped = r.skipped_recording.len(),
                            failed = r.failed.len()
                        )
                        .into_owned(),
                    ];
                    lines.extend(r.failed.iter().map(|(session, why)| {
                        t!("pull.failed_session", session = session, why = why).into_owned()
                    }));
                    lines.extend(r.device_errors.iter().map(|(tail, why)| {
                        t!("pull.device_error", tail = tail, why = why).into_owned()
                    }));
                    let failed = !r.failed.is_empty() || !r.device_errors.is_empty();
                    (lines, failed)
                }
                Err(e) => (vec![e.to_string()], true),
            };
            let mut s = lock(&state);
            s.running = false;
            s.phase = Phase::Finished(lines, failed);
        });
    }

    fn draw(&mut self, f: &mut Frame, elapsed: Duration) {
        let area = f.area();
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            f.render_widget(
                Paragraph::new(t!("tui.too_small").into_owned()).wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
        let [header, main, status, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        self.draw_header(f, header);
        match &self.view {
            View::List => self.draw_list(f, main),
            View::Detail(d) => self.draw_detail(f, main, d),
            View::Pull => self.draw_pull(f, main),
        }
        if let Some(sel) = self.picker {
            draw_picker(f, main, sel);
        }
        if let Some((text, style, _)) = &self.toast {
            f.render_widget(Paragraph::new(Line::styled(text.clone(), *style)), status);
        }
        let hints = match (&self.view, self.filtering) {
            (_, true) => t!("tui.hints_filter"),
            (View::List, _) => t!("tui.hints_list"),
            (View::Detail(_), _) => t!("tui.hints_detail"),
            (View::Pull, _) => t!("tui.hints_pull"),
        };
        f.render_widget(
            Paragraph::new(Line::styled(hints.into_owned(), theme::dim())),
            footer,
        );
        if self.transition && motion() {
            self.fx.add_effect(fx::sweep_in(
                Motion::LeftToRight,
                12,
                0,
                theme::FADE,
                (280, Interpolation::QuadOut),
            ));
        }
        self.transition = false;
        self.fx
            .process_effects(elapsed.into(), f.buffer_mut(), main);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let title = match &self.view {
            View::List => t!("tui.recordings").into_owned(),
            View::Detail(d) => d
                .summary
                .as_ref()
                .map(|s| s.title.clone())
                .or_else(|| {
                    self.items
                        .iter()
                        .find(|i| i.id == d.id)
                        .and_then(|i| i.title.clone())
                })
                .unwrap_or_else(|| t!("tui.untitled").into_owned()),
            View::Pull => t!("tui.pull_title").into_owned(),
        };
        let processing = self
            .items
            .iter()
            .filter(|i| i.status == RecordingStatus::Processing)
            .count();
        let mut right = Vec::new();
        if lock(&self.pull).running {
            right.push(Span::styled(
                format!("{} {}  ", spinner(), t!("tui.pulling")),
                theme::accent(),
            ));
        }
        right.push(if processing > 0 {
            Span::styled(
                format!("{} {}", spinner(), t!("tui.processing", count = processing)),
                theme::accent(),
            )
        } else {
            Span::styled(format!("● {}", t!("tui.idle")), theme::ok())
        });
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Min(10), Constraint::Length(36)]).areas(area);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("▌liberarr  ", theme::accent()),
                Span::raw(title),
            ])),
            left_area,
        );
        f.render_widget(
            Paragraph::new(Line::from(right)).alignment(ratatui::layout::Alignment::Right),
            right_area,
        );
    }

    fn draw_list(&mut self, f: &mut Frame, area: Rect) {
        let [bar, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        let count = self.visible().len();
        let search = if self.filtering || !self.filter.is_empty() {
            Line::from(vec![
                Span::styled("/ ", theme::accent()),
                Span::raw(self.filter.clone()),
                Span::styled(if self.filtering { "▏" } else { "" }, theme::accent()),
            ])
        } else {
            Line::styled(t!("tui.count", count = count).into_owned(), theme::dim())
        };
        f.render_widget(Paragraph::new(search), bar);
        if self.items.is_empty() {
            let mut lines: Vec<Line> = LOGO
                .iter()
                .map(|l| Line::styled(*l, theme::accent()))
                .collect();
            lines.push(Line::raw(""));
            lines.push(Line::raw(t!("tui.empty").into_owned()));
            let [c] = Layout::vertical([Constraint::Length(7)])
                .flex(Flex::Center)
                .areas(list_area);
            f.render_widget(
                Paragraph::new(lines)
                    .alignment(ratatui::layout::Alignment::Center)
                    .wrap(Wrap { trim: true }),
                c,
            );
            return;
        }
        let rows: Vec<ListItem> = self
            .visible()
            .iter()
            .map(|i| {
                let (mark, style, note) = match i.status {
                    RecordingStatus::Done if i.no_speech => (
                        "○".to_string(),
                        theme::dim(),
                        t!("no_speech.marker").into_owned(),
                    ),
                    RecordingStatus::Done => ("●".to_string(), theme::ok(), String::new()),
                    RecordingStatus::Processing => {
                        (spinner().to_string(), theme::accent(), String::new())
                    }
                    RecordingStatus::Failed => (
                        "▲".to_string(),
                        theme::error(),
                        t!(
                            "tui.failed_step",
                            step = i.failed_step.as_deref().unwrap_or("?")
                        )
                        .into_owned(),
                    ),
                };
                let duration = i.duration_s.map(timestamp).unwrap_or_default();
                // Status and date first: a long title never pushes them off screen.
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{mark} "), style),
                    Span::styled(
                        format!("{}  {duration:>5}  ", local(i.created_at)),
                        theme::dim(),
                    ),
                    Span::raw(
                        i.title
                            .clone()
                            .unwrap_or_else(|| t!("tui.untitled").into_owned()),
                    ),
                    Span::styled(format!("  {note}"), style),
                ]))
            })
            .collect();
        if self.list.selected().is_some_and(|s| s >= count) {
            self.list.select(Some(count.saturating_sub(1)));
        }
        f.render_stateful_widget(
            List::new(rows)
                .highlight_style(theme::selected())
                .highlight_symbol("▶ "),
            list_area,
            &mut self.list,
        );
    }

    fn draw_detail(&self, f: &mut Frame, area: Rect, d: &Detail) {
        let [tabs_area, body] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
        let done = d
            .summary
            .as_ref()
            .map_or(0, |s| s.actions.iter().filter(|a| a.done).count());
        let total = d.summary.as_ref().map_or(0, |s| s.actions.len());
        let titles = [
            t!("tui.tab_summary").into_owned(),
            t!("tui.tab_transcript").into_owned(),
            t!("tui.tab_mindmap").into_owned(),
            format!("{} ({done}/{total})", t!("tui.tab_actions")),
            t!("tui.tab_jobs").into_owned(),
        ];
        f.render_widget(
            Tabs::new(titles)
                .select(d.tab)
                .highlight_style(theme::selected())
                .divider("│"),
            tabs_area,
        );
        let none = |key: &str| vec![Line::styled(t!(key).into_owned(), theme::dim())];
        let lines: Vec<Line> = match TABS[d.tab] {
            Tab::Summary => d
                .summary
                .as_ref()
                .map_or_else(|| none("tui.no_summary"), |s| markdown(&s.summary_md)),
            Tab::Transcript => match &d.transcript {
                None => none("tui.no_transcript"),
                Some(t) if !t.has_speech() => none("no_speech.detail"),
                Some(t) if t.segments.is_empty() => {
                    t.text.lines().map(|l| Line::raw(l.to_string())).collect()
                }
                Some(t) => t
                    .segments
                    .iter()
                    .map(|s| {
                        Line::from(vec![
                            Span::styled(format!("{}  ", timestamp(s.start)), theme::dim()),
                            Span::raw(s.text.trim().to_string()),
                        ])
                    })
                    .collect(),
            },
            Tab::Mindmap => d
                .summary
                .as_ref()
                .map_or_else(|| none("tui.no_summary"), |s| mindmap(&s.mindmap)),
            Tab::Actions => match &d.summary {
                None => none("tui.no_summary"),
                Some(s) if s.actions.is_empty() => none("tui.no_actions"),
                Some(s) => s
                    .actions
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let check = if a.done { "[x] " } else { "[ ] " };
                        let mut extra = Vec::new();
                        if let Some(o) = &a.owner {
                            extra.push(o.clone());
                        }
                        if let Some(due) = &a.due {
                            extra.push(due.clone());
                        }
                        if let Some(at) = a.start {
                            extra.push(timestamp(at));
                        }
                        let style = if i == d.action {
                            theme::selected()
                        } else if a.done {
                            theme::dim()
                        } else {
                            Style::new()
                        };
                        Line::from(vec![
                            Span::styled(format!("{check}{}", a.action), style),
                            Span::styled(format!("  {}", extra.join(" · ")), theme::dim()),
                        ])
                    })
                    .collect(),
            },
            Tab::Jobs => d
                .jobs
                .iter()
                .map(|j| {
                    let (mark, style) = match j.status {
                        JobStatus::Done => ("●".to_string(), theme::ok()),
                        JobStatus::Failed => ("▲".to_string(), theme::error()),
                        JobStatus::Running => (spinner().to_string(), theme::accent()),
                        JobStatus::Queued => ("○".to_string(), theme::dim()),
                    };
                    Line::from(vec![
                        Span::styled(format!("{mark} {:<11}", j.step), style),
                        Span::styled(
                            format!("{}  ×{}  ", local(j.updated_at), j.attempts),
                            theme::dim(),
                        ),
                        Span::styled(j.error.clone().unwrap_or_default(), theme::error()),
                    ])
                })
                .collect(),
        };
        // On the actions tab the cursor drives the scroll.
        let scroll = if TABS[d.tab] == Tab::Actions {
            (d.action as u16).saturating_sub(body.height.saturating_sub(2))
        } else {
            d.scroll
        };
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((scroll, 0)),
            body,
        );
    }

    fn draw_pull(&self, f: &mut Frame, area: Rect) {
        let s = lock(&self.pull);
        let [c] = Layout::horizontal([Constraint::Length(area.width.min(80))])
            .flex(Flex::Center)
            .areas(area);
        let [text_area, gauge_area, result_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(c.inner(ratatui::layout::Margin::new(1, 1)));
        let line = match &s.phase {
            Phase::Idle => Line::raw(t!("tui.pull_ready").into_owned()),
            Phase::Searching => Line::styled(
                format!("{} {}", spinner(), t!("pull.searching")),
                theme::accent(),
            ),
            Phase::Downloading(tr) => Line::styled(
                format!(
                    "{} {}",
                    spinner(),
                    t!(
                        "tui.pull_downloading",
                        label = tr.label,
                        count = s.downloaded + 1
                    )
                ),
                theme::accent(),
            ),
            Phase::Finished(_, failed) => {
                let style = if *failed { theme::warn() } else { theme::ok() };
                Line::styled(t!("tui.pull_finished").into_owned(), style)
            }
        };
        f.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), text_area);
        if let Phase::Downloading(tr) = &s.phase {
            let ratio = if tr.total == 0 {
                0.0
            } else {
                (tr.done as f64 / tr.total as f64).clamp(0.0, 1.0)
            };
            let label = format!(
                "{:.1}/{:.1} MB  {:.0} kB/s  {}:{:02}",
                mb(tr.done),
                mb(tr.total),
                tr.speed / 1000.0,
                tr.eta / 60,
                tr.eta % 60
            );
            f.render_widget(
                Gauge::default()
                    .gauge_style(theme::accent())
                    .ratio(ratio)
                    .label(label),
                gauge_area,
            );
        }
        if let Phase::Finished(lines, _) = &s.phase {
            let lines: Vec<Line> = lines.iter().map(|l| Line::raw(l.clone())).collect();
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), result_area);
        }
    }
}

fn draw_picker(f: &mut Frame, area: Rect, sel: usize) {
    let [v] = Layout::vertical([Constraint::Length(STEPS.len() as u16 + 2)])
        .flex(Flex::Center)
        .areas(area);
    let [r] = Layout::horizontal([Constraint::Length(34)])
        .flex(Flex::Center)
        .areas(v);
    let lines: Vec<Line> = STEPS
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let style = if i == sel {
                theme::selected()
            } else {
                Style::new()
            };
            Line::styled(format!(" {s} "), style)
        })
        .collect();
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme::accent())
                .title(t!("tui.rerun_title").into_owned()),
        ),
        r,
    );
}
