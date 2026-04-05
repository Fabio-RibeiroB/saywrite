use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk::{glib, Align};

use crate::{
    config::{AppSettings, ProviderMode},
    host_integration,
    model_installer,
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
    mode_row.set_selected(if settings.borrow().provider_mode == ProviderMode::Cloud { 1 } else { 0 });
    {
        let settings = settings.clone();
        mode_row.connect_selected_notify(move |row| {
            let mut state = settings.borrow_mut();
            state.provider_mode = if row.selected() == 1 {
                ProviderMode::Cloud
            } else {
                ProviderMode::Local
            };
            let _ = state.save();
        });
    }
    mode_group.add(&mode_row);

    let local_group = adw::PreferencesGroup::builder().title("Local Model").build();
    let model_row = adw::EntryRow::builder()
        .title("Model file path")
        .text(
            settings
                .borrow()
                .local_model_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        )
        .build();
    {
        let settings = settings.clone();
        model_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            let value = row.text().trim().to_string();
            state.local_model_path = (!value.is_empty()).then(|| value.into());
            let _ = state.save();
        });
    }
    local_group.add(&model_row);

    let install_row = adw::ActionRow::builder()
        .title("Default model")
        .subtitle(if model_installer::model_exists() {
            "ggml-base.en — installed"
        } else {
            "ggml-base.en — not installed (~142 MB)"
        })
        .build();

    let install_btn = gtk::Button::with_label(if model_installer::model_exists() {
        "Installed"
    } else {
        "Download"
    });
    install_btn.set_valign(Align::Center);
    if model_installer::model_exists() {
        install_btn.set_sensitive(false);
    } else {
        install_btn.add_css_class("suggested-action");
    }

    {
        let install_btn = install_btn.clone();
        let install_row = install_row.clone();
        let settings = settings.clone();
        install_btn.clone().connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Downloading\u{2026}");

            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            thread::spawn(move || {
                let result = model_installer::download_default_model(|progress| {
                    let msg = match progress.total_bytes {
                        Some(total) => format!(
                            "{} / {}",
                            model_installer::format_bytes(progress.bytes_downloaded),
                            model_installer::format_bytes(total),
                        ),
                        None => model_installer::format_bytes(progress.bytes_downloaded),
                    };
                    // Progress updates are frequent; we don't need every one
                    drop(msg);
                });
                match result {
                    Ok(_) => { let _ = tx.send(Ok("done".into())); }
                    Err(e) => { let _ = tx.send(Err(e.to_string())); }
                }
            });

            let btn = btn.clone();
            let install_row = install_row.clone();
            let settings = settings.clone();
            glib::timeout_add_local(Duration::from_millis(200), move || match rx.try_recv() {
                Ok(Ok(_)) => {
                    btn.set_label("Installed");
                    btn.remove_css_class("suggested-action");
                    install_row.set_subtitle("ggml-base.en — installed");
                    let mut state = settings.borrow_mut();
                    state.local_model_path = Some(crate::config::default_model_path());
                    let _ = state.save();
                    glib::ControlFlow::Break
                }
                Ok(Err(err)) => {
                    btn.set_label("Retry");
                    btn.set_sensitive(true);
                    install_row.set_subtitle(&format!("Download failed: {err}"));
                    model_installer::cleanup_partial();
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    btn.set_label("Retry");
                    btn.set_sensitive(true);
                    model_installer::cleanup_partial();
                    glib::ControlFlow::Break
                }
            });
        });
    }

    install_row.add_suffix(&install_btn);
    local_group.add(&install_row);

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
        .description("Start and stop dictation from anywhere with a single shortcut.")
        .build();

    let shortcut_row = adw::ActionRow::builder()
        .title("Global shortcut")
        .subtitle("Press this key combination to toggle dictation.")
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
        .title("How it works")
        .subtitle("SayWrite listens for this shortcut system-wide and starts recording immediately.")
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
        .title("Text delivery")
        .subtitle(if host_integration::host_available() {
            "Host service connected — text will be typed into the focused app."
        } else {
            "Host service not running — text will be copied to the clipboard."
        })
        .build();
    note_group.add(&row);
    page.add(&note_group);
    page
}

fn build_probe_group(title: &str, probe: &RuntimeProbe) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(title).build();

    for (label, value) in [
        ("Provider", probe.provider_label.clone()),
        ("Dictation", probe.dictation_label.clone()),
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
