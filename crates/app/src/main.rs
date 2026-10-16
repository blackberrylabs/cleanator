//! Cleanator desktop app (iced). One window, three gestures: look, tick, copy the script.
//! Runs the core in a background thread; never deletes anything (R1).

mod i18n;
mod treemap;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::widget::{
    button, canvas, checkbox, column, container, horizontal_space, row, scrollable, text, Space,
};
use iced::{Background, Border, Color, Element, Length, Task, Theme};

use cleanator_core::classify::{classify, ClassifyOptions, Recommendation, Report};
use cleanator_core::paths::Env;
use cleanator_core::rules::{Level, RuleSet};
use cleanator_core::scan::{scan, DirNode, Scan, ScanOptions};
use cleanator_core::script::{generate, ScriptOptions, Selection};
use cleanator_core::size::{human, ymd};

use i18n::{Lang, Strings};
use treemap::{Item, Treemap, TreemapEvent};

fn main() -> iced::Result {
    iced::application(App::title, App::update, App::view)
        .theme(App::theme)
        .window_size((1320.0, 860.0))
        .run_with(App::new)
}

#[derive(Debug, Clone)]
enum Message {
    Scanned(Result<(Arc<Scan>, Arc<Report>), String>),
    Treemap(TreemapEvent),
    Up,
    Toggle(String, bool),
    ToggleGroup(String, bool),
    Expand(String),
    OpenGroup(String),
    OpenDocs(String),
    SelectSafe,
    Clear,
    CopyScript,
    SaveScript,
    ToggleTheme,
    ToggleLang,
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

struct App {
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
}

/// Rules with more items than this are shown as one group row, opened on demand.
const GROUP_THRESHOLD: usize = 6;

impl App {
    fn new() -> (App, Task<Message>) {
        let env = Env::current();
        let root = std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| env.home.clone());
        let lang = Lang::from_env();
        let strings = Strings::load(lang);
        let status = strings.t("status.scanning", &[("path", &root.to_string_lossy())]);
        let dark = matches!(std::env::var("CLEANATOR_THEME").as_deref(), Ok("dark"));
        let app = App {
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
        };
        (app, start_scan(root))
    }

    fn title(&self) -> String {
        self.strings.t("app.title", &[])
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

    fn refresh_status(&mut self) {
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
            Message::Scanned(Ok((scan, report))) => {
                self.current = scan.summary.root.clone();
                self.scan = Some(scan);
                self.report = Some(report);
                self.phase = Phase::Ready;
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
            Message::ToggleTheme => self.dark = !self.dark,
            Message::ToggleLang => {
                self.lang = if self.lang == Lang::Fr {
                    Lang::En
                } else {
                    Lang::Fr
                };
                self.strings = Strings::load(self.lang);
                self.refresh_status();
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
                row![self.view_explorer(scan, report), self.view_panel(report)]
                    .spacing(14)
                    .height(Length::Fill)
                    .into()
            }
            (Phase::Failed(e), _, _) => container(text(e.clone()).size(16))
                .center(Length::Fill)
                .into(),
            _ => container(text(self.status.clone()).size(18))
                .center(Length::Fill)
                .into(),
        };
        let footer = row![
            text(self.status.clone()).size(12).style(text::secondary),
            horizontal_space(),
            button(text(self.t("lang.toggle")).size(12))
                .style(button::text)
                .on_press(Message::ToggleLang),
            button(text(self.t("theme.toggle")).size(12))
                .style(button::text)
                .on_press(Message::ToggleTheme),
        ]
        .align_y(iced::Alignment::Center);
        column![header, body, footer].spacing(14).padding(16).into()
    }

    fn view_header(&self) -> Element<'_, Message> {
        let (scanned, safe, review, in_use, never, n_safe, n_review) = match &self.report {
            Some(r) => (
                r.scan.total_size,
                r.totals.safe,
                r.totals.review,
                r.totals.in_use,
                r.totals.never,
                r.totals.counts.get("safe").copied().unwrap_or(0),
                r.totals.counts.get("review").copied().unwrap_or(0),
            ),
            None => (0, 0, 0, 0, 0, 0, 0),
        };
        let items = |n: u64| self.strings.t("kpi.items", &[("n", &n.to_string())]);
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
        value: u64,
        sub: &str,
        dot: Option<Color>,
    ) -> Element<'a, Message> {
        let mut head = row![].spacing(6).align_y(iced::Alignment::Center);
        if let Some(c) = dot {
            head = head.push(swatch(c, 8));
        }
        head = head.push(text(label.to_string()).size(12).style(text::secondary));
        let sub_text = if sub.is_empty() {
            String::new()
        } else {
            shorten(sub, 34)
        };
        card(
            column![
                head,
                text(human(value)).size(28),
                text(sub_text).size(11).style(text::secondary),
            ]
            .spacing(2)
            .into(),
        )
        .width(Length::Fill)
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
            .width(Length::FillPortion(13))
            .height(Length::Fill)
            .into()
    }

    fn view_legend(&self) -> Element<'_, Message> {
        let mut r = row![].spacing(14);
        for cat in LEGEND {
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
        r.into()
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
                    neutral(self.dark)
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
                color: neutral(self.dark),
                path: None,
            });
        }
        if others > 0 {
            items.push(Item {
                label: "…".into(),
                size: others,
                color: neutral(self.dark),
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
                .filter(|r| r.effective_level == level && !r.recent)
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

        let recent_n = report.recommendations.iter().filter(|r| r.recent).count();
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
        let name = &items[0].rule_name;
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

fn selectable(r: &Recommendation) -> bool {
    r.in_use.is_none() && r.command.is_some() && !r.recent && r.effective_level != Level::Never
}

// ------------------------------------------------------------------ helpers

fn start_scan(root: PathBuf) -> Task<Message> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<(Arc<Scan>, Arc<Report>), String> {
            let rules = RuleSet::embedded().map_err(|e| e.to_string())?;
            let scan = scan(&root, ScanOptions::default()).map_err(|e| e.to_string())?;
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

fn card<'a>(content: Element<'a, Message>) -> container::Container<'a, Message> {
    container(content).padding(14).style(|theme: &Theme| {
        let p = theme.extended_palette();
        container::Style {
            background: Some(Background::Color(p.background.weak.color)),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: p.background.strong.color,
            },
            ..Default::default()
        }
    })
}

fn swatch<'a>(color: Color, size: u16) -> Element<'a, Message> {
    container(Space::new(size, size))
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: (size as f32 / 3.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn pill<'a>(label: String, color: Color) -> Element<'a, Message> {
    container(text(label).size(11).color(treemap::contrast_text(color)))
        .padding([2, 8])
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
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

fn hex(v: u32) -> Color {
    Color::from_rgb8(
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
    )
}

fn level_color(level: Level, dark: bool) -> Color {
    match (level, dark) {
        (Level::Safe, false) => hex(0x008300),
        (Level::Safe, true) => hex(0x2fa32f),
        (Level::Review, false) => hex(0xeb6834),
        (Level::Review, true) => hex(0xd95926),
        (Level::Never, _) => hex(0x7a869a),
    }
}

fn neutral(dark: bool) -> Color {
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
        return neutral(dark);
    };
    if size > 0 && *total * 3 >= size {
        category_color(cat, dark)
    } else {
        treemap::lighten(category_color(cat, dark), if dark { -0.18 } else { 0.22 })
    }
}
