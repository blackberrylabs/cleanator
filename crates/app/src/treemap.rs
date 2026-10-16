//! Squarified treemap (Bruls, Huizing & van Wijk, 2000) drawn on an iced canvas.
//! Layout is recomputed from the current bounds on every draw and every click: at most a
//! few dozen tiles, so it costs nothing and tiles always fill the available space.

use std::path::PathBuf;

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{alignment, Color, Pixels, Point, Rectangle, Renderer, Size, Theme};

use cleanator_core::size::human;

#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    pub size: u64,
    pub color: Color,
    /// `None` for aggregated blocks (files of this folder, "others") that cannot be opened.
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Tile<'a> {
    pub item: &'a Item,
    pub rect: Rectangle,
}

/// Lay `items` (any order) out in `bounds`.
pub fn layout<'a>(items: &'a [Item], bounds: Rectangle) -> Vec<Tile<'a>> {
    let total: f64 = items.iter().map(|i| i.size as f64).sum();
    let mut sorted: Vec<&Item> = items.iter().filter(|i| i.size > 0).collect();
    sorted.sort_by_key(|i| std::cmp::Reverse(i.size));
    let mut tiles = Vec::with_capacity(sorted.len());
    if total <= 0.0 || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return tiles;
    }
    let area_scale = (bounds.width as f64 * bounds.height as f64) / total;
    let mut rect = Rect {
        x: bounds.x as f64,
        y: bounds.y as f64,
        w: bounds.width as f64,
        h: bounds.height as f64,
    };
    let mut row: Vec<(&Item, f64)> = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let item = sorted[i];
        let area = item.size as f64 * area_scale;
        let side = rect.w.min(rect.h);
        if row.is_empty() || worst(&row, area, side) <= worst(&row, 0.0, side) {
            row.push((item, area));
            i += 1;
        } else {
            place_row(&row, &mut rect, &mut tiles);
            row.clear();
        }
    }
    if !row.is_empty() {
        place_row(&row, &mut rect, &mut tiles);
    }
    tiles
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Worst aspect ratio of `row` laid along a side of length `side`, with `extra` added.
fn worst(row: &[(&Item, f64)], extra: f64, side: f64) -> f64 {
    let areas: Vec<f64> = row
        .iter()
        .map(|(_, a)| *a)
        .chain((extra > 0.0).then_some(extra))
        .collect();
    let sum: f64 = areas.iter().sum();
    if sum <= 0.0 || side <= 0.0 {
        return f64::MAX;
    }
    let max = areas.iter().cloned().fold(0.0, f64::max);
    let min = areas.iter().cloned().fold(f64::MAX, f64::min);
    let s2 = sum * sum;
    let side2 = side * side;
    (side2 * max / s2).max(s2 / (side2 * min))
}

fn place_row<'a>(row: &[(&'a Item, f64)], rect: &mut Rect, tiles: &mut Vec<Tile<'a>>) {
    let sum: f64 = row.iter().map(|(_, a)| *a).sum();
    if rect.w >= rect.h {
        // vertical strip on the left
        let strip_w = sum / rect.h;
        let mut y = rect.y;
        for (item, area) in row {
            let h = area / strip_w;
            tiles.push(Tile {
                item,
                rect: Rectangle::new(
                    Point::new(rect.x as f32, y as f32),
                    Size::new(strip_w as f32, h as f32),
                ),
            });
            y += h;
        }
        rect.x += strip_w;
        rect.w -= strip_w;
    } else {
        // horizontal strip on top
        let strip_h = sum / rect.w;
        let mut x = rect.x;
        for (item, area) in row {
            let w = area / strip_h;
            tiles.push(Tile {
                item,
                rect: Rectangle::new(
                    Point::new(x as f32, rect.y as f32),
                    Size::new(w as f32, strip_h as f32),
                ),
            });
            x += w;
        }
        rect.y += strip_h;
        rect.h -= strip_h;
    }
}

/// The canvas program: holds the items of the folder being shown.
pub struct Treemap {
    pub items: Vec<Item>,
    pub dark: bool,
}

const GAP: f32 = 3.0;

