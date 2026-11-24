//! Deterministic video export for the fresco stack (the app-support
//! layer above paint-kit; unpublished like render-conformance).
//!
//! The core is a timed frame loop: for each frame the exporter hands
//! a fresh zeroed [`Surface`] to a caller draw closure together with
//! the frame's timestamp, then writes the finished surface into a
//! [`FrameSink`]. The reference sink writes a numbered 24-bit BMP
//! sequence (the estate's zero-dependency image convention, matching
//! render-conformance's artifacts); real output backends (an ffmpeg
//! rawvideo pipe) live behind default-off features.
//!
//! Determinism contract: two exports of the same draw closure are
//! byte-identical. The closure must fully repaint the surface (the
//! exporter guarantees a zeroed canvas per frame); wall-clock state
//! is forbidden by construction because the only clock is the frame
//! timestamp.

use std::fmt;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use paint_kit::Surface;

/// Export failure modes.
#[derive(Debug)]
pub enum VideoError {
    /// The sink or the filesystem failed.
    Io(io::Error),
    /// The requested dimensions cannot form a surface.
    Dimensions(usize, usize),
}

impl fmt::Display for VideoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VideoError::Io(e) => write!(f, "video export i/o: {e}"),
            VideoError::Dimensions(w, h) => {
                write!(f, "cannot form a {w}x{h} surface")
            }
        }
    }
}

impl std::error::Error for VideoError {}

impl From<io::Error> for VideoError {
    fn from(e: io::Error) -> Self {
        VideoError::Io(e)
    }
}

/// What an export produced: frame count and written bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportStats {
    frames: u64,
    bytes: u64,
}

impl ExportStats {
    /// Frames the sink accepted.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// Bytes the sink wrote.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// A destination for finished frames: numbered stills, a pipe into an
/// encoder, or any other consumer of premultiplied RGBA8 surfaces.
pub trait FrameSink {
    /// Accept one finished frame. `index` counts from zero; `t_seconds`
    /// is the frame's presentation timestamp (`index / fps`).
    fn write_frame(&mut self, surface: &Surface, index: u32, t_seconds: f64) -> io::Result<()>;

    /// Complete the export and report what was written.
    ///
    /// # Errors
    /// When the sink cannot flush or close its output.
    fn finish(&mut self) -> io::Result<ExportStats>;
}

/// The oracle sink: a numbered 24-bit BMP sequence
/// (`{prefix}_{index:05}.bmp`) in a directory - the estate's
/// zero-dependency image convention (mirrors render-conformance's
/// artifacts), human-viewable and trivially parseable by tests.
pub struct BmpSequenceSink {
    dir: PathBuf,
    prefix: String,
    frames: u64,
    bytes: u64,
}

impl BmpSequenceSink {
    /// New sink creating `dir` (parents included) if absent.
    ///
    /// # Errors
    /// When the directory cannot be created.
    pub fn new(dir: &Path, prefix: &str) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(BmpSequenceSink {
            dir: dir.to_path_buf(),
            prefix: prefix.to_string(),
            frames: 0,
            bytes: 0,
        })
    }

    /// The path frame `index` will take (oracle use).
    #[must_use]
    pub fn frame_path(&self, index: u32) -> PathBuf {
        self.dir.join(format!("{}_{index:05}.bmp", self.prefix))
    }
}

