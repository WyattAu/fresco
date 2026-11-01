//! Golden-image differential harness for the fresco UI engine.
//!
//! L3 layer, dev-only, `publish = false` (estate rule: a differential
//! harness is not a library anyone depends on). Compares two RGBA8
//! buffers under a per-pixel channel delta plus a mismatched-pixel
//! budget, hashes buffers with FNV-1a for stable golden keys, and emits
//! viewable BMP artifacts (golden, actual, diff) on failure.
//!
//! This is the CI oracle for ADR-0004: any future GPU backend must pass
//! against CPU-reference output through this harness (paradigm 950,
//! headless golden images).
#![forbid(unsafe_code)]

use std::fs;
use std::io::Write;
use std::path::Path;

/// Borrowed view over an RGBA8 buffer with explicit stride.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceRef<'a> {
    /// Premultiplied RGBA8 bytes, row-major.
    pub data: &'a [u8],
    /// Pixel width.
    pub width: usize,
    /// Pixel height.
    pub height: usize,
    /// Byte stride (>= width * 4).
    pub stride: usize,
}

impl<'a> SurfaceRef<'a> {
    /// View from raw parts.
    #[must_use]
    pub const fn new(data: &'a [u8], width: usize, height: usize, stride: usize) -> Self {
        SurfaceRef {
            data,
            width,
            height,
            stride,
        }
    }

    fn pixel(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let base = y.checked_mul(self.stride)?.checked_add(x.checked_mul(4)?)?;
        let d = self.data;
        Some([
            *d.get(base)?,
            *d.get(base + 1)?,
            *d.get(base + 2)?,
            *d.get(base + 3)?,
        ])
    }
}

/// Comparison policy.
#[derive(Clone, Copy, Debug)]
pub struct CompareConfig {
    /// Per-pixel channel delta at or below which a pixel matches.
    pub max_channel_delta: u8,
    /// Fraction of pixels allowed to mismatch.
    pub max_mismatch_frac: f64,
}

impl CompareConfig {
    /// Strictest policy: byte-identical.
    #[must_use]
    pub const fn strict() -> Self {
        CompareConfig {
            max_channel_delta: 0,
            max_mismatch_frac: 0.0,
        }
    }
}

/// Successful comparison summary.
#[derive(Clone, Copy, Debug)]
pub struct Report {
    /// Width compared.
    pub width: usize,
    /// Height compared.
    pub height: usize,
    /// Fraction of mismatched pixels.
    pub mismatch_frac: f64,
    /// Largest single-channel delta seen.
    pub max_delta: u8,
    /// FNV-1a hash of buffer A.
    pub hash_a: u64,
    /// FNV-1a hash of buffer B.
    pub hash_b: u64,
}

/// Failed comparison: the report plus the first mismatched pixel.
#[derive(Clone, Copy, Debug)]
pub struct Mismatch {
    /// Full report for assertions.
    pub report: Report,
    /// First mismatched pixel coordinate.
    pub first: Option<(usize, usize)>,
}

/// Compare two surfaces under `cfg`.
///
/// `Ok` when the mismatch fraction is within budget; every pixel whose
/// max channel delta exceeds `max_channel_delta` counts as mismatched.
/// Dimension mismatch is always an error.
pub fn compare(a: &SurfaceRef, b: &SurfaceRef, cfg: CompareConfig) -> Result<Report, Mismatch> {
    let report = Report {
        width: a.width,
        height: a.height,
        mismatch_frac: 0.0,
        max_delta: 0,
        hash_a: fnv1a(a.data),
        hash_b: fnv1a(b.data),
    };
    if a.width != b.width || a.height != b.height {
        return Err(Mismatch {
            report: Report {
                mismatch_frac: 1.0,
                ..report
            },
            first: None,
        });
    }
    let total = a.width.saturating_mul(a.height);
    let mut mismatched: u64 = 0;
    let mut max_delta: u8 = 0;
    let mut first: Option<(usize, usize)> = None;
    for y in 0..a.height {
        for x in 0..a.width {
            let pa = a.pixel(x, y).unwrap_or([0; 4]);
            let pb = b.pixel(x, y).unwrap_or([0; 4]);
            let delta = pa
                .iter()
                .zip(pb.iter())
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            if delta > max_delta {
                max_delta = delta;
            }
            if delta > cfg.max_channel_delta {
                mismatched += 1;
                if first.is_none() {
                    first = Some((x, y));
                }
            }
        }
    }
    let frac = if total == 0 {
        0.0
    } else {
        mismatched as f64 / total as f64
    };
    let report = Report {
        mismatch_frac: frac,
        max_delta,
        ..report
    };
    if frac <= cfg.max_mismatch_frac {
        Ok(report)
    } else {
        Err(Mismatch { report, first })
    }
}

