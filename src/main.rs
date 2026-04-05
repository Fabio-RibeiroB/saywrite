mod app;
mod cleanup;
mod config;
mod dictation;
mod host_integration;
mod runtime;
mod ui;

fn main() -> glib::ExitCode {
    app::run()
}
