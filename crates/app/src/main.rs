//! Cleanator desktop app (iced). One window, three gestures: look, tick, copy the script.
//! Runs the core in a background thread; never deletes anything (R1).
#![forbid(unsafe_code)]

mod treemap;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::{
    button, canvas, checkbox, column, container, horizontal_space, row, scrollable, text, Space,
};
use iced::{Background, Border, Color, Element, Length, Subscription, Task, Theme};

use cleanator_core::classify::{classify, ClassifyOptions, Recommendation, Report};
use cleanator_core::paths::Env;
use cleanator_core::rules::{Level, RuleSet};
use cleanator_core::scan::{scan, DirNode, Scan, ScanOptions, ScanProgress};
use cleanator_core::script::{generate, ScriptOptions, Selection};
use cleanator_core::size::{human, ymd};

use bbl_ui::palette::{alert, blend, good, hex, neutral as grey};
use bbl_ui::widgets::{card, pill, swatch};
use bbl_ui::{Lang, Prefs, Strings};
use treemap::{Item, Treemap, TreemapEvent};

const EN: &str = include_str!("../../../locales/en.json");
const FR: &str = include_str!("../../../locales/fr.json");

/// Window icon, embedded so the binary is self-sufficient (the `.desktop` entry uses the
/// same picture from `assets/`).
const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon-256.png");

/// The first positional argument: the directory to scan.
fn root_arg() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--select" {
            args.next();
        } else if !a.starts_with("--") {
            return Some(PathBuf::from(a));
        }
    }
    None
}

/// The `--select` argument, if any.
fn requested_selection() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--select" {
            return args.next();
        }
        if let Some(v) = a.strip_prefix("--select=") {
            return Some(v.to_string());
        }
    }
    None
}

fn main() -> iced::Result {
    let icon = iced::window::icon::from_file_data(ICON_PNG, None).ok();
    iced::application(App::title, App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1320.0, 860.0),
            min_size: Some(iced::Size::new(960.0, 640.0)),
            icon,
            ..Default::default()
        })
        .run_with(App::new)
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    Scanned(Result<(Arc<Scan>, Arc<Report>), String>),
    Treemap(TreemapEvent),
    SetView(View),
    Up,
    Toggle(String, bool),
    ToggleGroup(String, bool),
    ToggleIds(Vec<String>, bool),
    Expand(String),
    OpenGroup(String),
    OpenDocs(String),
    SelectSafe,
    Clear,
    CopyScript,
    SaveScript,
    ToggleTheme,
    SetLang(Lang),
}

impl From<TreemapEvent> for Message {
    fn from(e: TreemapEvent) -> Self {
        Message::Treemap(e)
    }
}

enum Phase {
    Scanning,
    Ready,
    Failed(String),
}

/// What the background thread is doing, for the loading screen.
const STAGE_WALKING: u8 = 0;
const STAGE_RULES: u8 = 1;

/// Shared with the background thread: live counters and the current stage.
struct Work {
    progress: Arc<ScanProgress>,
    stage: Arc<AtomicU8>,
    started: Instant,
}

