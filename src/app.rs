use libadwaita as adw;
use std::{cell::RefCell, path::Path, process::Command, rc::Rc, thread};

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::{config::AppSettings, host_setup, ui};

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
    thread::spawn(|| {
        run_user_systemctl(&["unmask", "saywrite-host.service"]);
        run_user_systemctl(&["start", "saywrite-host.service"]);
        // Self-heal the GNOME hands-free keybinding on every launch: on
        // fresh Flatpak installs nothing else sets the command path, and
        // previous versions could leave the binding field empty after a
        // cancelled capture. Cheap (a couple of gsettings calls) and
        // idempotent when the keybinding is already correct.
        let label = AppSettings::load().global_shortcut_label;
        host_setup::self_heal_gnome_shortcut(&label);
    });
}

fn stop_host_daemon() {
    run_user_systemctl(&["stop", "saywrite-host.service"]);
    run_user_systemctl(&["mask", "saywrite-host.service"]);
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
