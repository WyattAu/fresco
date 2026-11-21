//! Desktop runner for kestrel-gui (X11/Wayland window).
fn main() {
    if let Err(e) = kestrel_gui::app::run_desktop() {
        eprintln!("kestrel-gui desktop: {e}");
        std::process::exit(1);
    }
}