impl Work {
    fn new() -> Work {
        Work {
            progress: Arc::new(ScanProgress::default()),
            stage: Arc::new(AtomicU8::new(STAGE_WALKING)),
            started: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Explorer,
    Assistants,
    Projects,
}

struct App {
    prefs: Prefs<()>,
    lang: Lang,
    strings: Strings,
    dark: bool,
    root: PathBuf,
    phase: Phase,
    scan: Option<Arc<Scan>>,
    report: Option<Arc<Report>>,
    current: PathBuf,
    selected: HashSet<String>,
    expanded: HashSet<String>,
    open_groups: HashSet<String>,
    status: String,
    view: View,
    work: Work,
}

/// Rules with more items than this are shown as one group row, opened on demand.
const GROUP_THRESHOLD: usize = 6;

impl App {
    fn new() -> (App, Task<Message>) {
        let env = Env::current();
        let root = root_arg().unwrap_or_else(|| env.home.clone());
        let prefs: Prefs<()> = Prefs::load("cleanator");
        let lang = prefs.initial_lang();
        let strings = Strings::load(EN, FR, lang);
        let status = strings.t("status.scanning", &[("path", &root.to_string_lossy())]);
        let dark = prefs.initial_dark();
        let work = Work::new();
        let task = start_scan(root.clone(), &work);
        let app = App {
            prefs,
            lang,
            strings,
            dark,
            root: root.clone(),
            phase: Phase::Scanning,
            scan: None,
            report: None,
            current: root.clone(),
            selected: HashSet::new(),
            expanded: HashSet::new(),
            open_groups: HashSet::new(),
            status,
            view: View::Explorer,
            work,
        };
        (app, task)
    }

    /// Open the group of a rule (or of one item) and move the explorer to its folder.
    fn apply_selection(&mut self, want: &str) {
        let Some(report) = self.report.clone() else {
            return;
        };
        let (rule_id, item) = match want.split_once("::") {
            Some((r, _)) => (r, Some(want)),
            None => (want, None),
        };
        let Some(first) = report.recommendations.iter().find(|r| match item {
            Some(id) => r.id == id,
            None => r.rule_id == rule_id && !r.recent && !r.negligible && r.in_use.is_none(),
        }) else {
            return;
        };
        self.open_groups.insert(format!("{}:{}", first.effective_level, first.rule_id));
        if item.is_some() && first.effective_level != Level::Never {
            self.selected.insert(first.id.clone());
        }
        if let Some(p) = first.path.as_deref().map(PathBuf::from) {
            let folder = if p.is_dir() { Some(p.as_path()) } else { p.parent() };
            if let Some(f) = folder {
                if self.scan.as_ref().is_some_and(|s| s.tree.find(f).is_some()) {
                    self.current = f.to_path_buf();
                }
            }
        }
    }

    fn title(&self) -> String {
        self.strings.t("app.title", &[])
    }

    fn subscription(&self) -> Subscription<Message> {
        match self.phase {
            Phase::Scanning => iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick),
            _ => Subscription::none(),
        }
    }

    fn theme(&self) -> Theme {
        if self.dark {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    fn t(&self, key: &str) -> String {
        self.strings.t(key, &[])
    }

    /// Persist language and theme so the next launch starts the same way.
    fn remember(&mut self) {
        self.prefs.remember(self.lang, self.dark);
        self.prefs.save();
    }

    /// The footer line, in the current language, for the current phase.
    fn refresh_status(&mut self) {
        if matches!(self.phase, Phase::Scanning) {
            self.status = self
                .strings
                .t("status.scanning", &[("path", &self.root.to_string_lossy())]);
            return;
        }
        if let Phase::Failed(e) = &self.phase {
            self.status = self.strings.t("status.error", &[("error", e)]);
            return;
        }
        if let (Some(scan), Some(report)) = (&self.scan, &self.report) {
            self.status = self.strings.t(
                "status.scanned",
                &[
                    ("size", &human(scan.summary.total_size)),
                    (
                        "secs",
                        &format!("{:.1}", scan.summary.duration_ms as f64 / 1000.0),
                    ),
                    ("rules", &report.rule_count.to_string()),
                    ("dormancy", &report.dormancy_days.to_string()),
                ],
            );
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // The loading screen reads the live counters on every view; the tick only
            // forces a redraw.
            Message::Tick => {}
            Message::Scanned(Ok((scan, report))) => {
                self.current = scan.summary.root.clone();
                self.scan = Some(scan);
                self.report = Some(report);
                self.phase = Phase::Ready;
                // `--select <rule-id>` or `--select <rule-id::key>` (the control tower): open
                // the group, show the first item's folder, tick the item when one is named.
                if let Some(want) = requested_selection() {
                    self.apply_selection(&want);
                }
                self.refresh_status();
            }
            Message::Scanned(Err(e)) => {
                self.status = self.strings.t("status.error", &[("error", &e)]);
                self.phase = Phase::Failed(e);
            }
            Message::Treemap(TreemapEvent::Open(p)) => {
                if self
                    .scan
                    .as_ref()
                    .is_some_and(|s| s.tree.find(&p).is_some())
                {
                    self.current = p;
                }
            }
            Message::SetView(v) => self.view = v,
            Message::Up => {
                if self.current != self.root {
                    if let Some(parent) = self.current.parent() {
                        self.current = parent.to_path_buf();
                    }
                }
            }
            Message::Toggle(id, on) => {
                if on {
                    self.selected.insert(id);
                } else {
                    self.selected.remove(&id);
                }
            }
            Message::ToggleGroup(rule_id, on) => {
                if let Some(r) = &self.report {
                    for rec in r
                        .recommendations
                        .iter()
                        .filter(|x| x.rule_id == rule_id && selectable(x))
                    {
                        if on {
                            self.selected.insert(rec.id.clone());
                        } else {
                            self.selected.remove(&rec.id);
                        }
                    }
                }
            }
            Message::ToggleIds(ids, on) => {
                for id in ids {
                    if on {
                        self.selected.insert(id);
                    } else {
                        self.selected.remove(&id);
                    }
                }
            }
            Message::Expand(id) => {
                if !self.expanded.remove(&id) {
                    self.expanded.insert(id);
                }
            }
            Message::OpenGroup(key) => {
                if !self.open_groups.remove(&key) {
                    self.open_groups.insert(key);
                }
            }
            Message::OpenDocs(url) => {
                if url.starts_with("http") {
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                }
            }
            Message::SelectSafe => {
                if let Some(r) = &self.report {
                    for rec in &r.recommendations {
                        if rec.effective_level == Level::Safe && selectable(rec) {
                            self.selected.insert(rec.id.clone());
                        }
                    }
                }
            }
            Message::Clear => self.selected.clear(),
            Message::CopyScript => {
                if let Some((script, n)) = self.script() {
                    self.status = self.strings.t("panel.copied", &[("n", &n.to_string())]);
                    return iced::clipboard::write(script);
                }
                self.status = self.t("panel.nothing");
            }
            Message::SaveScript => {
                if let Some((script, n)) = self.script() {
                    let path = Env::current().home.join("cleanator-cleanup.sh");
                    match std::fs::write(&path, script) {
                        Ok(()) => {
                            self.status = self.strings.t(
                                "panel.saved",
                                &[("path", &path.to_string_lossy()), ("n", &n.to_string())],
                            )
                        }
                        Err(e) => {
                            self.status =
                                self.strings.t("status.error", &[("error", &e.to_string())])
                        }
                    }
                } else {
                    self.status = self.t("panel.nothing");
                }
            }
            Message::ToggleTheme => {
                self.dark = !self.dark;
                self.remember();
            }
            Message::SetLang(lang) => {
                if lang != self.lang {
                    self.lang = lang;
                    self.strings = Strings::load(EN, FR, lang);
                    self.refresh_status();
                    self.remember();
                }
            }
        }
        Task::none()
    }

    fn script(&self) -> Option<(String, usize)> {
        let report = self.report.as_ref()?;
        if self.selected.is_empty() {
            return None;
        }
        let ids: Vec<String> = self.selected.iter().cloned().collect();
        let (text, n) = generate(
            report,
            &ScriptOptions {
                selection: Selection::Ids(ids),
                include_in_use: false,
            },
        );
        Some((text, n))
    }

    // ------------------------------------------------------------------ view

    fn view(&self) -> Element<'_, Message> {
        let header = self.view_header();
        let body: Element<'_, Message> = match (&self.phase, &self.scan, &self.report) {
            (Phase::Ready, Some(scan), Some(report)) => {
                row![self.view_left(scan, report), self.view_panel(report)]
                    .spacing(14)
                    .height(Length::Fill)
                    .into()
            }
            (Phase::Failed(e), _, _) => container(text(e.clone()).size(16))
                .center(Length::Fill)
                .into(),
            _ => self.view_loading(),
        };
        let footer = bbl_ui::widgets::footer(
            self.status.clone(),
            self.lang,
            Message::SetLang,
            self.t("theme.toggle"),
            Message::ToggleTheme,
        );
        let mut page = column![header].spacing(14).padding(16);
        if let Some(notice) = self.view_atime_notice() {
            page = page.push(notice);
        }
        page.push(body).push(footer).into()
    }

    /// One line when access times could not be trusted (0.1.1 design, D4): what was
    /// found, what it changed, what to do.
    fn view_atime_notice(&self) -> Option<Element<'_, Message>> {
        let report = self.report.as_ref()?;
        let a = &report.atime;
        let mut lines: Vec<String> = Vec::new();
        if a.unknown_items > 0 {
            lines.push(self.strings.t(
                "atime.noatime",
                &[
                    ("n", &a.unknown_items.to_string()),
                    ("size", &human(a.unknown_size)),
                    ("mounts", &a.noatime_mounts.join(", ")),
                ],
            ));
        }
        if let (Some(m), Some(until)) = (a.mass_read, a.deferred_until) {
            if a.deferred_items > 0 {
                lines.push(self.strings.t(
                    "atime.mass_read",
                    &[
                        ("day", &ymd(m.day)),
                        ("n", &a.deferred_items.to_string()),
                        ("size", &human(a.deferred_size)),
                        ("until", &ymd(until)),
                    ],
                ));
            }
        }
        if lines.is_empty() {
            return None;
        }
        let dot = swatch(level_color(Level::Review, self.dark), 8);
        let mut col = column![].spacing(2);
        for l in lines {
            col = col.push(text(l).size(12));
        }
        Some(
            container(row![dot, col].spacing(10).align_y(iced::Alignment::Center))
                .padding(iced::Padding {
                    top: 8.0,
                    right: 14.0,
                    bottom: 8.0,
                    left: 14.0,
                })
                .width(Length::Fill)
                .style(|theme: &Theme| {
                    let p = theme.extended_palette();
                    container::Style {
                        background: Some(Background::Color(p.background.weak.color)),
                        border: Border {
                            radius: 8.0.into(),
                            width: 1.0,
                            color: p.background.strong.color,
                        },
                        ..Default::default()
                    }
                })
                .into(),
        )
    }

