//! Widget rendering onto the sparse-strip backend: a
//! [`WidgetRenderer`] implementation over [`StripTarget`].
//!
//! Each method mirrors its `DrawListPainter` counterpart's geometry
//! exactly (same fills, same micro-font blitting as 1x1 rect fills,
//! same ring-inside-the-rect focus), so the widget-level parity
//! harness can rasterize one widget tree through both backends and
//! compare. Shadows use vello's native analytic gaussian; their edge
//! semantics differ from the CPU SDF by documented design, so the
//! widget-level parity test carries a measured corner-style budget
//! rather than the zero budget the crisp geometry holds.

use crate::{Rect2D, WidgetColors, WidgetRenderer};
use paint_kit::StripTarget;

/// Shrink (positive) or grow (negative) a rect on all sides.
fn inflate(rect: Rect2D, d: f64) -> Rect2D {
    Rect2D {
        x: rect.x + d,
        y: rect.y + d,
        width: (rect.width - 2.0 * d).max(0.0),
        height: (rect.height - 2.0 * d).max(0.0),
    }
}

/// Widget renderer over the sparse-strip pipeline.
pub struct StripListPainter<'t> {
    /// The encode target.
    pub target: &'t mut StripTarget,
    /// Palette.
    pub colors: WidgetColors,
}

impl<'t> StripListPainter<'t> {
    /// Over an existing target.
    #[must_use]
    pub fn new(target: &'t mut StripTarget, colors: WidgetColors) -> Self {
        StripListPainter { target, colors }
    }

    fn fill(&mut self, rect: Rect2D, color: [u8; 4]) {
        self.target.fill_rect(to_prect(rect), color);
    }

    fn fill_round(&mut self, rect: Rect2D, radius: f64, color: [u8; 4]) {
        self.target.fill_round_rect(to_prect(rect), radius, color);
    }

    fn fill_shadow(&mut self, rect: Rect2D, radius: f64) {
        // Fixed near-black, luminance-only: identical under every CVD
        // variant and every theme, so no palette field is needed.
        self.target
            .fill_shadow_round_rect(to_prect(rect), radius, 3.0, [0, 0, 0, 170]);
    }
}

fn to_prect(rect: Rect2D) -> paint_kit::Rect {
    paint_kit::Rect::from_size(
        paint_kit::Vec2::new(rect.x, rect.y),
        paint_kit::Vec2::new(rect.width, rect.height),
    )
}

impl WidgetRenderer for StripListPainter<'_> {
    fn label_text(&mut self, rect: Rect2D, text: &str, px_size: f64, color: [u8; 4]) {
        // Same geometry as the CPU blitter, but each set glyph pixel
        // becomes a 1x1 fill (binary coverage, full opacity).
        let scale = ((px_size / 5.0).round() as u32).max(1);
        let advance = px_size * 0.6;
        let baseline_y = rect.y + (rect.height * 0.5) - (f64::from(scale) * 5.0 * 0.5);
        let mut pen_x = rect.x + 2.0;
        for ch in text.chars() {
            if let Some(bitmap) = text_kit::microfont::micro_bitmap(ch, scale) {
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
                        self.target.fill_rect(
                            paint_kit::Rect::from_size(
                                paint_kit::Vec2::new(
                                    pen_x + f64::from(gx),
                                    baseline_y + f64::from(gy),
                                ),
                                paint_kit::Vec2::new(1.0, 1.0),
                            ),
                            color,
                        );
                    }
                }
            }
            pen_x += advance;
        }
    }

    fn label_box(&mut self, rect: Rect2D, color: [u8; 4]) {
        self.fill(rect, color);
    }

    fn button_frame(&mut self, rect: Rect2D, colors: &WidgetColors, focused: bool, pressed: bool) {
        // Shadow drawn first, matching the CPU chrome's SHADOW_MARGIN
        // extent.
        self.fill_shadow(rect, 6.0);
        if focused {
            self.fill_round(rect, 6.0, colors.focus_ring);
            self.fill_round(inflate(rect, -1.5), 4.5, colors.button);
        } else {
            self.fill_round(rect, 6.0, colors.button);
        }
        if pressed {
            self.fill_round(rect, 6.0, colors.pressed_overlay);
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
            for edge in rect_edges(box_rect, 1.0) {
                self.fill(edge, colors.focus_ring);
            }
        }
    }

    fn panel(&mut self, rect: Rect2D, color: [u8; 4]) {
        self.fill(rect, color);
    }

    fn text_field(&mut self, rect: Rect2D, colors: &WidgetColors, focused: bool) {
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

    fn push_clip(&mut self, rect: Rect2D) {
        self.target.push_clip(to_prect(rect));
    }

    fn pop_clip(&mut self) {
        self.target.pop_clip();
    }

    fn tab_strip(
        &mut self,
        rect: Rect2D,
        tabs: &[String],
        active: usize,
        colors: &WidgetColors,
        focused: bool,
    ) {
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
            for edge in rect_edges(rect, 1.0) {
                self.fill(edge, colors.focus_ring);
            }
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
        let header = Rect2D {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: crate::SELECT_HEADER_H.min(rect.height),
        };
        self.fill_shadow(header, 6.0);
        if focused {
            self.fill_round(header, 6.0, colors.focus_ring);
            self.fill_round(inflate(header, -1.5), 4.5, colors.button);
        } else {
            self.fill_round(header, 6.0, colors.button);
        }
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
            for edge in rect_edges(rect, 1.0) {
                self.fill(edge, colors.focus_ring);
            }
        } else if focused {
            for edge in rect_edges(header, 1.0) {
                self.fill(edge, colors.focus_ring);
            }
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
            for edge in rect_edges(rect, 1.0) {
                self.fill(edge, colors.focus_ring);
            }
        }
    }
}

/// The four 1-px border rects of `rect` at `thickness`.
fn rect_edges(rect: Rect2D, thickness: f64) -> [Rect2D; 4] {
    let t = thickness.max(1.0);
    [
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
    ]
}
