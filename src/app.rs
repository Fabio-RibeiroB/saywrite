use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, thread};

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::{config::AppSettings, host_integration, host_setup, ui};

pub const APP_ID: &str = "io.github.fabio.SayWrite";

pub fn run() -> glib::ExitCode {
    register_resources();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    app.connect_startup(|_| {
        load_css();
        start_native_integration();
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

fn start_native_integration() {
    host_setup::cleanup_legacy_host_companion();
    host_integration::start_background_integration();
    thread::spawn(|| {
        let label = AppSettings::load().global_shortcut_label;
        host_setup::self_heal_gnome_shortcut(&label);
    });
}

fn register_resources() {
    let bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/saywrite.gresource"
    )));
    let resource = gio::Resource::from_data(&bytes).expect("resource bundle");
    gio::resources_register(&resource);
}