    /// Shown while the background thread walks the disk, then applies the rules: what is
    /// being read, how much so far, and the promise that nothing is deleted.
    fn view_loading(&self) -> Element<'_, Message> {
        let p = &self.work.progress;
        let secs = self.work.started.elapsed().as_secs_f64();
        let stage = match self.work.stage.load(Ordering::Relaxed) {
            STAGE_RULES => self.t("loading.rules"),
            _ => self.t("loading.walking"),
        };
        let counters = self.strings.t(
            "loading.progress",
            &[
                ("size", &human(p.bytes())),
                ("files", &group_thousands(p.files())),
                ("dirs", &group_thousands(p.dirs())),
                ("secs", &format!("{secs:.0}")),
            ],
        );
        // A simple pulse: a dot travelling along a short track, no spinner widget needed.
        let frame = (secs * 4.0) as usize % 8;
        let mut track = row![].spacing(6);
        for i in 0..8 {
            let c = if i == frame {
                self.theme().palette().primary
            } else {
                tile_neutral(self.dark)
            };
            track = track.push(swatch(c, 8));
        }
        let body = column![
            text(self.t("loading.title")).size(22),
            text(shorten(&self.root.to_string_lossy(), 72))
                .size(13)
                .style(text::secondary),
            Space::with_height(10),
            track,
            Space::with_height(10),
            text(stage).size(14),
            text(counters)
                .size(12)
                .font(iced::Font::MONOSPACE)
                .style(text::secondary),
            Space::with_height(18),
            text(self.t("loading.promise"))
                .size(12)
                .style(text::secondary),
        ]
        .spacing(6)
        .align_x(iced::Alignment::Center);
        container(card(body.into()).padding(32))
            .center(Length::Fill)
            .into()
    }

    fn view_header(&self) -> Element<'_, Message> {
        let (scanned, safe, review, in_use, never, n_safe, n_review) = match &self.report {
            Some(r) => (
                Some(r.scan.total_size),
                Some(r.totals.safe),
                Some(r.totals.review),
                Some(r.totals.in_use),
                Some(r.totals.never),
                r.totals.counts.get("safe").copied().unwrap_or(0),
                r.totals.counts.get("review").copied().unwrap_or(0),
            ),
            None => (None, None, None, None, None, 0, 0),
        };
        let items = |n: u64| {
            if self.report.is_none() {
                String::new()
            } else {
                self.strings.t("kpi.items", &[("n", &n.to_string())])
            }
        };
        row![
            self.kpi(
                &self.t("kpi.scanned"),
                scanned,
                &self.root.to_string_lossy(),
                None
            ),
            self.kpi(
                &self.t("kpi.safe"),
                safe,
                &items(n_safe),
                Some(level_color(Level::Safe, self.dark))
            ),
            self.kpi(
                &self.t("kpi.review"),
                review,
                &items(n_review),
                Some(level_color(Level::Review, self.dark))
            ),
            self.kpi(&self.t("kpi.in_use"), in_use, "", None),
            self.kpi(&self.t("kpi.never"), never, "", None),
        ]
        .spacing(12)
        .into()
    }

    fn kpi<'a>(
        &self,
        label: &str,
        value: Option<u64>,
        sub: &str,
        dot: Option<Color>,
    ) -> Element<'a, Message> {
        let sub_text = if sub.is_empty() {
            String::new()
        } else {
            shorten(sub, 34)
        };
        bbl_ui::widgets::kpi(
            label,
            value.map(human).unwrap_or_else(|| "—".into()),
            sub_text,
            dot,
        )
    }

    fn view_left<'a>(&'a self, scan: &'a Scan, report: &'a Report) -> Element<'a, Message> {
        let tab = |label: String, v: View, current: View| -> Element<'a, Message> {
            let b = button(text(label).size(12)).padding([4, 10]);
            if v == current {
                b.style(button::primary).into()
            } else {
                b.style(button::text).on_press(Message::SetView(v)).into()
            }
        };
        let tabs = row![
            tab(self.t("tabs.explorer"), View::Explorer, self.view),
            tab(self.t("tabs.assistants"), View::Assistants, self.view),
            tab(self.t("tabs.projects"), View::Projects, self.view),
        ]
        .spacing(4);
        let body: Element<'a, Message> = match self.view {
            View::Explorer => self.view_explorer(scan, report),
            View::Assistants => self.view_assistants(report),
            View::Projects => self.view_projects(scan, report),
        };
        column![tabs, body]
            .spacing(8)
            .width(Length::FillPortion(13))
            .height(Length::Fill)
            .into()
    }

    /// Per assistant: configuration (kept), session history (review) and caches (safe),
    /// including items classified but recent, so the picture is complete.
    fn view_assistants<'a>(&'a self, report: &'a Report) -> Element<'a, Message> {
        #[derive(Default)]
        struct Row {
            config: u64,
            history: u64,
            caches: u64,
            sessions: usize,
            oldest: Option<i64>,
        }
        let mut rows: BTreeMap<&'static str, Row> = BTreeMap::new();
        for r in &report.recommendations {
            if !r.category.starts_with("assistant") {
                continue;
            }
            let Some(name) = assistant_of(r.path.as_deref().unwrap_or(&r.display)) else {
                continue;
            };
            let e = rows.entry(name).or_default();
            match r.category.as_str() {
                "assistant-config" => e.config += r.size,
                "assistant-history" => {
                    e.history += r.size;
                    e.sessions += 1;
                    e.oldest = match (e.oldest, r.last_used) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (None, b) => b,
                        (a, None) => a,
                    };
                }
                _ => e.caches += r.size,
            }
        }
        let mut sorted: Vec<(&str, Row)> = rows.into_iter().collect();
        sorted.sort_by_key(|(_, r)| std::cmp::Reverse(r.config + r.history + r.caches));

        let header = row![
            text(self.t("assistants.name"))
                .size(11)
                .style(text::secondary)
                .width(Length::FillPortion(3)),
            text(self.t("assistants.total"))
                .size(11)
                .style(text::secondary)
                .width(Length::FillPortion(1)),
            text(self.t("assistants.config"))
                .size(11)
                .style(text::secondary)
                .width(Length::FillPortion(1)),
            text(self.t("assistants.history"))
                .size(11)
                .style(text::secondary)
                .width(Length::FillPortion(1)),
            text(self.t("assistants.caches"))
                .size(11)
                .style(text::secondary)
                .width(Length::FillPortion(1)),
            text(self.t("assistants.sessions"))
                .size(11)
                .style(text::secondary)
                .width(Length::FillPortion(2)),
        ]
        .spacing(8);
        let mut list = column![header].spacing(6);
        if sorted.is_empty() {
            list = list.push(
                text(self.t("assistants.none"))
                    .size(12)
                    .style(text::secondary),
            );
        }
        for (name, r) in &sorted {
            let name = if *name == OTHER_ASSISTANT {
                self.t("assistants.other")
            } else {
                name.to_string()
            };
            let total = r.config + r.history + r.caches;
            let sessions = if r.sessions == 0 {
                "—".to_string()
            } else {
                format!(
                    "{} · {} {}",
                    r.sessions,
                    self.t("assistants.oldest"),
                    r.oldest.map(ymd).unwrap_or_else(|| "—".into())
                )
            };
            let mono = |v: u64| {
                text(human(v))
                    .size(12)
                    .font(iced::Font::MONOSPACE)
                    .width(Length::FillPortion(1))
            };
            list = list.push(
                row![
                    row![
                        swatch(category_color("assistant-history", self.dark), 8),
                        text(name).size(13)
                    ]
                    .spacing(6)
                    .align_y(iced::Alignment::Center)
                    .width(Length::FillPortion(3)),
                    mono(total),
                    mono(r.config),
                    mono(r.history),
                    mono(r.caches),
                    text(sessions)
                        .size(11)
                        .style(text::secondary)
                        .width(Length::FillPortion(2)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }
        let head = column![
            text(self.t("assistants.title")).size(16),
            text(self.t("assistants.hint"))
                .size(11)
                .style(text::secondary),
        ]
        .spacing(2);
        card(
            column![head, scrollable(list).height(Length::Fill)]
                .spacing(12)
                .into(),
        )
        .height(Length::Fill)
        .into()
    }

    /// Per project: regenerable weight (dependencies, builds, environments), last touch,
    /// active or dormant, with one checkbox for everything selectable in it.
    fn view_projects<'a>(&'a self, scan: &'a Scan, report: &'a Report) -> Element<'a, Message> {
        #[derive(Default)]
        struct Row {
            regenerable: u64,
            items: Vec<String>,
            selectable: Vec<String>,
            selectable_size: u64,
        }
        let mut rows: BTreeMap<&str, Row> = BTreeMap::new();
        for r in &report.recommendations {
            let Some(p) = &r.project else { continue };
            let e = rows.entry(p.as_str()).or_default();
            e.regenerable += r.size;
            e.items.push(r.id.clone());
            if selectable(r) {
                e.selectable.push(r.id.clone());
                e.selectable_size += r.reclaimable;
            }
        }
        let mut sorted: Vec<(&str, Row)> = rows.into_iter().collect();
        sorted.sort_by_key(|(_, r)| std::cmp::Reverse(r.regenerable));
        let window = report.dormancy_days as i64 * 86_400;
        let now = chrono_now();
        let home = self.root.to_string_lossy().to_string();

        let mut list = column![].spacing(6).width(Length::Fill);
        let (mut dormant_n, mut dormant_size) = (0usize, 0u64);
        for (path, r) in &sorted {
            let node = scan.tree.find(Path::new(path));
            let touched = node.map(|n| n.newest_source_mtime);
            let dormant = touched.is_some_and(|t| now - t >= window);
            if dormant {
                dormant_n += 1;
                dormant_size += r.regenerable;
            }
            let all_on = !r.selectable.is_empty()
                && r.selectable.iter().all(|id| self.selected.contains(id));
            let label = format!(
                "{:>7}  {}",
                human(r.regenerable),
                shorten(&path.replacen(&home, "~", 1), 72)
            );
            let ids = r.selectable.clone();
            let mut cb = checkbox(label, all_on)
                .size(16)
                .text_size(13)
                .width(Length::Fill);
            if !ids.is_empty() {
                cb = cb.on_toggle(move |on| Message::ToggleIds(ids.clone(), on));
            }
            let state = if dormant {
                self.t("projects.dormant")
            } else {
                self.t("projects.active")
            };
            let meta = format!(
                "{} {} · {} · {} {} {}",
                self.t("projects.touched"),
                touched.map(ymd).unwrap_or_else(|| "—".into()),
                state,
                self.strings.t(
                    if r.items.len() == 1 {
                        "projects.item"
                    } else {
                        "projects.items"
                    },
                    &[("n", &r.items.len().to_string())]
                ),
                self.t("projects.selectable"),
                human(r.selectable_size)
            );
            list = list.push(
                column![
                    row![
                        swatch(
                            if dormant {
                                level_color(Level::Safe, self.dark)
                            } else {
                                level_color(Level::Review, self.dark)
                            },
                            8
                        ),
                        cb
                    ]
                    .spacing(6)
                    .align_y(iced::Alignment::Center),
                    text(meta).size(10).style(text::secondary),
                ]
                .spacing(1),
            );
        }
        if sorted.is_empty() {
            list = list.push(
                text(self.t("projects.none"))
                    .size(12)
                    .style(text::secondary),
            );
        }
        let head = column![
            text(self.t("projects.title")).size(16),
            text(self.strings.t(
                "projects.hint",
                &[
                    ("n", &dormant_n.to_string()),
                    ("size", &human(dormant_size)),
                    ("days", &report.dormancy_days.to_string())
                ]
            ))
            .size(11)
            .style(text::secondary),
        ]
        .spacing(2);
        card(
            column![head, scrollable(list).height(Length::Fill)]
                .spacing(12)
                .width(Length::Fill)
                .into(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn view_explorer<'a>(&'a self, scan: &'a Scan, report: &'a Report) -> Element<'a, Message> {
        let node = scan.tree.find(&self.current).unwrap_or(&scan.tree);
        let items = self.items_for(node, report);
        let rel = self
            .current
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let crumb = if rel.is_empty() {
            format!("~  ·  {}", human(node.size))
        } else {
            format!("~/{rel}  ·  {}", human(node.size))
        };
        let mut up = button(text(self.t("treemap.up")).size(12)).style(button::secondary);
        if self.current != self.root {
            up = up.on_press(Message::Up);
        }
        let head = row![
            column![
                text(self.t("treemap.title")).size(16),
                text(self.t("treemap.hint")).size(11).style(text::secondary),
            ]
            .spacing(2),
            horizontal_space(),
            text(crumb).size(12).font(iced::Font::MONOSPACE),
            up,
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        let map = canvas(Treemap {
            items,
            dark: self.dark,
        })
        .width(Length::Fill)
        .height(Length::Fill);
        card(column![head, map, self.view_legend()].spacing(10).into())
            .height(Length::Fill)
            .into()
    }

    fn view_legend(&self) -> Element<'_, Message> {
        // Two rows: iced 0.13 has no wrapping row, and nine entries overflow one line.
        let mut rows = column![].spacing(4);
        for chunk in LEGEND.chunks(5) {
            let mut r = row![].spacing(14);
            for cat in chunk {
                let label = self.strings.t(&format!("legend.{cat}"), &[]);
                r = r.push(
                    row![
                        swatch(category_color(cat, self.dark), 10),
                        text(label).size(11).style(text::secondary)
                    ]
                    .spacing(5)
                    .align_y(iced::Alignment::Center),
                );
            }
            rows = rows.push(r);
        }
        rows.into()
    }

    /// Tiles for the folder shown: its subfolders (top 40, the rest aggregated), the
    /// files directly inside, and other-filesystem mount points.
    fn items_for(&self, node: &DirNode, report: &Report) -> Vec<Item> {
        let mut items: Vec<Item> = Vec::new();
        let mut others = 0u64;
        for (i, c) in node.children.iter().enumerate() {
            if c.size == 0 && !c.other_fs {
                continue;
            }
            if i >= 40 {
                others += c.size;
                continue;
            }
            items.push(Item {
                label: c.name.clone(),
                size: c.size.max(1),
                color: if c.other_fs {
                    tile_neutral(self.dark)
                } else {
                    color_for(&c.path, c.size, report, self.dark)
                },
                path: if c.other_fs {
                    None
                } else {
                    Some(c.path.clone())
                },
            });
        }
        if node.own_size > 0 {
            items.push(Item {
                label: self.t("treemap.files"),
                size: node.own_size,
                color: tile_neutral(self.dark),
                path: None,
            });
        }
        if others > 0 {
            items.push(Item {
                label: "…".into(),
                size: others,
                color: tile_neutral(self.dark),
                path: None,
            });
        }
        items
    }

    fn view_panel<'a>(&'a self, report: &'a Report) -> Element<'a, Message> {
        let head = column![
            text(self.t("panel.title")).size(16),
            text(self.t("panel.subtitle"))
                .size(11)
                .style(text::secondary),
        ]
        .spacing(2);

        let mut list = column![].spacing(4);
        for level in [Level::Safe, Level::Review] {
            let recs: Vec<&Recommendation> = report
                .recommendations
                .iter()
                .filter(|r| r.effective_level == level && !r.recent && !r.negligible)
                .collect();
            if recs.is_empty() {
                continue;
            }
            let total: u64 = recs
                .iter()
                .filter(|r| r.in_use.is_none())
                .map(|r| r.reclaimable)
                .sum();
            let label = if level == Level::Safe {
                self.t("panel.safe")
            } else {
                self.t("panel.review")
            };
            list = list.push(
                row![
                    pill(label, level_color(level, self.dark)),
                    text(human(total))
                        .size(12)
                        .font(iced::Font::MONOSPACE)
                        .style(text::secondary),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center)
                .padding(iced::Padding {
                    top: 10.0,
                    right: 2.0,
                    bottom: 4.0,
                    left: 2.0,
                }),
            );

            // Rules with many items become one group row; the rest stay individual rows;
            // everything sorted by size.
            let mut by_rule: BTreeMap<&str, Vec<&Recommendation>> = BTreeMap::new();
            for r in &recs {
                by_rule.entry(r.rule_id.as_str()).or_default().push(r);
            }
            enum Row<'r> {
                One(&'r Recommendation),
                Group(&'r str, Vec<&'r Recommendation>, u64),
            }
            let mut rows: Vec<(u64, Row)> = Vec::new();
            for (rule_id, items) in by_rule {
                if items.len() > GROUP_THRESHOLD {
                    let sum: u64 = items.iter().map(|r| r.reclaimable).sum();
                    rows.push((sum, Row::Group(rule_id, items, sum)));
                } else {
                    for r in items {
                        rows.push((r.reclaimable, Row::One(r)));
                    }
                }
            }
            rows.sort_by_key(|(size, _)| std::cmp::Reverse(*size));
            for (_, r) in rows {
                match r {
                    Row::One(rec) => list = list.push(self.view_rec(rec, 0)),
                    Row::Group(rule_id, items, sum) => {
                        list = list.push(self.view_group(level, rule_id, &items, sum));
                    }
                }
            }
        }

        let recent_n = report
            .recommendations
            .iter()
            .filter(|r| r.recent && r.atime_caveat.is_none())
            .count();
        if recent_n > 0 {
            list = list.push(
                text(self.strings.t(
                    "panel.recent",
                    &[
                        ("n", &recent_n.to_string()),
                        ("days", &report.dormancy_days.to_string()),
                    ],
                ))
                .size(11)
                .style(text::secondary),
            );
        }

        let selected_total: u64 = report
            .recommendations
            .iter()
            .filter(|r| self.selected.contains(&r.id))
            .map(|r| r.reclaimable)
            .sum();
        let footer = column![
            text(self.strings.t(
                "panel.selected",
                &[
                    ("size", &human(selected_total)),
                    ("n", &self.selected.len().to_string())
                ],
            ))
            .size(12),
            row![
                button(text(self.t("panel.select_safe")).size(12))
                    .style(button::secondary)
                    .on_press(Message::SelectSafe),
                button(text(self.t("panel.clear")).size(12))
                    .style(button::text)
                    .on_press(Message::Clear),
                horizontal_space(),
                button(text(self.t("panel.save_script")).size(12))
                    .style(button::secondary)
                    .on_press(Message::SaveScript),
                button(text(self.t("panel.copy_script")).size(12))
                    .style(button::primary)
                    .on_press(Message::CopyScript),
            ]
            .spacing(8),
        ]
        .spacing(8);

        card(
            column![head, scrollable(list).height(Length::Fill), footer]
                .spacing(10)
                .into(),
        )
        .width(Length::FillPortion(8))
        .height(Length::Fill)
        .into()
    }

    /// A rule with many items: one line with a checkbox for all of them, opened on demand.
    fn view_group<'a>(
        &'a self,
        level: Level,
        rule_id: &'a str,
        items: &[&'a Recommendation],
        sum: u64,
    ) -> Element<'a, Message> {
        let key = format!("{level}:{rule_id}");
        let open = self.open_groups.contains(&key);
        let selectable_items: Vec<&&Recommendation> =
            items.iter().filter(|r| selectable(r)).collect();
        let all_on = !selectable_items.is_empty()
            && selectable_items
                .iter()
                .all(|r| self.selected.contains(&r.id));
        let full = &items[0].rule_name;
        let name = full.split(" (").next().unwrap_or(full).to_string();
        let name = &name;
        let label = format!(
            "{:>7}  {}",
            human(sum),
            self.strings.t(
                "panel.group",
                &[("name", name), ("n", &items.len().to_string())]
            )
        );
        let rid = rule_id.to_string();
        let mut cb = checkbox(label, all_on).size(16).text_size(13);
        if !selectable_items.is_empty() {
            cb = cb.on_toggle(move |on| Message::ToggleGroup(rid.clone(), on));
        }
        let caret = button(text(if open { "▾" } else { "▸" }).size(12))
            .style(button::text)
            .padding(2)
            .on_press(Message::OpenGroup(key.clone()));
        let mut col = column![row![caret, cb].spacing(4).align_y(iced::Alignment::Center)]
            .spacing(1)
            .padding(iced::Padding {
                top: 2.0,
                right: 0.0,
                bottom: 6.0,
                left: 0.0,
            });
        if open {
            for rec in items {
                col = col.push(self.view_rec(rec, 22));
            }
        }
        col.into()
    }

    /// One recommendation: a compact line (size, path, rule, why) and, when expanded, the
    /// exact command, the full path, the last use, the cost and a link to the rule's
    /// documentation — the second depth a developer checks before ticking.
    fn view_rec<'a>(&'a self, r: &'a Recommendation, indent: u16) -> Element<'a, Message> {
        let home = self.root.to_string_lossy().to_string();
        let display = shorten(&r.display.replacen(&home, "~", 1), 54);
        let label = format!("{:>7}  {}", human(r.reclaimable), display);
        let id = r.id.clone();
        let mut cb = checkbox(label, self.selected.contains(&r.id))
            .size(16)
            .text_size(13);
        if selectable(r) {
            cb = cb.on_toggle(move |on| Message::Toggle(id.clone(), on));
        }
        let open = self.expanded.contains(&r.id);
        let caret = button(text(if open { "▾" } else { "▸" }).size(12))
            .style(button::text)
            .padding(2)
            .on_press(Message::Expand(r.id.clone()));
        let mut meta = format!("{} · {}", r.rule_id, shorten(&r.why, 70));
        if let Some(u) = &r.in_use {
            meta = format!("{} · {}: {}", r.rule_id, self.t("panel.in_use"), u);
        }
        let mut col = column![
            row![caret, cb].spacing(4).align_y(iced::Alignment::Center),
            text(meta).size(10).style(text::secondary),
        ]
        .spacing(1)
        .padding(iced::Padding {
            top: 2.0,
            right: 0.0,
            bottom: 6.0,
            left: indent as f32,
        });
        if open {
            let mono = |s: String| text(s).size(11).font(iced::Font::MONOSPACE);
            let mut details = column![].spacing(3);
            if let Some(p) = &r.path {
                details = details.push(mono(p.clone()));
            }
            if let Some(c) = &r.command {
                details = details.push(mono(format!("$ {c}")));
            }
            details = details.push(
                text(format!(
                    "{}: {} · {}: {} · {}: {}",
                    self.t("detail.last_used"),
                    r.last_used.map(ymd).unwrap_or_else(|| "—".into()),
                    self.t("detail.regenerate"),
                    r.regenerate,
                    self.t("detail.cost"),
                    r.cost
                ))
                .size(11)
                .style(text::secondary),
            );
            if let Some(n) = &r.notes {
                details = details.push(text(n.clone()).size(11).style(text::secondary));
            }
            let mut links = row![text(format!("{}: {}", self.t("detail.rule"), r.rule_id))
                .size(11)
                .style(text::secondary)]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            if r.source_url.starts_with("http") {
                links = links.push(
                    button(text(self.t("detail.docs")).size(11))
                        .style(button::text)
                        .padding(2)
                        .on_press(Message::OpenDocs(r.source_url.clone())),
                );
            } else if r.source_url == "observed" {
                links = links.push(
                    text(self.t("detail.observed"))
                        .size(11)
                        .style(text::secondary),
                );
            }
            details = details.push(links);
            col = col.push(
                container(details)
                    .padding(iced::Padding {
                        top: 6.0,
                        right: 10.0,
                        bottom: 6.0,
                        left: 10.0,
                    })
                    .style(|theme: &Theme| {
                        let p = theme.extended_palette();
                        container::Style {
                            background: Some(Background::Color(p.background.base.color)),
                            border: Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    })
                    .width(Length::Fill),
            );
        }
        col.into()
    }
}

