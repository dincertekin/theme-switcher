#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
// Only `app` uses it outside of tests, so it's dead code on other platforms.
#[cfg_attr(not(windows), allow(dead_code))]
mod solar;

#[cfg(windows)]
fn main() {
    app::run();
}

// The sun math builds and tests anywhere; the app itself is Windows-only.
#[cfg(not(windows))]
fn main() {
    eprintln!("theme-switcher only runs on Windows.");
    std::process::exit(1);
}
