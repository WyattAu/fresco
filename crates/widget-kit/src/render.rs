//! Renderer contract and the stock paint-kit implementation.
//!
//! Widgets compute; renderers paint. The stock implementation uses
//! paint-kit directly. Label text pixels are a placeholder box in the
//! stock renderer (documented limitation): the consumer wires the
//! text-kit glyph path through this same trait at integration
//! (kestrel implements `label_box` with real glyphs).

use crate::{WidgetKind, WidgetRef, WidgetTree};
use layout_kit::engine::Rect2D;
use paint_kit::{Paint, Painter, Solid, Surface};

/// Paints widgets onto a target. Object-safe; consumers may implement
/// richer text handling.
pub trait WidgetRenderer {
    /// Static text placeholder (full rect fill at the text color).
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
        WidgetKind::Label { .. } => renderer.label_box(rect, colors.text),
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
        WidgetKind::Row { .. } | WidgetKind::Column { .. } | WidgetKind::Spacer => {}
    }
    if !is_scroll {
        paint_children(tree, Some(r), offset, renderer, colors);
    }
}

use std::vec::Vec;