/// Which assistant a path belongs to, from its well-known locations.
fn assistant_of(path: &str) -> Option<&'static str> {
    const MAP: &[(&str, &str)] = &[
        ("/.claude", "Claude Code"),
        ("/.cache/claude-cli-nodejs", "Claude Code"),
        ("/.config/Claude", "Claude Desktop"),
        ("/.codex", "Codex"),
        ("/.gemini", "Gemini CLI"),
        ("/.config/Cursor", "Cursor"),
        ("/.cursor", "Cursor"),
        ("/.config/Code", "VS Code"),
        ("/.vscode", "VS Code"),
        ("/.config/Windsurf", "Windsurf"),
        ("/.windsurf", "Windsurf"),
        ("/.codeium", "Windsurf"),
        ("/opencode", "OpenCode"),
        ("/.continue", "Continue"),
        ("/JetBrains", "JetBrains"),
        ("/.aider", "Aider"),
        ("/.config/goose", "Goose"),
        ("/.config/amp", "Amp"),
        ("/.cache/zed", "Zed"),
        ("/.hermes", "Hermes"),
    ];
    MAP.iter()
        .find(|(frag, _)| path.contains(frag))
        .map(|(_, name)| *name)
        .or(Some(OTHER_ASSISTANT))
}

/// Sentinel for assistant data outside the known locations; translated in the view.
const OTHER_ASSISTANT: &str = "\u{0}other";

