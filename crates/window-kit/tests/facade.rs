//! Facade tests, all headless via the fake backend (REQ-W1..W4).

use std::cell::RefCell;

use window_kit::backend::fake::{FakePresent, FakeRunner};
use window_kit::event::PhysicalSize;
use window_kit::event::{Event, KeyCharState, LogicalPoint, LogicalSize, WindowId};
use window_kit::window::{Present, PresentError, Window, WindowAttrs};
use window_kit::{EventSink, FrameClock, Runner, Ticker};

#[derive(Default)]
struct RecordingSink {
    seen: RefCell<Vec<String>>,
    requested: u32,
}

impl RecordingSink {
    fn log(&self, s: &str) {
        self.seen.borrow_mut().push(s.to_string());
    }
}

impl EventSink for RecordingSink {
    fn event(&mut self, window: &mut dyn Window, present: &mut dyn Present, event: &Event) {
        match *event {
            Event::Resized(PhysicalSize { width, height }) => {
                self.log("resized");
                // Present a full frame sized to the window.
                let phys = window.physical_size();
                let _ = (width, height);
                let len = usize::try_from(phys.width)
                    .ok()
                    .and_then(|w| w.checked_mul(phys.height as usize))
                    .and_then(|px| px.checked_mul(4))
                    .unwrap_or(0);
                let pixels = vec![128_u8; len];
                let res = present.present(window.id(), phys.width, phys.height, &pixels);
                assert!(res.is_ok(), "present must succeed for sized frames");
            }
            Event::CursorMoved(at) => {
                self.log("cursor");
                let _ = at;
            }
            Event::KeyChar { ch, state } => {
                let st = match state {
                    KeyCharState::Pressed => "down",
                    KeyCharState::Released => "up",
                };
                self.log(&format!("key {ch} {st}"));
            }
            Event::CloseRequested => {
                self.log("close");
                window.set_title("closing");
            }
            _ => self.log("other"),
        }
        let _ = &self.requested;
    }
}

fn attrs(title: &str) -> WindowAttrs {
    WindowAttrs {
        title: title.to_string(),
        logical_size: Some(LogicalSize {
            width: 200.0,
            height: 100.0,
        }),
        resizable: true,
    }
}

#[test]
fn fake_runner_delivers_scripted_events_in_order() {
    let mut runner = FakeRunner::new();
    let w = runner.add_window(&attrs("win"));
    runner.push(w, Event::RedrawRequested);
    runner.push(w, Event::CursorMoved(LogicalPoint { x: 10.0, y: 20.0 }));
    runner.push(w, Event::CloseRequested);
    let mut sink = RecordingSink::default();
    assert!(runner.run(&mut sink).is_ok(), "fake run must succeed");
    let seen = sink.seen.borrow().clone();
    assert_eq!(
        seen,
        vec![
            "other".to_string(),
            "cursor".to_string(),
            "close".to_string()
        ],
        "order must follow the script"
    );
}

#[test]
fn fake_window_dpi_and_physical_math() {
    let mut runner = FakeRunner::new();
    let w = runner.add_window(&attrs("dpi"));
    let Some(win) = runner.window(w) else {
        return;
    };
    win.set_scale(2.0);
    assert_eq!(win.scale(), 2.0);
    let phys = win.physical_size();
    assert_eq!(phys.width, 400);
    assert_eq!(phys.height, 200);
    let p = win.to_logical(40.0, 30.0);
    assert!((p.x - 20.0).abs() < 1e-9 && (p.y - 15.0).abs() < 1e-9);
    let logical = win.logical_size();
    assert!((logical.width - 200.0).abs() < 1e-9);
}

#[test]
fn fake_present_captures_and_validates() {
    let mut present = FakePresent::default();
    let id = WindowId(7);
    let ok = present.present(id, 2, 2, &[0_u8; 16]);
    assert!(ok.is_ok());
    assert_eq!(present.frames().len(), 1);
    let err = present.present(id, 2, 2, &[0_u8; 15]);
    assert_eq!(
        err,
        Err(PresentError::SizeMismatch {
            expected: 16,
            got: 15
        })
    );
    let err = present.present(WindowId(9), 2, 2, &[0_u8; 16]);
    // FakePresent accepts any known-format frame; unknown windows are a
    // runner-level concern, not a present-level one.
    assert!(err.is_ok());
}

#[test]
fn frame_clock_statistics() {
    let mut clock = FrameClock::default();
    assert_eq!(clock.frames(), 0);
    clock.begin_frame();
    let d = clock.end_frame();
    assert!(d >= Duration::ZERO);
    clock.begin_frame();
    let _ = clock.end_frame();
    assert_eq!(clock.frames(), 2);
    assert!(clock.mean() >= Duration::ZERO);
    // end without begin is defined: zero duration.
    let d = clock.end_frame();
    assert_eq!(d, Duration::ZERO);
}

use std::time::Duration;
