//! fx showcase: the animated capability catalog in a real window.
//!
//! Renders the fx families (backgrounds, text effects, charts) every
//! frame with the animation clock advancing, presents via softbuffer,
//! records through the inspector when `GATE_A_RECORD=1`, prints
//! pacing stats at exit, and closes automatically after the frame
//! count elapses (override with `GATE_A_FRAMES`).
//!
//! Run: cargo run --release --example fx_showcase -p window-kit

use std::time::Instant;

use fresco_charts::{BarChart, LineChart, Sparkline};
use fx::{CountUp, FlickeringGrid, GridPattern, ScrambledText, ShimmerText, StarField};
use widget_kit::{TreeItem, WidgetColors, WidgetKind, WidgetTree, paint_tree};
use window_kit::event::{Event, LogicalSize};
use window_kit::window::{EventSink, Window, WindowAttrs};
use window_kit::winit_backend::WinitRunner;

const W: f64 = 640.0;
const H: f64 = 420.0;

struct ShowcaseSink {
    tree: WidgetTree,
    surface: paint_kit::Surface,
    painter: paint_kit::Painter,
    frame: usize,
    render_us: Vec<u128>,
    intervals_us: Vec<u128>,
    last_present: Option<Instant>,
    inspector: window_kit::inspector::Inspector,
}

impl ShowcaseSink {
    fn new() -> Self {
        let mut tree = WidgetTree::new();
        let root = tree.push(WidgetKind::Column { gap: 8.0 });
        let overlay = tree.push(WidgetKind::TreeList {
            items: vec![
                TreeItem {
                    label: "fresco fx".to_string(),
                    depth: 0,
                    has_children: true,
                    expanded: true,
                },
                TreeItem {
                    label: "solid-fx catalog".to_string(),
                    depth: 1,
                    has_children: false,
                    expanded: false,
                },
                TreeItem {
                    label: "plychart charts".to_string(),
                    depth: 1,
                    has_children: false,
                    expanded: false,
                },
            ],
            selected: 0,
        });
        tree.add_child(root, overlay);
        tree.compute(widget_kit::Size2D {
            width: W,
            height: H,
        });
        ShowcaseSink {
            tree,
            surface: paint_kit::Surface::new(W as usize, H as usize).unwrap_or_default(),
            painter: paint_kit::Painter::default(),
            frame: 0,
            render_us: Vec::new(),
            intervals_us: Vec::new(),
            last_present: None,
            inspector: window_kit::inspector::Inspector::from_env(),
        }
    }

    fn paint_frame(&mut self, width: usize, height: usize, t: f64) {
        self.surface = paint_kit::Surface::new(width, height).unwrap_or_default();
        paint_tree(
            &self.tree,
            &mut self.surface,
            &mut self.painter,
            &WidgetColors::default(),
        );
        // Backgrounds first (under the tree overlay).
        {
            let mut painter = paint_kit::Painter::default();
            let mut fx = fx::Fx::new(&mut self.surface, &mut painter, t);
            GridPattern::new().draw(&mut fx, (W * 0.5) as usize, H as usize);
            FlickeringGrid::new().draw(&mut fx, (W * 0.5) as usize, H as usize);
            StarField::new().draw(&mut fx, (W * 0.5) as usize, H as usize);
        }
        // Charts column on the right.
        {
            let mut painter = paint_kit::Painter::default();
            let mut chart_fx = fresco_charts::ChartFx::new(&mut self.surface, &mut painter, t);
            let chart_x = W * 0.52;
            let chart_w = W * 0.44;
            Sparkline::new(vec![
                3.0, 5.0, 4.0, 8.0, 6.0, 9.0, 7.0, 12.0, 10.0, 14.0, 11.0, 16.0,
            ])
            .draw(&mut chart_fx, rect(chart_x, 20.0, chart_w, 90.0), t);
            BarChart::new(vec![4.0, 9.0, 6.0, 12.0, 7.0, 10.0, 5.0, 11.0]).draw(
                &mut chart_fx,
                rect(chart_x, 130.0, chart_w, 90.0),
                t,
            );
            LineChart::new(vec![5.0, 7.0, 4.0, 9.0, 6.0, 11.0, 8.0, 6.0, 13.0]).draw(
                &mut chart_fx,
                rect(chart_x, 240.0, chart_w, 90.0),
                t,
            );
        }
        // Text effects along the bottom.
        {
            let mut painter = paint_kit::Painter::default();
            let mut fx = fx::Fx::new(&mut self.surface, &mut painter, t);
            CountUp::new(218.0).draw(&mut fx, 20.0, 340.0);
            ScrambledText::new("fresco fx").draw(&mut fx, 120.0, 348.0);
            ShimmerText::new("solid-fx + plychart capability").draw(&mut fx, 20.0, 380.0);
        }
    }

    fn report(&self) {
        println!(
            "fx showcase: {} frames on the live display",
            self.render_us.len()
        );
        let mut render = self.render_us.clone();
        render.sort();
        if render.len() > 2 {
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
            let within = intervals.iter().filter(|t| **t <= 17_000).count();
            println!(
                "  present-to-present us: mean {mean} p50 {p50}, within 16.7 ms: {within}/{}",
                intervals.len()
            );
        }
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> paint_kit::Rect {
    paint_kit::Rect::from_size(paint_kit::Vec2::new(x, y), paint_kit::Vec2::new(w, h))
}

impl EventSink for ShowcaseSink {
    fn event(
        &mut self,
        window: &mut dyn Window,
        present: &mut dyn window_kit::window::Present,
        event: &Event,
    ) {
        match event {
            Event::RedrawRequested | Event::Resized(_) => {
                let frame_count = std::env::var("GATE_A_FRAMES")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(300);
                if self.frame >= frame_count {
                    self.inspector.finish(
                        std::env::var("GATE_A_VIDEO")
                            .map(|v| v == "1")
                            .unwrap_or(false),
                    );
                    self.report();
                    std::process::exit(0);
                }
                let phys = window.physical_size();
                let t = self.frame as f64 / 60.0;
                let start = Instant::now();
                self.paint_frame(phys.width as usize, phys.height as usize, t);
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
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

fn main() {
    let runner = WinitRunner::new(vec![WindowAttrs {
        title: "fresco fx showcase".to_string(),
        logical_size: Some(LogicalSize {
            width: W,
            height: H,
        }),
        resizable: true,
    }]);
    let Ok(runner) = runner else {
        eprintln!("winit init failed (no display?)");
        std::process::exit(2);
    };
    let mut sink = ShowcaseSink::new();
    let _ = runner.run(&mut sink);
}
