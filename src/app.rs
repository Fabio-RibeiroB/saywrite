use libadwaita as adw;
use std::{cell::RefCell, path::Path, process::Command, rc::Rc};

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::{
    config::{self, AppSettings},
    ui,
};

pub const APP_ID: &str = "io.github.fabio.SayWrite";

pub fn run() -> glib::ExitCode {
    register_resources();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    app.connect_startup(|_| {
        load_css();
        start_host_daemon();
    });
    app.connect_shutdown(|_| {
        stop_host_daemon();
    });
    app.connect_activate(activate);
    app.run()
}

fn activate(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }

    let settings = Rc::new(RefCell::new(AppSettings::load()));
    if settings.borrow().onboarding_complete {
        ui::main_window::present(app, settings);
    } else {
        let app_clone = app.clone();
        let settings_clone = settings.clone();
        ui::onboarding::present(app, settings, move || {
            ui::main_window::present(&app_clone, settings_clone.clone());
        });
    }
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_resource("/io/github/fabio/SayWrite/style.css");
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn start_host_daemon() {
    arm_host_session();
    run_user_systemctl(&["unmask", "saywrite-host.service"]);
    run_user_systemctl(&["start", "saywrite-host.service"]);
}

fn stop_host_daemon() {
    disarm_host_session();
    run_user_systemctl(&["stop", "saywrite-host.service"]);
    run_user_systemctl(&["mask", "saywrite-host.service"]);
}

fn arm_host_session() {
    if inside_flatpak() {
        let script = format!(
            "STATE_HOME=\"${{XDG_STATE_HOME:-$HOME/.local/state}}\"; mkdir -p \"$STATE_HOME/{app_dir}\"; printf '%s\\n' '{pid}' > \"$STATE_HOME/{app_dir}/{marker}\"",
            app_dir = config::APP_DIR_NAME,
            marker = config::HOST_SESSION_MARKER_NAME,
            pid = std::process::id(),
        );
        run_host_shell(&script);
        return;
    }

    let marker = config::host_session_marker_path();
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker, format!("{}\n", std::process::id()));
}

fn disarm_host_session() {
    if inside_flatpak() {
        let script = format!(
            "STATE_HOME=\"${{XDG_STATE_HOME:-$HOME/.local/state}}\"; rm -f \"$STATE_HOME/{app_dir}/{marker}\"",
            app_dir = config::APP_DIR_NAME,
            marker = config::HOST_SESSION_MARKER_NAME,
        );
        run_host_shell(&script);
        return;
    }

    let _ = std::fs::remove_file(config::host_session_marker_path());
}

fn run_user_systemctl(args: &[&str]) {
    let status = if inside_flatpak() {
        Command::new("flatpak-spawn")
            .args(["--host", "systemctl", "--user"])
            .args(args)
            .status()
    } else {
        Command::new("systemctl").arg("--user").args(args).status()
    };
    let _ = status;
}

fn run_host_shell(script: &str) {
    let status = if inside_flatpak() {
        Command::new("flatpak-spawn")
            .args(["--host", "sh", "-lc", script])
            .status()
    } else {
        Command::new("sh").args(["-lc", script]).status()
    };
    let _ = status;
}

fn inside_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn register_resources() {
    let bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/saywrite.gresource"
    )));
    let resource = gio::Resource::from_data(&bytes).expect("resource bundle");
    gio::resources_register(&resource);
}
