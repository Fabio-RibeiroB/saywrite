use libadwaita as adw;
use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::Align;

use crate::{
    config::AppSettings,
    runtime::{probe_runtime, RuntimeProbe},
};

pub fn present(parent: &adw::ApplicationWindow, settings: Rc<RefCell<AppSettings>>) {
    let prefs = adw::PreferencesWindow::builder()
        .transient_for(parent)
        .modal(true)
        .search_enabled(false)
        .title("SayWrite Settings")
        .default_width(720)
        .default_height(560)
        .build();

    prefs.add(&build_engine_page(settings.clone()));
    prefs.add(&build_shortcut_page(settings.clone()));
    prefs.add(&build_diagnostics_page(settings));
    prefs.present();
}

fn build_engine_page(settings: Rc<RefCell<AppSettings>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Engine")
        .icon_name("preferences-system-symbolic")
        .build();

    let mode_group = adw::PreferencesGroup::builder()
        .title("Transcription Mode")
        .build();

    let mode_row = adw::ComboRow::builder()
        .title("Use")
        .subtitle("Choose the default dictation engine")
        .build();
    mode_row.set_model(Some(&gtk::StringList::new(&["Local", "Cloud"])));
    mode_row.set_selected(if settings.borrow().provider_mode == "cloud" { 1 } else { 0 });
    {
        let settings = settings.clone();
        mode_row.connect_selected_notify(move |row| {
            let mut state = settings.borrow_mut();
            state.provider_mode = if row.selected() == 1 { "cloud".into() } else { "local".into() };
            let _ = state.save();
        });
    }
    mode_group.add(&mode_row);

    let local_group = adw::PreferencesGroup::builder().title("Local Model").build();
    let model_row = adw::EntryRow::builder()
        .title("Model file path")
        .text(&settings.borrow().local_model_path)
        .build();
    {
        let settings = settings.clone();
        model_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            state.local_model_path = row.text().to_string();
            let _ = state.save();
        });
    }
    local_group.add(&model_row);

    let cloud_group = adw::PreferencesGroup::builder().title("Cloud API").build();
    let base_row = adw::EntryRow::builder()
        .title("API base URL")
        .text(&settings.borrow().cloud_api_base)
        .build();
    {
        let settings = settings.clone();
        base_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            state.cloud_api_base = row.text().to_string();
            let _ = state.save();
        });
    }
    cloud_group.add(&base_row);

    let key_row = adw::PasswordEntryRow::builder()
        .title("API key")
        .text(&settings.borrow().cloud_api_key)
        .build();
    {
        let settings = settings.clone();
        key_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            state.cloud_api_key = row.text().to_string();
            let _ = state.save();
        });
    }
    cloud_group.add(&key_row);

    page.add(&mode_group);
    page.add(&local_group);
    page.add(&cloud_group);
    page
}

fn build_shortcut_page(settings: Rc<RefCell<AppSettings>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Shortcut")
        .icon_name("input-keyboard-symbolic")
        .build();

    let group = adw::PreferencesGroup::builder()
        .title("Hands-free activation")
        .description("SayWrite now targets one obvious global action instead of exposing multiple conflicting flows.")
        .build();

    let shortcut_row = adw::ActionRow::builder()
        .title("Current default")
        .subtitle("GNOME shortcut installer scripts still own the actual system binding for now.")
        .build();
    let shortcut_label = gtk::Label::builder()
        .label(&settings.borrow().global_shortcut_label)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    shortcut_label.add_css_class("shortcut-pill");
    shortcut_row.add_suffix(&shortcut_label);
    shortcut_row.set_activatable(false);

    let note_row = adw::ActionRow::builder()
        .title("Long-term direction")
        .subtitle("Replace desktop-specific launch shortcuts with a real Rust host daemon that owns activation, recording, and insertion.")
        .build();
    note_row.set_activatable(false);

    group.add(&shortcut_row);
    group.add(&note_row);
    page.add(&group);
    page
}

fn build_diagnostics_page(settings: Rc<RefCell<AppSettings>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Diagnostics")
        .icon_name("utilities-system-monitor-symbolic")
        .build();
    let probe = probe_runtime(&settings.borrow());

    page.add(&build_probe_group("Runtime", &probe));

    let note_group = adw::PreferencesGroup::builder().title("Status").build();
    let row = adw::ActionRow::builder()
        .title("Migration phase")
        .subtitle("The Rust shell is the new app surface. Python backend pieces remain in-tree until the service and insertion layers are migrated.")
        .build();
    note_group.add(&row);
    page.add(&note_group);
    page
}

fn build_probe_group(title: &str, probe: &RuntimeProbe) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(title).build();

    for (label, value) in [
        ("Acceleration", probe.acceleration_label.clone()),
        ("whisper.cpp", if probe.whisper_cli_found { format!("Found at {}", probe.whisper_cli_display) } else { "Not found yet".into() }),
        ("Model", if probe.local_model_present { probe.local_model_display.clone() } else { "No local model downloaded yet".into() }),
        ("Insertion", probe.insertion_label.clone()),
    ] {
        let row = adw::ActionRow::builder().title(label).subtitle(&value).build();
        group.add(&row);
    }

    group
}
