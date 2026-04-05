mod app;
mod cleanup;
mod config;
mod dictation;
mod host_api;
mod host_integration;
mod model_installer;
mod runtime;
mod ui;

fn main() -> glib::ExitCode {
    app::run()
}
