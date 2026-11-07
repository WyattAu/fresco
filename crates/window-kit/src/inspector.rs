//! Fresco inspector: tracing, frame capture, recording, and analysis
//! for running windows.
//!
//! The [`Inspector`] attaches to any render loop: every frame it
//! computes what actually changed (pixel diff vs the previous frame),
//! records the frame as a BMP, captures the screen via the platform
//! tool when asked, and accumulates a trace. At the end it writes a
//! manifest (`trace.json`), a human-readable analysis
//! (`analysis.txt`), and optionally encodes the frame sequence to a
//! video with ffmpeg (when available).
//!
//! Review during operation: every artifact lands on disk as soon as
//! its frame completes, so frames can be inspected while the run is
//! still going.
//!
//! Everything is std-only; screen capture shells out to whichever of
//! `scrot`, `grim`, or `import` (ImageMagick) is found, and video
//! encoding shells out to `ffmpeg`. Both are optional: without them
//! the inspector still records frames and writes the analysis.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Metadata + analysis for one recorded frame.
#[derive(Clone, Debug, Default)]
pub struct FrameMeta {
    /// Frame ordinal (0-based within the run).
    pub index: u64,
    /// Paint + present cost of this frame in microseconds.
    pub cost_us: u64,
    /// Microseconds since the previous recorded frame.
    pub interval_us: u64,
    /// Pixels whose bytes changed vs the previous frame.
    pub changed_pixels: u64,
    /// Fraction of pixels changed, in [0, 1].
    pub changed_ratio: f64,
    /// Damage regions painted this frame.
    pub regions: usize,
    /// FNV-1a of the frame bytes.
    pub hash: u64,
    /// Frame file name (when recorded).
    pub file: Option<String>,
}

/// FNV-1a over a byte slice.
#[must_use]
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Attachable recorder + analyzer for a running window.
pub struct Inspector {
    dir: PathBuf,
    record_every: usize,
    record_on: bool,
    screen_every: usize,
    screen_tool: Option<&'static str>,
    frames: Vec<FrameMeta>,
    prev: Vec<u8>,
    last_record: Option<Instant>,
    index: u64,
}

impl Inspector {
    /// Build from the environment:
    ///
    /// - `GATE_A_RECORD` (`1`/`true`/`on`): enable frame recording;
    /// - `GATE_A_RECORD_EVERY` (default 1): record every Nth frame;
    /// - `GATE_A_SCREEN_EVERY` (default 0 = off): capture the screen
    ///   every Nth frame via the platform tool;
    /// - `GATE_A_DIR` (default `/tmp/opencode/fresco-trace`): output
    ///   directory (created on demand).
    #[must_use]
    pub fn from_env() -> Self {
        let flag = |name: &str| {
            std::env::var(name)
                .map(|v| {
                    let v = v.to_ascii_lowercase();
                    v == "1" || v == "true" || v == "on"
                })
                .unwrap_or(false)
        };
        let number = |name: &str, default: usize| -> usize {
            std::env::var(name)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        let dir = std::env::var("GATE_A_DIR")
            .unwrap_or_else(|_| "/tmp/opencode/fresco-trace".to_string());
        Self::with_dir(
            PathBuf::from(dir),
            flag("GATE_A_RECORD"),
            number("GATE_A_RECORD_EVERY", 1),
            number("GATE_A_SCREEN_EVERY", 0),
        )
    }

    /// Explicit constructor: `dir` is created on demand. `record_on`
    /// gates frame recording; `record_every` selects the cadence;
    /// `screen_every > 0` enables platform screen captures.
    #[must_use]
    pub fn with_dir(
        dir: PathBuf,
        record_on: bool,
        record_every: usize,
        screen_every: usize,
    ) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Inspector {
            dir,
            record_on,
            record_every,
            screen_every,
            screen_tool: Self::find_screen_tool(),
            frames: Vec::new(),
            prev: Vec::new(),
            last_record: None,
            index: 0,
        }
    }

