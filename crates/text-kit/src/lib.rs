//! Text engine trait for the fresco UI engine (ADR-0002).
//!
//! The estate owns the contract; first implementation is Parley behind
//! the `parley` feature. The insurance pattern is the same as
//! ui-text-edit's `TextBuffer`: the trait is the portability contract, so
//! a DirectWrite adapter (item 881) can arrive without touching callers.
//!
//! Layers:
//!
//! - [`TextEngine`]: shaping (`shape`) and paragraph layout (`layout`).
//! - [`GlyphAtlas`]: shelf-packed coverage atlas with LRU eviction (887).
//! - [`FakeEngine`]: deterministic monospace engine for hermetic tests
//!   and golden images.
//!
//! Glyph coverage bitmaps are `A8`; rasterization itself belongs to
//! font-shape (L0 oracle) and reaches this crate through the
//! [`GlyphRasterizer`] callback.
#![forbid(unsafe_code)]

pub mod atlas;
pub mod fake;

#[cfg(feature = "parley")]
pub mod parley_backend;

pub use atlas::{GlyphAtlas, GlyphBitmap, GlyphKey, GlyphRasterizer};
pub use fake::FakeEngine;

use std::ops::Range;

/// Font handle owned by the caller; opaque to this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u32);

/// Rasterizer glyph identifier (font-specific).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlyphId(pub u32);

/// Shaping request.
#[derive(Clone, Copy, Debug)]
pub struct ShapeQuery<'a> {
    /// Text to shape (one direction, one script run; the engine may
    /// internally segment).
    pub text: &'a str,
    /// Font to use.
    pub font: FontId,
    /// Pixel size (em size in device pixels).
    pub px_size: f64,
    /// Extra letter spacing in pixels.
    pub letter_spacing: f64,
}

/// One positioned glyph from shaping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    /// Glyph in the font.
    pub id: GlyphId,
    /// Byte index of the cluster start in the source text.
    pub cluster: u32,
    /// Advance along the reading axis, pixels.
    pub x_advance: f64,
    /// Offset from the pen position, pixels.
    pub x_offset: f64,
    /// Offset from the baseline, pixels (positive down).
    pub y_offset: f64,
}

/// Shaping result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeResult {
    /// Positioned glyphs in visual order.
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance width, pixels.
    pub width: f64,
}

/// Layout request: shaping plus line breaking.
#[derive(Clone, Copy, Debug)]
pub struct LayoutQuery<'a> {
    /// Paragraph text.
    pub text: &'a str,
    /// Font to use.
    pub font: FontId,
    /// Pixel size.
    pub px_size: f64,
    /// Wrap width; `None` means no wrapping (single line).
    pub max_width: Option<f64>,
    /// Line height in pixels (baseline to baseline).
    pub line_height: f64,
}

/// One laid-out line: a glyph range plus metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct LineBox {
    /// Range into [`LayoutResult::glyphs`].
    pub glyphs: Range<usize>,
    /// Line advance width, pixels.
    pub width: f64,
    /// Baseline y of the line, pixels from the top.
    pub baseline_y: f64,
}

/// Layout result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutResult {
    /// All glyphs, grouped by line in order.
    pub glyphs: Vec<ShapedGlyph>,
    /// Line boxes, top to bottom.
    pub lines: Vec<LineBox>,
    /// Total layout height (lines * line_height), pixels.
    pub height: f64,
}

/// The estate-owned text contract (ADR-0002).
///
/// Implementations: [`FakeEngine`] (deterministic, hermetic), the Parley
/// backend (feature `parley`), and future platform engines behind the
/// same methods.
pub trait TextEngine {
    /// Shape a single run.
    fn shape(&mut self, query: &ShapeQuery<'_>) -> ShapeResult;

    /// Shape and break into lines.
    fn layout(&mut self, query: &LayoutQuery<'_>) -> LayoutResult;
}