/// FNV-1a 64-bit hash (stable golden keys; reference vectors in tests).
#[must_use]
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Write a surface as a 24-bit BMP (alpha composited over black).
pub fn write_bmp(path: &Path, surface: &SurfaceRef) -> std::io::Result<()> {
    let w = surface.width;
    let h = surface.height;
    let row_bytes = w * 3;
    let pad = (4 - (row_bytes % 4)) % 4;
    let row_padded = row_bytes + pad;
    let image_size = row_padded * h;
    let file_size = 54 + image_size;

    let mut out = Vec::with_capacity(file_size);
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    push_u32(&mut out, file_size as u32);
    push_u32(&mut out, 0);
    push_u32(&mut out, 54);
    // BITMAPINFOHEADER
    push_u32(&mut out, 40);
    push_u32(&mut out, w as u32);
    push_i32(&mut out, h as i32); // positive: bottom-up rows
    push_u16(&mut out, 1);
    push_u16(&mut out, 24);
    push_u32(&mut out, 0);
    push_u32(&mut out, image_size as u32);
    push_i32(&mut out, 2835);
    push_i32(&mut out, 2835);
    push_u32(&mut out, 0);
    push_u32(&mut out, 0);
    // Pixel rows, bottom-up, BGR.
    for y in (0..h).rev() {
        for x in 0..w {
            let px = surface.pixel(x, y).unwrap_or([0; 4]);
            // Premultiplied over black == the values themselves.
            out.push(px[2]);
            out.push(px[1]);
            out.push(px[0]);
        }
        out.resize(out.len() + pad, 0);
    }
    fs::File::create(path)?.write_all(&out)
}

