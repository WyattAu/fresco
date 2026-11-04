//! Renderer contract and the stock paint-kit implementation.
//!
//! Widgets compute; renderers paint. The stock implementation uses
//! paint-kit directly. Label text pixels are a placeholder box in the
//! stock renderer (documented limitation): the consumer wires the
//! text-kit glyph path through this same trait at integration
//! (kestrel overrides `label_text` with real font glyphs).

use crate::{WidgetKind, WidgetRef, WidgetTree};
use layout_kit::engine::Rect2D;
use paint_kit::{Paint, Painter, Solid, Surface};
use text_kit::microfont;

/// Paints widgets onto a target. Object-safe; consumers may implement
/// richer text handling.
pub trait WidgetRenderer {
    /// Static text: draws readable micro-font glyphs inside `rect`.
    /// The stock implementation blits 3×5 micro-font bitmaps scaled to
    /// `px_size`; consumers override for real font rendering.
    fn label_text(&mut self, rect: Rect2D, text: &str, px_size: f64, color: [u8; 4]) {
        // Fallback: solid box (the pre-glyph placeholder).
        let _ = (text, px_size);
        self.label_box(rect, color);
    }

    /// Static text placeholder retained for API compat; prefer
    /// `label_text`.
    fn label_box(&mut self, rect: Rect2D, color: [u8; 4]);

    /// Button chrome: base fill, pressed overlay, focus ring.
    fn button_frame(&mut self, rect: Rect2D, colors: &WidgetColors, focused: bool, pressed: bool);

    /// Checkbox chrome: box plus check mark when checked.
    fn checkbox(&mut self, rect: Rect2D, colors: &WidgetColors, checked: bool, focused: bool);

    /// Container panel fill.
    fn panel(&mut self, rect: Rect2D, color: [u8; 4]);

    /// Text entry chrome: entry box, caret when focused.
    fn text_field(&mut self, rect: Rect2D, colors: &WidgetColors, focused: bool);

    /// Push a clip rect (scissor) for a clipped container.
    fn push_clip(&mut self, _rect: Rect2D) {}

    /// Pop the last clip.
    fn pop_clip(&mut self) {}

    /// Tab strip chrome: strip fill, per-tab boxes, active tab marker.
    fn tab_strip(
        &mut self,
        rect: Rect2D,
        tabs: &[String],
        active: usize,
        colors: &WidgetColors,
        focused: bool,
    ) {
        let _ = (rect, tabs, active, colors, focused);
    }

    /// Dropdown: header with the selected option and a caret marker;
    /// when open, one row per option below with the selected row
    /// highlighted.
    fn select(
        &mut self,
        rect: Rect2D,
        options: &[String],
        selected: usize,
        open: bool,
        colors: &WidgetColors,
        focused: bool,
    ) {
        let _ = (rect, options, selected, open, colors, focused);
    }

    /// Tree rows: one row per visible item, selection highlight,
    /// expand markers, focus border.
    fn tree_list(
        &mut self,
        rect: Rect2D,
        items: &[crate::TreeItem],
        selected: usize,
        colors: &WidgetColors,
        focused: bool,
    ) {
        let _ = (rect, items, selected, colors, focused);
    }
}

/// Default palette (neutral dark); callers override with theme-kit
/// resolved roles at integration time.
#[derive(Clone, Copy, Debug)]
pub struct WidgetColors {
    /// Container surface.
    pub surface: [u8; 4],
    /// Text placeholder color.
    pub text: [u8; 4],
    /// Button base.
    pub button: [u8; 4],
    /// Focus ring.
    pub focus_ring: [u8; 4],
    /// Pressed overlay.
    pub pressed_overlay: [u8; 4],
    /// Checkbox box.
    pub checkbox_box: [u8; 4],
    /// Check mark.
    pub check: [u8; 4],
}

impl Default for WidgetColors {
    fn default() -> Self {
        WidgetColors {
            surface: [24, 26, 32, 255],
            text: [200, 204, 214, 255],
            button: [56, 60, 72, 255],
            focus_ring: [120, 170, 255, 255],
            pressed_overlay: [0, 0, 0, 90],
            checkbox_box: [40, 44, 54, 255],
            check: [140, 220, 160, 255],
        }
    }
}

