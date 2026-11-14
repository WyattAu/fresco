//! Desktop runner for the M2 showcase (X11/Wayland window).
fn main() {
    if let Err(e) = fresco_android::run_desktop() {
        eprintln!("fresco M2 desktop: {e}");
        std::process::exit(1);
    }
}