/// Write an amplified diff view: per-pixel max channel delta scaled by 8.
pub fn write_diff(path: &Path, a: &SurfaceRef, b: &SurfaceRef) -> std::io::Result<()> {
    let w = a.width;
    let h = a.height;
    let row_bytes = w * 3;
    let pad = (4 - (row_bytes % 4)) % 4;
    let row_padded = row_bytes + pad;
    let image_size = row_padded * h;
    let file_size = 54 + image_size;

    let mut out = Vec::with_capacity(file_size);
    out.extend_from_slice(b"BM");
    push_u32(&mut out, file_size as u32);
    push_u32(&mut out, 0);
    push_u32(&mut out, 54);
    push_u32(&mut out, 40);
    push_u32(&mut out, w as u32);
    push_i32(&mut out, h as i32);
    push_u16(&mut out, 1);
    push_u16(&mut out, 24);
    push_u32(&mut out, 0);
    push_u32(&mut out, image_size as u32);
    push_i32(&mut out, 2835);
    push_i32(&mut out, 2835);
    push_u32(&mut out, 0);
    push_u32(&mut out, 0);
    for y in (0..h).rev() {
        for x in 0..w {
            let pa = a.pixel(x, y).unwrap_or([0; 4]);
            let pb = b.pixel(x, y).unwrap_or([0; 4]);
            let d = pa
                .iter()
                .zip(pb.iter())
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            let v = d.saturating_mul(8);
            out.push(v);
            out.push(v);
            out.push(v);
        }
        out.resize(out.len() + pad, 0);
    }
    fs::File::create(path)?.write_all(&out)
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn push_i32(out: &mut Vec<u8>, v: i32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn fnv_reference_vectors() {
        // From the FNV reference page (Landon Curt Noll et al).
        assert_eq!(fnv1a(&[]), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn fnv_stable_and_sensitive() {
        let a = [1_u8, 2, 3, 4];
        let b = [1_u8, 2, 3, 5];
        assert_eq!(fnv1a(&a), fnv1a(&a));
        assert_ne!(fnv1a(&a), fnv1a(&b));
    }

    #[test]
    fn golden_compare_identical_passes_strict() {
        let data = [1_u8, 2, 3, 255, 4, 5, 6, 128];
        let a = SurfaceRef::new(&data, 2, 1, 8);
        let b = SurfaceRef::new(&data, 2, 1, 8);
        let report = compare(&a, &b, CompareConfig::strict()).unwrap_or(Report {
            width: 0,
            height: 0,
            mismatch_frac: 1.0,
            max_delta: 255,
            hash_a: 0,
            hash_b: 0,
        });
        assert_eq!(report.mismatch_frac, 0.0);
        assert_eq!(report.max_delta, 0);
        assert_eq!(report.hash_a, report.hash_b);
    }

    #[test]
    fn golden_compare_budget_and_delta_gate() {
        let a_data = [0_u8, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        let mut b_data = a_data;
        b_data[0] = 30; // one pixel, one channel, delta 30
        let a = SurfaceRef::new(&a_data, 2, 2, 8);
        let b = SurfaceRef::new(&b_data, 2, 2, 8);

        // Budget 0: fails with max_delta 30 and first mismatch (0, 0).
        let strict = compare(&a, &b, CompareConfig::strict());
        assert!(strict.is_err(), "delta 30 must fail strict gate");
        let Err(m) = compare(&a, &b, CompareConfig::strict()) else {
            return;
        };
        assert!((m.report.mismatch_frac - 0.25).abs() < 1e-9);
        assert_eq!(m.report.max_delta, 30);
        assert_eq!(m.first, Some((0, 0)));

        // Within delta tolerance: passes even at budget 0.
        let tolerant = CompareConfig {
            max_channel_delta: 30,
            max_mismatch_frac: 0.0,
        };
        assert!(compare(&a, &b, tolerant).is_ok());

        // Delta gate 0 but budget 25 percent: passes.
        let budgeted = CompareConfig {
            max_channel_delta: 0,
            max_mismatch_frac: 0.25,
        };
        assert!(compare(&a, &b, budgeted).is_ok());
        let tight_budget = CompareConfig {
            max_channel_delta: 0,
            max_mismatch_frac: 0.1,
        };
        assert!(compare(&a, &b, tight_budget).is_err());
    }

    #[test]
    fn golden_compare_dimension_mismatch_fails() {
        let a_data = [0_u8; 16];
        let b_data = [0_u8; 12];
        let a = SurfaceRef::new(&a_data, 2, 2, 8);
        let b = SurfaceRef::new(&b_data, 3, 1, 12);
        let result = compare(&a, &b, CompareConfig::strict());
        assert!(result.is_err(), "dimension mismatch must fail");
        let Err(m) = result else {
            return;
        };
        assert!(m.first.is_none());
        assert_eq!(m.report.mismatch_frac, 1.0);
    }

    #[test]
    fn bmp_header_sanity() {
        let data = vec![0_u8; 8 * 4]; // 2x2
        let surf = SurfaceRef::new(&data, 2, 2, 8);
        let path = std::env::temp_dir().join("fresco_conformance_test.bmp");
        assert!(write_bmp(&path, &surf).is_ok(), "bmp write failed");
        let bytes = fs::read(&path).unwrap_or_default();
        let _ = fs::remove_file(&path);
        assert!(bytes.starts_with(b"BM"));
        let size = u32::from_le_bytes([
            *bytes.get(2).unwrap_or(&0),
            *bytes.get(3).unwrap_or(&0),
            *bytes.get(4).unwrap_or(&0),
            *bytes.get(5).unwrap_or(&0),
        ]);
        // 54 header + 2 rows of (2*3 + 2 pad) = 54 + 16 = 70.
        assert_eq!(size, 70);
        let width = u32::from_le_bytes([
            *bytes.get(18).unwrap_or(&0),
            *bytes.get(19).unwrap_or(&0),
            *bytes.get(20).unwrap_or(&0),
            *bytes.get(21).unwrap_or(&0),
        ]);
        assert_eq!(width, 2);
    }

    #[test]
    fn diff_view_writes_amplified_deltas() {
        let a_data = vec![0_u8; 8];
        let mut b_data = vec![0_u8; 8];
        b_data[0] = 10;
        let a = SurfaceRef::new(&a_data, 1, 2, 4);
        let b = SurfaceRef::new(&b_data, 1, 2, 4);
        let path = std::env::temp_dir().join("fresco_conformance_diff_test.bmp");
        assert!(write_diff(&path, &a, &b).is_ok(), "diff write failed");
        let bytes = fs::read(&path).unwrap_or_default();
        let _ = fs::remove_file(&path);
        assert!(bytes.starts_with(b"BM"));
        assert!(bytes.len() > 54);
    }
}