/// Paint widgets through paint-kit onto a surface.
pub struct DrawListPainter<'p> {
    /// Target surface.
    pub surface: &'p mut Surface,
    /// Painter (owns raster scratch).
    pub painter: &'p mut Painter,
    /// Palette.
    pub colors: WidgetColors,
}

impl DrawListPainter<'_> {
    fn fill(&mut self, rect: Rect2D, color: [u8; 4]) {
        self.painter.fill_rect(
            self.surface,
            into_rect(rect),
            &Paint::from(Solid::premul(color)),
        );
    }

    fn border(&mut self, rect: Rect2D, thickness: f64, color: [u8; 4]) {
        let t = thickness.max(1.0);
        let edges = [
            Rect2D {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: t,
            },
            Rect2D {
                x: rect.x,
                y: rect.y + rect.height - t,
                width: rect.width,
                height: t,
            },
            Rect2D {
                x: rect.x,
                y: rect.y,
                width: t,
                height: rect.height,
            },
            Rect2D {
                x: rect.x + rect.width - t,
                y: rect.y,
                width: t,
                height: rect.height,
            },
        ];
        for edge in edges {
            self.fill(edge, color);
        }
    }
}

fn into_rect(rect: Rect2D) -> paint_kit::Rect {
    paint_kit::Rect::from_size(
        paint_kit::Vec2::new(rect.x, rect.y),
        paint_kit::Vec2::new(rect.width, rect.height),
    )
}