    fn find_screen_tool() -> Option<&'static str> {
        ["scrot", "grim", "import"].into_iter().find(|tool| {
            Command::new(tool)
                .arg("--help")
                .output()
                .is_ok_and(|o| o.status.success())
        })
    }

    /// Frames recorded so far.
    #[must_use]
    pub fn frames(&self) -> &[FrameMeta] {
        &self.frames
    }

    /// Record one frame: diff vs the previous frame, write a BMP when
    /// the record cadence fires, optionally capture the screen.
    ///
    /// `paint_us` is the caller's paint+present timing; `regions` is
    /// the damage region count (0 for full-frame repaints).
    pub fn record_frame(
        &mut self,
        rgba8: &[u8],
        width: usize,
        height: usize,
        paint_us: u64,
        regions: usize,
    ) {
        let now = Instant::now();
        let interval_us = self
            .last_record
            .map(|t| now.duration_since(t).as_micros() as u64)
            .unwrap_or(0);
        self.last_record = Some(now);

        let px_total = (rgba8.len() / 4).max(1);
        let changed_pixels = if self.prev.len() == rgba8.len() {
            // Per-pixel: any channel differing counts as changed.
            rgba8
                .as_chunks::<4>()
                .0
                .iter()
                .zip(self.prev.as_chunks::<4>().0.iter())
                .filter(|(a, b)| a != b)
                .count() as u64
        } else {
            px_total as u64
        };
        let changed_ratio = changed_pixels as f64 / px_total as f64;

        let record = self.record_on && self.index.is_multiple_of(self.record_every.max(1) as u64);
        if record {
            let name = format!("frame_{:06}.bmp", self.index);
            let path = self.dir.join(&name);
            if write_bmp24(&path, rgba8, width, height).is_ok() {
                self.frames.push(FrameMeta {
                    index: self.index,
                    cost_us: paint_us,
                    interval_us,
                    changed_pixels,
                    changed_ratio,
                    regions,
                    hash: fnv1a(rgba8),
                    file: Some(name),
                });
            }
        }
        if self.screen_every > 0
            && self.index.is_multiple_of(self.screen_every as u64)
            && let Some(tool) = self.screen_tool
        {
            let out = self.dir.join(format!("screen_{:06}.png", self.index));
            let mut cmd = Command::new(tool);
            match tool {
                "import" => {
                    cmd.arg("-window")
                        .arg("root")
                        .arg(out.display().to_string());
                }
                _ => {
                    // scrot and grim both take the output path last.
                    cmd.arg(out.display().to_string());
                }
            }
            let _ = cmd.output();
        }
        self.prev = rgba8.to_vec();
        self.index += 1;
    }

    /// Finish the run: write `trace.json` (frame manifest),
    /// `analysis.txt` (human-readable stats), and encode `video.mp4`
    /// with ffmpeg when `video` is set and frames were recorded.
    ///
    /// Returns the analysis path.
    pub fn finish(&self, video: bool) -> PathBuf {
        let mut json = String::from("{\"frames\": [\n");
        for (i, f) in self.frames.iter().enumerate() {
            let _ = writeln!(
                json,
                "  {{\"index\": {}, \"cost_us\": {}, \"interval_us\": {}, \
                 \"changed_pixels\": {}, \"changed_ratio\": {:.4}, \
                 \"regions\": {}, \"hash\": \"{:016x}\", \"file\": {:?}}}{}",
                f.index,
                f.cost_us,
                f.interval_us,
                f.changed_pixels,
                f.changed_ratio,
                f.regions,
                f.hash,
                f.file.as_deref().unwrap_or(""),
                if i + 1 < self.frames.len() { "," } else { "" }
            );
        }
        let _ = writeln!(json, "], \"count\": {} }}", self.frames.len());
        let _ = std::fs::write(self.dir.join("trace.json"), &json);

        let analysis = self.analyze();
        let _ = std::fs::write(self.dir.join("analysis.txt"), &analysis);

        if video {
            let recorded = self.frames.iter().filter(|f| f.file.is_some()).count();
            if recorded > 1 && self.frames.iter().any(|f| f.file.is_some()) {
                let span_us: u64 = self
                    .frames
                    .last()
                    .map(|f| f.interval_us.saturating_add(f.cost_us))
                    .unwrap_or(1);
                let fps = ((recorded as f64) / (span_us.max(1) as f64 / 1_000_000.0))
                    .round()
                    .clamp(1.0, 120.0) as u32;
                let out = self.dir.join("video.mp4");
                let pattern = self.dir.join("frame_%06d.bmp");
                let _ = Command::new("ffmpeg")
                    .args([
                        "-y",
                        "-framerate",
                        &fps.to_string(),
                        "-i",
                        &pattern.display().to_string(),
                        "-c:v",
                        "libx264",
                        "-pix_fmt",
                        "yuv420p",
                        &out.display().to_string(),
                    ])
                    .output();
            }
        }
        self.dir.join("analysis.txt")
    }

    /// Human-readable analysis: pacing percentiles, change hotspots,
    /// identical-frame count.
    #[must_use]
    pub fn analyze(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "fresco inspector: {} frames, dir {}",
            self.frames.len(),
            self.dir.display()
        );
        let mut costs: Vec<u64> = self.frames.iter().map(|f| f.cost_us).collect();
        costs.sort();
        let mean: u64 = costs.iter().sum::<u64>() / costs.len().max(1) as u64;
        let pick = |p: f64| -> u64 {
            let idx = ((p / 100.0) * (costs.len().saturating_sub(1)) as f64).round() as usize;
            costs.get(idx).copied().unwrap_or(0)
        };
        let _ = writeln!(
            out,
            "paint cost us: mean {mean} p50 {} p99 {} max {}",
            pick(50.0),
            pick(99.0),
            costs.last().copied().unwrap_or(0)
        );
        let mut ratios: Vec<f64> = self.frames.iter().map(|f| f.changed_ratio).collect();
        ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean_change: f64 = ratios.iter().sum::<f64>() / ratios.len().max(1) as f64;
        let _ = writeln!(
            out,
            "changed ratio: mean {:.4} p50 {:.4} max {:.4}",
            mean_change,
            ratios.get(ratios.len() / 2).copied().unwrap_or(0.0),
            ratios.last().copied().unwrap_or(0.0)
        );
        let identical = self.frames.iter().filter(|f| f.changed_pixels == 0).count();
        let _ = writeln!(out, "identical frames: {identical}");
        if let Some((i, f)) = self
            .frames
            .iter()
            .enumerate()
            .max_by_key(|(_, f)| f.changed_pixels)
        {
            let _ = writeln!(
                out,
                "change hotspot: frame {i} ({} px changed, ratio {:.4})",
                f.changed_pixels, f.changed_ratio
            );
        }
        out
    }
}

