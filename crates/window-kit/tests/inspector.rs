//! Inspector tests: BMP writer, changed-pixel analysis, recording
//! cadence, manifest + analysis artifacts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use window_kit::inspector::{Inspector, write_bmp24};

#[test]
fn bmp_header_and_content_are_valid() {
    // 2x2 opaque red image: BGR bottom-up.
    let rgba: [u8; 16] = [
        255, 0, 0, 255, // (0,0)
        0, 255, 0, 255, // (1,0)
        0, 0, 255, 255, // (0,1)
        255, 255, 0, 255, // (1,1)
    ];
    let path = std::path::Path::new("/tmp/opencode/inspector-test.bmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    write_bmp24(path, &rgba, 2, 2).expect("write must succeed");
    let bytes = std::fs::read(path).expect("file must exist");
    // Header: magic, size, offset 54.
    assert_eq!(&bytes[0..2], b"BM");
    assert_eq!(bytes[10..14], 54_u32.to_le_bytes());
    // 2 wide: row = 6 bytes + 2 pad = 8; two rows = 16 pixel bytes.
    assert_eq!(bytes[34..38], 16_u32.to_le_bytes());
    // Bottom-up: file row 0 is image row 1 (BGR), then padding.
    // Blue (0,0,255) -> BGR [255,0,0]; yellow (255,255,0) -> [0,255,255].
    assert_eq!(&bytes[54..62], &[255, 0, 0, 0, 255, 255, 0, 0]);
    // File row 1 is image row 0: red -> [0,0,255]; green -> [0,255,0].
    assert_eq!(&bytes[62..70], &[0, 0, 255, 0, 255, 0, 0, 0]);
}

#[test]
fn inspector_counts_changed_pixels() {
    let dir = std::env::temp_dir().join("fresco-inspector-test");
    std::fs::create_dir_all(&dir).ok();
    let mut inspector = Inspector::with_dir(dir, true, 1, 0);

    // Frame 0: solid dark (first frame: everything counts as changed).
    let frame_a = [10_u8; 16];
    inspector.record_frame(&frame_a, 2, 2, 100, 1);
    // Frame 1: identical -> zero changed.
    inspector.record_frame(&frame_a, 2, 2, 100, 1);
    // Frame 2: one byte differs -> 1 changed byte.
    let mut frame_d = frame_a;
    frame_d[0] = 99;
    inspector.record_frame(&frame_d, 2, 2, 100, 1);

    let frames = inspector.frames();
    assert_eq!(frames.len(), 3);
    assert!(
        // Per-pixel: a 2x2 frame has 4 pixels, all changed on first
        // draw.
        frames[0].changed_pixels == 4,
        "first frame: all bytes changed"
    );
    assert_eq!(frames[1].changed_pixels, 0, "identical frame");
    assert_eq!(frames[2].changed_pixels, 1, "one byte differs");
    assert_ne!(frames[2].hash, frames[0].hash, "hashes must differ");
}

#[test]
fn inspector_disabled_records_nothing() {
    let dir = std::env::temp_dir().join("fresco-inspector-off");
    std::fs::create_dir_all(&dir).ok();
    let mut inspector = Inspector::with_dir(dir, false, 1, 0);
    inspector.record_frame(&[1_u8; 16], 2, 2, 50, 0);
    assert!(inspector.frames().is_empty(), "record_on=false must skip");
}