/// `2592431` → `2 592 431` (thin spaces, readable in both languages).
#[allow(clippy::manual_is_multiple_of)] // `is_multiple_of` needs Rust 1.87; keep older toolchains
fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn selectable(r: &Recommendation) -> bool {
    r.in_use.is_none()
        && r.command.is_some()
        && !r.recent
        && !r.negligible
        && r.effective_level != Level::Never
}

// ------------------------------------------------------------------ helpers

fn start_scan(root: PathBuf, work: &Work) -> Task<Message> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    let progress = work.progress.clone();
    let stage = work.stage.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<(Arc<Scan>, Arc<Report>), String> {
            let rules = RuleSet::embedded().map_err(|e| e.to_string())?;
            let opts = ScanOptions {
                progress: Some(progress),
                ..Default::default()
            };
            let scan = scan(&root, opts).map_err(|e| e.to_string())?;
            stage.store(STAGE_RULES, Ordering::Relaxed);
            let env = Env::current();
            let report = classify(&scan, &rules, &env, &ClassifyOptions::default());
            Ok((Arc::new(scan), Arc::new(report)))
        })();
        let _ = tx.send(result);
    });
    Task::perform(
        async move {
            rx.await
                .map_err(|_| "the scan thread stopped unexpectedly".to_string())
                .and_then(|r| r)
        },
        Message::Scanned,
    )
}