impl<Message: Clone> canvas::Program<Message> for Treemap
where
    Message: From<TreemapEvent>,
{
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event {
            if let Some(pos) = cursor.position_in(bounds) {
                let local = Rectangle::new(Point::ORIGIN, bounds.size());
                for tile in layout(&self.items, local) {
                    if tile.rect.contains(pos) {
                        if let Some(p) = &tile.item.path {
                            return (
                                canvas::event::Status::Captured,
                                Some(TreemapEvent::Open(p.clone()).into()),
                            );
                        }
                    }
                }
            }
        }
        (canvas::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let palette = theme.extended_palette();
        let hover = cursor.position_in(bounds);
        for tile in layout(&self.items, local) {
            let r = tile.rect;
            if r.width < 1.0 || r.height < 1.0 {
                continue;
            }
            let inner = Rectangle::new(
                Point::new(r.x + GAP / 2.0, r.y + GAP / 2.0),
                Size::new((r.width - GAP).max(1.0), (r.height - GAP).max(1.0)),
            );
            let hovered = hover.is_some_and(|p| r.contains(p)) && tile.item.path.is_some();
            let mut color = tile.item.color;
            if hovered {
                color = lighten(color, if self.dark { 0.12 } else { -0.08 });
            }
            let path = Path::rounded_rectangle(inner.position(), inner.size(), 4.0.into());
            frame.fill(&path, color);
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(with_alpha(palette.background.base.color, 0.9))
                    .with_width(1.0),
            );
            if inner.width > 54.0 && inner.height > 30.0 {
                let text_color = contrast_text(color);
                let max_chars = ((inner.width - 10.0) / 7.0).max(3.0) as usize;
                let label = truncate(&tile.item.label, max_chars);
                frame.fill_text(Text {
                    content: label,
                    position: Point::new(inner.x + 6.0, inner.y + 5.0),
                    color: text_color,
                    size: Pixels(12.0),
                    font: iced::Font::default(),
                    horizontal_alignment: alignment::Horizontal::Left,
                    vertical_alignment: alignment::Vertical::Top,
                    line_height: Default::default(),
                    shaping: iced::widget::text::Shaping::Advanced,
                });
                if inner.height > 46.0 {
                    frame.fill_text(Text {
                        content: human(tile.item.size),
                        position: Point::new(inner.x + 6.0, inner.y + inner.height - 6.0),
                        color: with_alpha(text_color, 0.85),
                        size: Pixels(if inner.width > 140.0 && inner.height > 90.0 {
                            18.0
                        } else {
                            12.0
                        }),
                        font: iced::Font::MONOSPACE,
                        horizontal_alignment: alignment::Horizontal::Left,
                        vertical_alignment: alignment::Vertical::Bottom,
                        line_height: Default::default(),
                        shaping: iced::widget::text::Shaping::Basic,
                    });
                }
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if let Some(pos) = cursor.position_in(bounds) {
            let local = Rectangle::new(Point::ORIGIN, bounds.size());
            if layout(&self.items, local)
                .iter()
                .any(|t| t.rect.contains(pos) && t.item.path.is_some())
            {
                return mouse::Interaction::Pointer;
            }
        }
        mouse::Interaction::default()
    }
}

#[derive(Debug, Clone)]
pub enum TreemapEvent {
    Open(PathBuf),
}

fn truncate(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1).max(1);
    let head: String = s.chars().take(keep).collect();
    format!("{head}…")
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

pub fn lighten(c: Color, amount: f32) -> Color {
    Color::from_rgb(
        (c.r + amount).clamp(0.0, 1.0),
        (c.g + amount).clamp(0.0, 1.0),
        (c.b + amount).clamp(0.0, 1.0),
    )
}

/// Black or white text depending on the tile's luminance.
pub fn contrast_text(c: Color) -> Color {
    let lum = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    if lum > 0.6 {
        Color::from_rgb(0.12, 0.12, 0.14)
    } else {
        Color::from_rgb(0.98, 0.98, 0.98)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_fill_the_bounds_and_keep_areas() {
        let items: Vec<Item> = [50u64, 25, 15, 10]
            .iter()
            .enumerate()
            .map(|(i, s)| Item {
                label: format!("i{i}"),
                size: *s,
                color: Color::BLACK,
                path: None,
            })
            .collect();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(200.0, 100.0));
        let tiles = layout(&items, bounds);
        assert_eq!(tiles.len(), 4);
        let area: f32 = tiles.iter().map(|t| t.rect.width * t.rect.height).sum();
        assert!((area - 20_000.0).abs() < 1.0, "{area}");
        let big = tiles.iter().find(|t| t.item.label == "i0").unwrap();
        assert!((big.rect.width * big.rect.height - 10_000.0).abs() < 1.0);
        for t in &tiles {
            assert!(t.rect.x >= -0.01 && t.rect.y >= -0.01);
            assert!(t.rect.x + t.rect.width <= 200.01 && t.rect.y + t.rect.height <= 100.01);
        }
    }

    #[test]
    fn truncates_labels() {
        assert_eq!(truncate("node_modules", 20), "node_modules");
        assert_eq!(truncate("a-very-long-directory-name", 8), "a-very-…");
    }
}
