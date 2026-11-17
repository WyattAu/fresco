//! Desktop runner for the game demo (X11/Wayland window).
fn main() {
    if let Err(e) = fresco_game::run_desktop() {
        eprintln!("fresco game desktop: {e}");
        std::process::exit(1);
    }
}
