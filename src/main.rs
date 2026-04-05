mod app;
mod ui;

pub use saywrite::{
    cleanup, config, dictation, host_api, host_integration, model_installer, runtime,
};

fn main() -> glib::ExitCode {
    app::run()
}
