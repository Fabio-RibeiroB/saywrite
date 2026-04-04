mod app;
mod bridge;
mod config;
mod runtime;
mod ui;

fn main() -> glib::ExitCode {
    app::run()
}
