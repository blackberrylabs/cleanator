//! Cleanator desktop app (iced). One window, three gestures: look, tick, copy the script.
//! Runs the core in a background thread; never deletes anything (R1).

mod i18n;
mod treemap;

use std::collections::HashSet;
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
use cleanator_core::size::human;

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
    status: String,
}

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

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Scanned(Ok((scan, report))) => {
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
                self.current = scan.summary.root.clone();
                self.scan = Some(scan);
                self.report = Some(report);
                self.phase = Phase::Ready;
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
            Message::SelectSafe => {
                if let Some(r) = &self.report {
                    for rec in &r.recommendations {
                        if rec.effective_level == Level::Safe
                            && rec.in_use.is_none()
                            && !rec.recent
                            && rec.command.is_some()
                        {
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
                row![self.view_explorer(scan, report), self.view_panel(report),]
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
                Some(level_color(Level::Safe))
            ),
            self.kpi(
                &self.t("kpi.review"),
                review,
                &items(n_review),
                Some(level_color(Level::Review))
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
            head =
                head.push(
                    container(Space::new(8, 8)).style(move |_: &Theme| container::Style {
                        background: Some(Background::Color(c)),
                        border: Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                );
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
                    container(Space::new(10, 10)).style(move |_: &Theme| container::Style {
                        background: Some(Background::Color(category_color(cat))),
                        border: Border {
                            radius: 3.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                    text(label).size(11).style(text::secondary),
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
                    pill(label, level_color(level)),
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
            for r in recs.iter().take(400) {
                list = list.push(self.view_rec(r));
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

    fn view_rec<'a>(&'a self, r: &'a Recommendation) -> Element<'a, Message> {
        let home = self.root.to_string_lossy().to_string();
        let display = shorten(&r.display.replacen(&home, "~", 1), 56);
        let label = format!("{:>7}  {}", human(r.reclaimable), display);
        let id = r.id.clone();
        let mut cb = checkbox(label, self.selected.contains(&r.id))
            .size(16)
            .text_size(13);
        let usable = r.in_use.is_none() && r.command.is_some();
        if usable {
            cb = cb.on_toggle(move |on| Message::Toggle(id.clone(), on));
        }
        let mut meta = format!("{} · {}", r.rule_id, shorten(&r.why, 70));
        if let Some(u) = &r.in_use {
            meta = format!("{} · {}: {}", r.rule_id, self.t("panel.in_use"), u);
        }
        column![cb, text(meta).size(10).style(text::secondary)]
            .spacing(1)
            .padding(iced::Padding {
                top: 2.0,
                right: 0.0,
                bottom: 6.0,
                left: 0.0,
            })
            .into()
    }
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

fn category_color(cat: &str) -> Color {
    match cat {
        "model" | "dataset" | "training-output" => Color::from_rgb8(0x8E, 0x5B, 0xD6),
        "python-env" => Color::from_rgb8(0x2A, 0x9D, 0x8F),
        "package-cache" | "tooling" => Color::from_rgb8(0x3A, 0x7B, 0xD5),
        "build" => Color::from_rgb8(0x5C, 0x6B, 0xC0),
        "docker" => Color::from_rgb8(0x1F, 0xA5, 0xC9),
        "assistant-history" | "assistant-cache" | "assistant-config" => {
            Color::from_rgb8(0xD6, 0x5B, 0x8A)
        }
        "backup" | "large-file" => Color::from_rgb8(0xE0, 0x8A, 0x3C),
        "git-reclonable" => Color::from_rgb8(0x5B, 0xA8, 0x5B),
        "system" | "system-index" | "browser" | "snapshot" => Color::from_rgb8(0x7A, 0x86, 0x9A),
        _ => Color::from_rgb8(0x9A, 0x9A, 0xA0),
    }
}

fn level_color(level: Level) -> Color {
    match level {
        Level::Safe => Color::from_rgb8(0x2E, 0x9E, 0x5B),
        Level::Review => Color::from_rgb8(0xE0, 0x8A, 0x3C),
        Level::Never => Color::from_rgb8(0x7A, 0x86, 0x9A),
    }
}

fn neutral(dark: bool) -> Color {
    if dark {
        Color::from_rgb8(0x3A, 0x3D, 0x45)
    } else {
        Color::from_rgb8(0xD9, 0xDC, 0xE3)
    }
}

/// Colour of a folder tile: the dominant rule category of what lies at or below it,
/// when classified items make up at least a third of the folder; neutral otherwise.
fn color_for(path: &Path, size: u64, report: &Report, dark: bool) -> Color {
    let mut by_cat: std::collections::BTreeMap<&str, u64> = Default::default();
    for r in &report.recommendations {
        let Some(p) = &r.path else { continue };
        let p = Path::new(p);
        if p == path || p.starts_with(path) {
            *by_cat.entry(r.category.as_str()).or_default() += r.size;
        } else if path.starts_with(p) {
            // inside a classified folder: inherit
            return category_color(&r.category);
        }
    }
    let Some((cat, total)) = by_cat.iter().max_by_key(|(_, s)| **s) else {
        return neutral(dark);
    };
    if size > 0 && *total * 3 >= size {
        category_color(cat)
    } else {
        treemap::lighten(category_color(cat), if dark { -0.18 } else { 0.22 })
    }
}