fn shorten(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let head = max / 2 - 1;
    let tail = max - head - 1;
    let h: String = s.chars().take(head).collect();
    let t: String = s.chars().skip(n - tail).collect();
    format!("{h}…{t}")
}

/// Legend categories; labels come from `locales/*.json` (`legend.<category>`).
const LEGEND: &[&str] = &[
    "model",
    "python-env",
    "package-cache",
    "build",
    "docker",
    "assistant-history",
    "backup",
    "git-reclonable",
    "system",
];

/// Categorical palette: eight hues validated for colour-vision deficiency and contrast
/// in each mode (dataviz reference palette; the dark steps are the same hues re-stepped
/// for the dark surface, not a flip). The ninth category, system, is a neutral grey.
fn category_color(cat: &str, dark: bool) -> Color {
    let (light, dk): (u32, u32) = match cat {
        "model" | "dataset" | "training-output" => (0x6250d6, 0x9085e9), // violet
        "python-env" => (0xeda100, 0xc98500),                            // yellow
        "package-cache" | "tooling" => (0x2a78d6, 0x3987e5),             // blue
        "build" => (0xe34948, 0xe66767),                                 // red
        "docker" => (0x1baf7a, 0x199e70),                                // aqua
        "assistant-history" | "assistant-cache" | "assistant-config" => (0xe87ba4, 0xd55181), // magenta
        "backup" | "large-file" => (0xeb6834, 0xd95926), // orange
        "git-reclonable" => (0x008300, 0x008300),        // green
        "system" | "system-index" | "browser" | "snapshot" => (0x7a869a, 0x8a94a6), // grey
        _ => (0x9a9aa0, 0x6b6e78),
    };
    hex(if dark { dk } else { light })
}