impl WidgetRenderer for DrawListPainter<'_> {
    fn label_text(&mut self, rect: Rect2D, text: &str, px_size: f64, color: [u8; 4]) {
        // Micro-font glyphs: 3×5 source scaled to px_size tall, blitted
        // with per-pixel coverage blending. Fully deterministic.
        let scale = ((px_size / 5.0).round() as u32).max(1);
        let advance = px_size * 0.6;
        let baseline_y = rect.y + (rect.height * 0.5) - (f64::from(scale) * 5.0 * 0.5);
        let mut pen_x = rect.x + 2.0;
        for ch in text.chars() {
            if let Some(bitmap) = microfont::micro_bitmap(ch, scale) {
                for gy in 0..bitmap.height {
                    for gx in 0..bitmap.width {
                        let idx = gy as usize * bitmap.width as usize + gx as usize;
                        let cov = match bitmap.coverage.get(idx) {
                            Some(c) => *c,
                            None => continue,
                        };
                        if cov == 0 {
                            continue;
                        }
                        let alpha = f64::from(cov) / 255.0 * (f64::from(color[3]) / 255.0);
                        let px = [
                            ((f64::from(color[0]) / 255.0).sqrt() * 255.0).round() as u8,
                            ((f64::from(color[1]) / 255.0).sqrt() * 255.0).round() as u8,
                            ((f64::from(color[2]) / 255.0).sqrt() * 255.0).round() as u8,
                            (alpha * 255.0).round() as u8,
                        ];
                        let dx = pen_x + f64::from(gx);
                        let dy = baseline_y + f64::from(gy);
                        self.surface.blend_pixel(
                            usize::try_from(dx as u64).unwrap_or(0),
                            usize::try_from(dy as u64).unwrap_or(0),
                            px,
                        );
                    }
                }
            }
            pen_x += advance;
        }
    }

    fn tab_strip(
        &mut self,
        rect: Rect2D,
        tabs: &[String],
        active: usize,
        colors: &WidgetColors,
        focused: bool,
    ) {
        // Strip background, then one box per tab; the active tab gets
        // the button fill plus an underline marker in the text color so
        // the active state survives CVD simulation and greyscale.
        self.fill(rect, colors.checkbox_box);
        let count = tabs.len().max(1);
        let tab_w = rect.width / f64::from(u16::try_from(count).unwrap_or(u16::MAX));
        for (i, name) in tabs.iter().enumerate() {
            let i_f = f64::from(u16::try_from(i).unwrap_or(u16::MAX));
            let tr = Rect2D {
                x: rect.x + i_f * tab_w,
                y: rect.y,
                width: tab_w,
                height: rect.height,
            };
            if i == active {
                self.fill(tr, colors.button);
                self.fill(
                    Rect2D {
                        x: tr.x,
                        y: tr.y + tr.height - 2.0,
                        width: tr.width,
                        height: 2.0,
                    },
                    colors.text,
                );
            }
            let label_rect = Rect2D {
                x: tr.x + 4.0,
                y: tr.y,
                width: (tr.width - 8.0).max(0.0),
                height: tr.height,
            };
            self.label_text(label_rect, name, 13.0, colors.text);
        }
        if focused {
            self.border(rect, 1.0, colors.focus_ring);
        }
    }

    fn select(
        &mut self,
        rect: Rect2D,
        options: &[String],
        selected: usize,
        open: bool,
        colors: &WidgetColors,
        focused: bool,
    ) {
        // Header: button fill, selected option + caret, focus border.
        let header = Rect2D {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: crate::SELECT_HEADER_H.min(rect.height),
        };
        self.fill(header, colors.button);
        let label = options
            .get(selected)
            .map(|s| format!(" {s} v"))
            .unwrap_or_else(|| " v".to_string());
        self.label_text(header, &label, 14.0, colors.text);
        if open {
            for (i, opt) in options.iter().enumerate() {
                let y = header.y
                    + crate::SELECT_HEADER_H
                    + f64::from(u16::try_from(i).unwrap_or(u16::MAX)) * crate::TREE_ROW_H;
                let row = Rect2D {
                    x: rect.x,
                    y,
                    width: rect.width,
                    height: crate::TREE_ROW_H,
                };
                if i == selected {
                    self.fill(row, colors.checkbox_box);
                }
                let text = format!(" {opt}");
                self.label_text(row, &text, 13.0, colors.text);
            }
            self.border(rect, 1.0, colors.focus_ring);
        } else if focused {
            self.border(header, 1.0, colors.focus_ring);
        }
    }

    fn tree_list(
        &mut self,
        rect: Rect2D,
        items: &[crate::TreeItem],
        selected: usize,
        colors: &WidgetColors,
        focused: bool,
    ) {
        for (pos, idx) in crate::tree_visible_rows(items).into_iter().enumerate() {
            let Some(item) = items.get(idx) else {
                continue;
            };
            let y = rect.y + f64::from(u32::try_from(pos).unwrap_or(u32::MAX)) * crate::TREE_ROW_H;
            let row = Rect2D {
                x: rect.x,
                y,
                width: rect.width,
                height: crate::TREE_ROW_H,
            };
            if pos == selected {
                self.fill(row, colors.button);
            }
            let marker = if item.has_children {
                if item.expanded { "- " } else { "+ " }
            } else {
                "  "
            };
            let indent =
                4.0 + f64::from(u32::try_from(item.depth).unwrap_or(u32::MAX)) * crate::TREE_INDENT;
            let label_rect = Rect2D {
                x: row.x + indent,
                y: row.y,
                width: (row.width - indent).max(0.0),
                height: row.height,
            };
            let text = format!("{marker}{}", item.label);
            self.label_text(label_rect, &text, 14.0, colors.text);
        }
        if focused {
            self.border(rect, 1.0, colors.focus_ring);
        }
    }

    fn label_box(&mut self, rect: Rect2D, color: [u8; 4]) {
        self.fill(rect, color);
    }

    fn button_frame(&mut self, rect: Rect2D, colors: &WidgetColors, focused: bool, pressed: bool) {
        self.fill(rect, colors.button);
        if pressed {
            self.fill(rect, colors.pressed_overlay);
        }
        if focused {
            self.border(rect, 2.0, colors.focus_ring);
        }
    }

    fn checkbox(&mut self, rect: Rect2D, colors: &WidgetColors, checked: bool, focused: bool) {
        let box_side = rect.height.min(rect.width).min(16.0);
        let box_rect = Rect2D {
            x: rect.x,
            y: rect.y + (rect.height - box_side) * 0.5,
            width: box_side,
            height: box_side,
        };
        self.fill(box_rect, colors.checkbox_box);
        if checked {
            let px = (box_side * 0.14).ceil();
            self.fill(
                Rect2D {
                    x: box_rect.x + box_side * 0.2,
                    y: box_rect.y + box_side * 0.55,
                    width: box_side * 0.35,
                    height: px,
                },
                colors.check,
            );
            self.fill(
                Rect2D {
                    x: box_rect.x + box_side * 0.5,
                    y: box_rect.y + box_side * 0.25,
                    width: px,
                    height: box_side * 0.4,
                },
                colors.check,
            );
        }
        if focused {
            self.border(box_rect, 1.0, colors.focus_ring);
        }
    }

    fn panel(&mut self, rect: Rect2D, color: [u8; 4]) {
        self.fill(rect, color);
    }

    fn text_field(&mut self, rect: Rect2D, colors: &WidgetColors, focused: bool) {
        // Entry box (inset 2 px) plus caret line at the leading edge
        // when focused.
        let inset = 2.0;
        let box_rect = Rect2D {
            x: rect.x + inset,
            y: rect.y + inset,
            width: rect.width - inset * 2.0,
            height: rect.height - inset * 2.0,
        };
        self.fill(box_rect, colors.checkbox_box);
        if focused {
            let caret_h = (box_rect.height * 0.7).ceil();
            let caret_y = box_rect.y + (box_rect.height - caret_h) * 0.5;
            self.fill(
                Rect2D {
                    x: box_rect.x + 3.0,
                    y: caret_y,
                    width: 1.0,
                    height: caret_h,
                },
                colors.focus_ring,
            );
        }
    }
}