impl FrameSink for BmpSequenceSink {
    fn write_frame(&mut self, surface: &Surface, index: u32, _t_seconds: f64) -> io::Result<()> {
        let bytes = write_bmp24(&self.frame_path(index), surface)?;
        self.frames += 1;
        self.bytes += bytes;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<ExportStats> {
        Ok(ExportStats {
            frames: self.frames,
            bytes: self.bytes,
        })
    }
}

/// Render `frames` frames at `fps` through `draw` into `sink`.
///
/// Each frame gets a fresh zeroed [`Surface`]; `draw` receives the
/// frame's timestamp `t = index / fps` in seconds and must fully
/// repaint the canvas. The frame index and timestamp are the only
/// inputs, so exports are byte-identical across runs.
///
/// # Errors
/// When the dimensions cannot form a surface or the sink fails.
pub fn export(
    width: usize,
    height: usize,
    fps: u32,
    frames: u32,
    sink: &mut impl FrameSink,
    draw: &mut dyn FnMut(f64, &mut Surface),
) -> Result<ExportStats, VideoError> {
    for index in 0..frames {
        let mut surface =
            Surface::new(width, height).ok_or(VideoError::Dimensions(width, height))?;
        let t = f64::from(index) / f64::from(fps.max(1));
        draw(t, &mut surface);
        sink.write_frame(&surface, index, t)?;
    }
    Ok(sink.finish()?)
}

/// FNV-1a 64-bit (the estate's stable golden key; matches
/// render-conformance's hash).
#[must_use]
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Write a surface as a 24-bit BMP (alpha composited over black;
/// premultiplied-over-black values are the values themselves).
/// Returns the bytes written.
fn write_bmp24(path: &Path, surface: &Surface) -> io::Result<u64> {
    let w = surface.width();
    let h = surface.height();
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
    let data = surface.data();
    for y in (0..h).rev() {
        for x in 0..w {
            let base = (y * surface.stride() + x * 4).min(data.len().saturating_sub(4));
            let px = data
                .get(base..base + 4)
                .and_then(|q| <[u8; 4]>::try_from(q).ok())
                .unwrap_or([0; 4]);
            out.push(px[2]);
            out.push(px[1]);
            out.push(px[0]);
        }
        out.resize(out.len() + pad, 0);
    }
    std::fs::File::create(path)?.write_all(&out)?;
    Ok(out.len() as u64)
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

/// The real output backend: pipes raw RGBA frames into an external
/// `ffmpeg` process (libx264 -> mp4). Off by default; the oracle test
/// skips gracefully when the binary is absent so CI runners without
/// ffmpeg still run the full BMP-sequence oracle path.
#[cfg(feature = "ffmpeg")]
pub mod ffmpeg_pipe {
    use super::{ExportStats, FrameSink};
    use std::io;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdin, Command, Stdio};

    use paint_kit::Surface;

    /// Whether an `ffmpeg` binary is on `PATH` (zero-dep scan).
    #[must_use]
    pub fn ffmpeg_available() -> bool {
        let Some(path_var) = std::env::var_os("PATH") else {
            return false;
        };
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("ffmpeg");
            if candidate.is_file() {
                return true;
            }
        }
        false
    }

    /// A pipe into `ffmpeg -f rawvideo -pix_fmt rgba ... -c:v libx264`.
    pub struct FfmpegPipeSink {
        child: Child,
        stdin: Option<ChildStdin>,
        output_path: PathBuf,
        frames: u64,
        bytes: u64,
    }

    impl FfmpegPipeSink {
        /// Spawn the encoder for `width`x`height` frames at `fps`.
        ///
        /// Frames must be opaque (alpha 255): ffmpeg's `rgba` is
        /// straight alpha, and opaque premultiplied equals straight.
        ///
        /// # Errors
        /// When the process cannot spawn (binary absent, spawn
        /// failure) or the output path has no parent directory.
        pub fn spawn(output: &Path, width: usize, height: usize, fps: u32) -> io::Result<Self> {
            let parent = output.parent().filter(|p| !p.as_os_str().is_empty());
            if let Some(dir) = parent {
                std::fs::create_dir_all(dir)?;
            }
            let mut child = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgba",
                    "-s",
                    &format!("{width}x{height}"),
                    "-r",
                    &fps.to_string(),
                    "-i",
                    "pipe:0",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "veryfast",
                    "-pix_fmt",
                    "yuv420p",
                    "-movflags",
                    "+faststart",
                ])
                .arg(output)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let stdin = child.stdin.take();
            let Some(stdin) = stdin else {
                return Err(io::Error::other("ffmpeg stdin not captured"));
            };
            Ok(FfmpegPipeSink {
                child,
                stdin: Some(stdin),
                output_path: output.to_path_buf(),
                frames: 0,
                bytes: 0,
            })
        }