fn level_color(level: Level, dark: bool) -> Color {
    match level {
        Level::Safe => good(dark),
        Level::Review => alert(dark),
        Level::Never => grey(),
    }
}

/// The tile colour of a folder nothing is classified under (not the family's grey).
fn tile_neutral(dark: bool) -> Color {
    if dark {
        hex(0x3a3d45)
    } else {
        hex(0xd9dce3)
    }
}

/// Colour of a folder tile: the dominant rule category of what lies at or below it
/// (classified items, proposed or recent), when they make up at least a third of the
/// folder; a paler version otherwise; neutral when nothing is classified.
fn color_for(path: &Path, size: u64, report: &Report, dark: bool) -> Color {
    let mut by_cat: BTreeMap<&str, u64> = BTreeMap::new();
    for r in &report.recommendations {
        let Some(p) = &r.path else { continue };
        let p = Path::new(p);
        if p == path || p.starts_with(path) {
            *by_cat.entry(r.category.as_str()).or_default() += r.size;
        } else if path.starts_with(p) {
            return category_color(&r.category, dark);
        }
    }
    let Some((cat, total)) = by_cat.iter().max_by_key(|(_, s)| **s) else {
        return tile_neutral(dark);
    };
    if size > 0 && *total * 3 >= size {
        category_color(cat, dark)
    } else {
        blend(category_color(cat, dark), tile_neutral(dark), 0.55)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1_000), "1\u{202f}000");
        assert_eq!(group_thousands(2_592_431), "2\u{202f}592\u{202f}431");
    }

    #[test]
    fn other_assistant_is_translated_in_both_languages() {
        for lang in [Lang::En, Lang::Fr] {
            let s = Strings::load(EN, FR, lang);
            let other = s.t("assistants.other", &[]);
            assert!(!other.is_empty() && !other.starts_with("assistants."));
        }
        assert_eq!(
            assistant_of("/home/x/.claude/projects"),
            Some("Claude Code")
        );
        assert_eq!(assistant_of("/home/x/.somewhere"), Some(OTHER_ASSISTANT));
    }
}
