mod app;
mod ui;

pub use saywrite::{
    cleanup, config, desktop_setup, dictation, integration_api, model_installer,
    native_integration, runtime,
};

fn main() -> glib::ExitCode {
    app::run()
}