        /// The encoded output path (oracle use).
        #[must_use]
        pub fn output_path(&self) -> &Path {
            &self.output_path
        }
    }

    impl FrameSink for FfmpegPipeSink {
        fn write_frame(
            &mut self,
            surface: &Surface,
            _index: u32,
            _t_seconds: f64,
        ) -> io::Result<()> {
            let Some(stdin) = self.stdin.as_mut() else {
                return Err(io::Error::other("encoder stdin already closed"));
            };
            // Row-packed tight RGBA regardless of the surface stride.
            let stride = surface.stride();
            let w4 = surface.width() * 4;
            for y in 0..surface.height() {
                let start = y * stride;
                let Some(row) = surface.data().get(start..start + w4) else {
                    return Err(io::Error::other("surface row out of range"));
                };
                stdin.write_all(row)?;
                self.bytes += row.len() as u64;
            }
            self.frames += 1;
            Ok(())
        }

        fn finish(&mut self) -> io::Result<ExportStats> {
            // EOF closes the pipe; ffmpeg finalizes the container.
            drop(self.stdin.take());
            let status = self.child.wait()?;
            if !status.success() {
                return Err(io::Error::other("ffmpeg exited non-zero"));
            }
            let out_bytes = std::fs::metadata(&self.output_path)
                .map(|m| m.len())
                .unwrap_or(0);
            Ok(ExportStats {
                frames: self.frames,
                bytes: out_bytes,
            })
        }
    }

    impl Drop for FfmpegPipeSink {
        fn drop(&mut self) {
            drop(self.stdin.take());
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A draw closure that fully repaints with a time-shifting color.
    fn shift_draw(t: f64, surface: &mut Surface) {
        let channel = (t * 127.0) as u8;
        surface.fill_solid([channel, 40, 200 - channel, 255]);
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "video-kit-{tag}-{}-{}",
            std::process::id(),
            fnv1a(tag.as_bytes())
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Oracle: two exports of the same draw closure are byte-
    /// identical - the determinism contract.
    #[test]
    fn export_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let dir_a = scratch_dir("det-a");
        let dir_b = scratch_dir("det-b");
        let mut sink_a = BmpSequenceSink::new(&dir_a, "f")?;
        let stats_a = export(64, 48, 10, 12, &mut sink_a, &mut shift_draw)?;
        sink_a.finish()?;
        let mut sink_b = BmpSequenceSink::new(&dir_b, "f")?;
        let stats_b = export(64, 48, 10, 12, &mut sink_b, &mut shift_draw)?;
        sink_b.finish()?;
        assert_eq!(stats_a, stats_b, "identical exports, identical stats");
        for index in 0..12 {
            let a = std::fs::read(dir_a.join(format!("f_{index:05}.bmp")))?;
            let b = std::fs::read(dir_b.join(format!("f_{index:05}.bmp")))?;
            assert_eq!(fnv1a(&a), fnv1a(&b), "frame {index} bytes differ");
        }
        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
        Ok(())
    }

    /// Oracle: every written frame parses as a 24-bit BMP with the
    /// exported dimensions, and the sink counts frames and bytes.
    #[test]
    fn frame_files_parse_as_bmp() -> Result<(), Box<dyn std::error::Error>> {
        let dir = scratch_dir("parse");
        let mut sink = BmpSequenceSink::new(&dir, "f")?;
        let stats = export(48, 32, 8, 6, &mut sink, &mut shift_draw)?;
        sink.finish()?;
        assert_eq!(stats.frames(), 6, "frame count");
        assert!(stats.bytes() > 6 * 54, "bytes beyond headers");
        for index in 0..6_u32 {
            let bytes = std::fs::read(dir.join(format!("f_{index:05}.bmp")))?;
            assert_eq!(bytes.first(), Some(&b'B'), "magic B");
            assert_eq!(bytes.get(1), Some(&b'M'), "magic M");
            let read_u32 = |at: usize| -> Option<u32> {
                let q = bytes.get(at..at + 4)?;
                Some(u32::from_le_bytes([q[0], q[1], q[2], q[3]]))
            };
            assert_eq!(read_u32(18), Some(48), "header width");
            assert_eq!(read_u32(22), Some(32), "header height");
            assert_eq!(read_u32(28), Some(24), "bit depth");
        }
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// Oracle: the timeline drives pixels - frames at t=0, middle and
    /// end all differ, so time actually reaches the canvas.
    #[test]
    fn timeline_drives_pixels() -> Result<(), Box<dyn std::error::Error>> {
        let dir = scratch_dir("time");
        let mut sink = BmpSequenceSink::new(&dir, "f")?;
        export(32, 32, 10, 11, &mut sink, &mut shift_draw)?;
        sink.finish()?;
        let hash = |index: u32| -> io::Result<u64> {
            Ok(fnv1a(&std::fs::read(
                dir.join(format!("f_{index:05}.bmp")),
            )?))
        };
        let first = hash(0)?;
        let mid = hash(5)?;
        let last = hash(10)?;
        assert_ne!(first, mid, "t=0 vs t=0.5");
        assert_ne!(mid, last, "t=0.5 vs t=1.0");
        assert_ne!(first, last, "t=0 vs t=1.0");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    /// Oracle: the FNV-1a key matches the reference vectors.
    #[test]
    fn fnv1a_reference_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    /// Oracle (ffmpeg feature): the pipe backend produces an mp4
    /// whose duration matches frames/fps; skipped gracefully when
    /// the binary is absent so CI runners without ffmpeg stay green.
    #[cfg(feature = "ffmpeg")]
    #[test]
    fn ffmpeg_pipe_encodes_duration() -> Result<(), Box<dyn std::error::Error>> {
        use super::ffmpeg_pipe::{FfmpegPipeSink, ffmpeg_available};
        use std::process::Command;
        if !ffmpeg_available() {
            eprintln!("skip: ffmpeg binary absent");
            return Ok(());
        }
        let dir = scratch_dir("mp4");
        let out = dir.join("out.mp4");
        let mut sink = FfmpegPipeSink::spawn(&out, 320, 180, 24)?;
        let stats = export(320, 180, 24, 24, &mut sink, &mut shift_draw)?;
        sink.finish()?;
        assert_eq!(stats.frames(), 24, "frames piped");
        assert!(out.is_file(), "mp4 written");
        assert!(
            std::fs::metadata(&out)?.len() > 1000,
            "mp4 carries encoded content"
        );
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(&out)
            .output()?;
        let text = String::from_utf8_lossy(&probe.stdout).trim().to_string();
        let duration: f64 = text.parse().unwrap_or(0.0);
        assert!(
            (0.8..=1.2).contains(&duration),
            "duration ~1.0s (24 frames / 24 fps), got {duration}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