/// Paint a whole tree: root panel, then widgets back-to-front. A root
/// ScrollPane clips and translates its children.
pub fn paint_tree(
    tree: &WidgetTree,
    surface: &mut Surface,
    painter: &mut Painter,
    colors: &WidgetColors,
) {
    let mut renderer = DrawListPainter {
        surface,
        painter,
        colors: *colors,
    };
    let Some(root) = tree.root() else {
        return;
    };
    if let Some(rect) = tree.widget(root).map(|w| w.rect) {
        renderer.panel(rect, colors.surface);
    }
    let root_scroll = match tree.widget(root).map(|w| &w.kind) {
        Some(WidgetKind::ScrollPane { scroll_y }) => Some(*scroll_y),
        _ => None,
    };
    match root_scroll {
        Some(scroll_y) => {
            renderer.push_clip(tree.widget(root).map(|w| w.rect).unwrap_or_default());
            paint_children(tree, Some(root), (0.0, -scroll_y), &mut renderer, colors);
            renderer.pop_clip();
        }
        None => paint_children(tree, Some(root), (0.0, 0.0), &mut renderer, colors),
    }
}

fn paint_children(
    tree: &WidgetTree,
    parent: Option<WidgetRef>,
    offset: (f64, f64),
    renderer: &mut dyn WidgetRenderer,
    colors: &WidgetColors,
) {
    let Some(parent) = parent else {
        return;
    };
    let mut queue: Vec<WidgetRef> = tree.children_of(parent);
    queue.reverse();
    for r in queue {
        paint_node(tree, r, offset, renderer, colors);
    }
}

fn paint_node(
    tree: &WidgetTree,
    r: WidgetRef,
    offset: (f64, f64),
    renderer: &mut dyn WidgetRenderer,
    colors: &WidgetColors,
) {
    let Some(w) = tree.widget(r) else {
        return;
    };
    let rect = Rect2D {
        x: w.rect.x + offset.0,
        y: w.rect.y + offset.1,
        width: w.rect.width,
        height: w.rect.height,
    };
    let is_scroll = matches!(&w.kind, WidgetKind::ScrollPane { .. });
    match &w.kind {
        WidgetKind::Label { text, px_size } => {
            renderer.label_text(rect, text, *px_size, colors.text);
        }
        WidgetKind::Button { .. } => {
            renderer.button_frame(rect, colors, w.focused, w.pressed);
        }
        WidgetKind::Checkbox { checked, .. } => {
            renderer.checkbox(rect, colors, *checked, w.focused);
        }
        WidgetKind::TextField { .. } => renderer.text_field(rect, colors, w.focused),
        WidgetKind::ScrollPane { scroll_y } => {
            renderer.push_clip(rect);
            paint_children(
                tree,
                Some(r),
                (offset.0, offset.1 - *scroll_y),
                renderer,
                colors,
            );
            renderer.pop_clip();
        }
        WidgetKind::TabList { tabs, active } => {
            renderer.tab_strip(rect, tabs, *active, colors, w.focused);
        }
        WidgetKind::TreeList { items, selected } => {
            renderer.tree_list(rect, items, *selected, colors, w.focused);
        }
        WidgetKind::Select {
            options,
            selected,
            open,
        } => {
            renderer.select(rect, options, *selected, *open, colors, w.focused);
        }
        WidgetKind::Row { .. } | WidgetKind::Column { .. } | WidgetKind::Spacer => {}
    }
    if !is_scroll {
        paint_children(tree, Some(r), offset, renderer, colors);
    }
}

use std::vec::Vec;