/// Write `rgba8` (premultiplied, row-major) as a 24-bit BMP: rows
/// bottom-up, BGR, rows padded to 4-byte boundaries.
pub fn write_bmp24(path: &Path, rgba8: &[u8], width: usize, height: usize) -> std::io::Result<()> {
    let row_pad = (4 - (width * 3) % 4) % 4;
    let row_size = width * 3 + row_pad;
    let pixel_size = row_size * height;
    let file_size = 54 + pixel_size;
    let mut out = Vec::with_capacity(file_size);
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&54_u32.to_le_bytes());
    // BITMAPINFOHEADER
    out.extend_from_slice(&40_u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&24_u16.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&(pixel_size as u32).to_le_bytes());
    out.extend_from_slice(&2835_u32.to_le_bytes());
    out.extend_from_slice(&2835_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    // Pixels, bottom-up, BGR.
    let zero_row = vec![0_u8; row_pad];
    for y in (0..height).rev() {
        for x in 0..width {
            let base = (y * width + x) * 4;
            if let Some(quad) = rgba8.get(base..base + 4) {
                let &[r, g, b, _] = quad else {
                    continue;
                };
                out.push(b);
                out.push(g);
                out.push(r);
            }
        }
        out.extend_from_slice(&zero_row);
    }
    std::fs::write(path, out)
}
