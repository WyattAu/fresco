//! Gate A evidence: a real window, a real winit event loop, and the
//! settings screen painted every frame through the CPU reference.
//! Measures winit frame pacing (present-to-present intervals) and
//! paint+present cost separately, over 300 frames, prints the stats,
//! and exits.
//!
//! Run: cargo run --release --example gate_a_pacing -p window-kit
//! (requires a display; the default features enable winit).

use std::time::Instant;

use widget_kit::{TreeItem, WidgetColors, WidgetKind, WidgetTree, paint_tree};
use window_kit::event::{Event, LogicalSize};
use window_kit::window::{EventSink, Present, Window, WindowAttrs};
use window_kit::winit_backend::WinitRunner;

const FRAMES: usize = 300;

fn frame_count() -> usize {
    std::env::var("GATE_A_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(FRAMES)
}

struct PacingSink {
    tree: WidgetTree,
    surface: paint_kit::Surface,
    painter: paint_kit::Painter,
    colors: WidgetColors,
    frame: usize,
    render_us: Vec<u128>,
    intervals_us: Vec<u128>,
    last_present: Option<Instant>,
    inspector: window_kit::inspector::Inspector,
}

impl PacingSink {
    fn new() -> Self {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 8.0 });
        let header = tree.push(WidgetKind::Label {
            text: "Settings".to_string(),
            px_size: 20.0,
        });
        tree.add_child(root, header);
        let row = tree.push(WidgetKind::Row { gap: 10.0 });
        let save = tree.push(WidgetKind::Button {
            label: "Save".to_string(),
            action: 1,
        });
        let checkbox = tree.push(WidgetKind::Checkbox {
            label: "Compact".to_string(),
            checked: true,
        });
        tree.add_child(root, row);
        tree.add_child(row, save);
        tree.add_child(row, checkbox);
        let sync = tree.push(WidgetKind::Select {
            options: vec![
                "Instant".to_string(),
                "Hourly".to_string(),
                "Daily".to_string(),
            ],
            selected: 0,
            open: false,
        });
        tree.add_child(root, sync);
        let outline = tree.push(WidgetKind::TreeList {
            items: vec![
                TreeItem {
                    label: "Projects".to_string(),
                    depth: 0,
                    has_children: true,
                    expanded: true,
                },
                TreeItem {
                    label: "fresco".to_string(),
                    depth: 1,
                    has_children: false,
                    expanded: false,
                },
                TreeItem {
                    label: "kestrel".to_string(),
                    depth: 1,
                    has_children: false,
                    expanded: false,
                },
            ],
            selected: 1,
        });
        tree.add_child(root, outline);
        PacingSink {
            tree,
            surface: paint_kit::Surface::new(360, 640).unwrap_or_default(),
            painter: paint_kit::Painter::default(),
            colors: WidgetColors::default(),
            frame: 0,
            render_us: Vec::new(),
            intervals_us: Vec::new(),
            last_present: None,
            inspector: window_kit::inspector::Inspector::from_env(),
        }
    }

    fn paint_frame(&mut self, width: usize, height: usize) {
        self.surface = paint_kit::Surface::new(width, height).unwrap_or_default();
        paint_tree(
            &self.tree,
            &mut self.surface,
            &mut self.painter,
            &self.colors,
        );
    }

    fn report(&self) {
        println!(
            "Gate A: {} frames on the live display",
            self.render_us.len()
        );
        let mut render = self.render_us.clone();
        render.sort();
        if !render.is_empty() {
            let mean = render.iter().sum::<u128>() / render.len() as u128;
            let Some(&p50) = render.get(render.len() / 2) else {
                return;
            };
            let Some(&p99) =
                render.get(((render.len() as f64 * 0.99).round() as usize).min(render.len() - 1))
            else {
                return;
            };
            let Some(&max) = render.last() else {
                return;
            };
            println!("  paint+present us: mean {mean} p50 {p50} p99 {p99} max {max}");
        }
        let mut intervals = self.intervals_us.clone();
        intervals.sort();
        if intervals.len() > 2 {
            let mean: u128 = intervals.iter().sum::<u128>() / intervals.len() as u128;
            let Some(&p50) = intervals.get(intervals.len() / 2) else {
                return;
            };
            let Some(&max) = intervals.last() else {
                return;
            };
            let within = intervals.iter().filter(|t| **t <= 17_000).count();
            println!("  present-to-present us: mean {mean} p50 {p50} max {max}");
            println!(
                "  frames within the 16.7 ms budget: {within}/{} ({:.1}%)",
                intervals.len(),
                100.0 * within as f64 / intervals.len() as f64
            );
        }
        println!("Gate A verdict: compare the interval stats against the 16.7 ms 60 Hz budget.");
    }
}

impl EventSink for PacingSink {
    fn event(&mut self, window: &mut dyn Window, present: &mut dyn Present, event: &Event) {
        match event {
            Event::RedrawRequested | Event::Resized(_) => {
                if self.frame >= frame_count() {
                    self.inspector.finish(
                        std::env::var("GATE_A_VIDEO")
                            .map(|v| v == "1")
                            .unwrap_or(false),
                    );
                    self.report();
                    std::process::exit(0);
                }
                let phys = window.physical_size();
                let start = Instant::now();
                self.paint_frame(phys.width as usize, phys.height as usize);
                let ok = present.present(window.id(), phys.width, phys.height, self.surface.data());
                let elapsed = start.elapsed().as_micros();
                self.render_us.push(elapsed);
                if let Some(last) = self.last_present {
                    self.intervals_us
                        .push(start.duration_since(last).as_micros());
                }
                self.last_present = Some(start);
                let _ = ok;
                self.inspector.record_frame(
                    self.surface.data(),
                    phys.width as usize,
                    phys.height as usize,
                    elapsed as u64,
                    0,
                );
                self.frame += 1;
                window.request_redraw();
            }
            Event::CloseRequested => {
                self.report();
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

fn main() {
    let runner = WinitRunner::new(vec![WindowAttrs {
        title: "fresco Gate A pacing".to_string(),
        logical_size: Some(LogicalSize {
            width: 360.0,
            height: 640.0,
        }),
        resizable: true,
    }]);
    let Ok(runner) = runner else {
        eprintln!("winit init failed (no display?) - Gate A bench needs a windowing session");
        std::process::exit(2);
    };
    let mut sink = PacingSink::new();
    let _ = runner.run(&mut sink);
    // If the loop exits by closing the window, still print whatever we
    // collected.
    sink.report();
}
