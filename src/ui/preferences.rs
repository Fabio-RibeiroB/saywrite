use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk::{glib, Align};

use crate::{
    config::{AppSettings, ModelSize, ProviderMode},
    host_integration,
    model_installer,
    runtime::{probe_runtime, RuntimeProbe},
    ui::async_poll,
};

const SETTINGS_SAVE_DEBOUNCE_MS: u64 = 300;

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
    let pending_save: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
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
        let pending_save = pending_save.clone();
        model_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            let value = row.text().trim().to_string();
            state.local_model_path = (!value.is_empty()).then(|| value.into());
            drop(state);
            schedule_settings_save(&settings, &pending_save);
        });
    }
    local_group.add(&model_row);

    let size_row = adw::ComboRow::builder()
        .title("Model size")
        .subtitle("Larger models are more accurate but slower")
        .build();
    size_row.set_model(Some(&gtk::StringList::new(&[
        ModelSize::Tiny.label(),
        ModelSize::Base.label(),
        ModelSize::Small.label(),
    ])));
    size_row.set_selected(settings.borrow().model_size.to_index());
    {
        let settings = settings.clone();
        size_row.connect_selected_notify(move |row| {
            let mut state = settings.borrow_mut();
            state.model_size = ModelSize::from_index(row.selected());
            let _ = state.save();
        });
    }
    local_group.add(&size_row);

    let current_size = settings.borrow().model_size;
    let installed = model_installer::model_exists_for_size(current_size);

    let install_subtitle = if installed {
        format!("{} — installed", current_size.filename())
    } else {
        format!("{} — not installed", current_size.filename())
    };
    let install_row = adw::ActionRow::builder()
        .title("Download model")
        .subtitle(&install_subtitle)
        .build();

    let progress_row = adw::ActionRow::builder()
        .title("Download progress")
        .subtitle("Waiting to start")
        .build();
    progress_row.set_visible(false);

    let progress_bar = gtk::ProgressBar::new();
    progress_bar.set_valign(Align::Center);
    progress_bar.set_hexpand(true);
    progress_bar.set_width_request(180);
    progress_row.add_suffix(&progress_bar);

    let install_btn = gtk::Button::with_label(if installed {
        "Installed"
    } else {
        "Download"
    });
    install_btn.set_valign(Align::Center);
    if installed {
        install_btn.set_sensitive(false);
    } else {
        install_btn.add_css_class("suggested-action");
    }

    {
        let install_btn = install_btn.clone();
        let install_row = install_row.clone();
        let progress_row = progress_row.clone();
        let progress_bar = progress_bar.clone();
        let settings = settings.clone();
        let size_row = size_row.clone();
        install_btn.clone().connect_clicked(move |btn| {
            let size = ModelSize::from_index(size_row.selected());
            btn.set_sensitive(false);
            btn.set_label("Downloading\u{2026}");
            progress_row.set_visible(true);
            progress_row.set_subtitle("Starting download\u{2026}");
            progress_bar.set_fraction(0.0);

            let (tx, rx) = mpsc::channel::<Result<DownloadState, String>>();
            thread::spawn(move || {
                let result = model_installer::download_model(size, |progress| {
                    let fraction = progress
                        .total_bytes
                        .map(|total| progress.bytes_downloaded as f64 / total as f64);
                    let label = match progress.total_bytes {
                        Some(total) => format!(
                            "{} / {}",
                            model_installer::format_bytes(progress.bytes_downloaded),
                            model_installer::format_bytes(total),
                        ),
                        None => model_installer::format_bytes(progress.bytes_downloaded),
                    };
                    let _ = tx.send(Ok(DownloadState::Progress { fraction, label }));
                });
                match result {
                    Ok(_) => {
                        let _ = tx.send(Ok(DownloadState::Done));
                    }
                    Err(e) => { let _ = tx.send(Err(e.to_string())); }
                }
            });

            let btn = btn.clone();
            let install_row = install_row.clone();
            let progress_row = progress_row.clone();
            let progress_bar = progress_bar.clone();
            let settings = settings.clone();
            let btn_on_disconnect = btn.clone();
            let install_row_on_disconnect = install_row.clone();
            let progress_row_on_disconnect = progress_row.clone();
            let progress_bar_on_disconnect = progress_bar.clone();
            async_poll::poll_receiver(
                rx,
                Duration::from_millis(200),
                move |result| {
                    match result {
                        Ok(DownloadState::Progress { fraction, label }) => {
                            progress_row.set_subtitle(&label);
                            if let Some(value) = fraction {
                                progress_bar.set_fraction(value);
                            } else {
                                progress_bar.pulse();
                            }
                            return glib::ControlFlow::Continue;
                        }
                        Ok(DownloadState::Done) => {
                            btn.set_label("Installed");
                            btn.remove_css_class("suggested-action");
                            install_row.set_subtitle(&format!("{} — installed", size.filename()));
                            progress_row.set_subtitle("Model ready");
                            progress_bar.set_fraction(1.0);
                            let mut state = settings.borrow_mut();
                            state.local_model_path = Some(crate::config::model_path_for_size(size));
                            state.model_size = size;
                            let _ = state.save();
                        }
                        Err(_) => {
                            btn.set_label("Retry");
                            btn.set_sensitive(true);
                            install_row.set_subtitle("Download failed");
                            progress_row
                                .set_subtitle("Download failed. Check your connection and try again.");
                            progress_bar.set_fraction(0.0);
                            model_installer::cleanup_partial_for_size(size);
                        }
                    }
                    glib::ControlFlow::Break
                },
                move || {
                    btn_on_disconnect.set_label("Retry");
                    btn_on_disconnect.set_sensitive(true);
                    install_row_on_disconnect.set_subtitle("Download interrupted");
                    progress_row_on_disconnect.set_subtitle("Download stopped unexpectedly.");
                    progress_bar_on_disconnect.set_fraction(0.0);
                    model_installer::cleanup_partial_for_size(size);
                    glib::ControlFlow::Break
                },
            );
        });
    }

    install_row.add_suffix(&install_btn);
    local_group.add(&install_row);
    local_group.add(&progress_row);

    let cloud_group = adw::PreferencesGroup::builder().title("Cloud API").build();
    let base_row = adw::EntryRow::builder()
        .title("API base URL")
        .text(&settings.borrow().cloud_api_base)
        .build();
    {
        let settings = settings.clone();
        let pending_save = pending_save.clone();
        base_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            state.cloud_api_base = row.text().to_string();
            drop(state);
            schedule_settings_save(&settings, &pending_save);
        });
    }
    cloud_group.add(&base_row);

    let key_row = adw::PasswordEntryRow::builder()
        .title("API key")
        .text(&settings.borrow().cloud_api_key)
        .build();
    {
        let settings = settings.clone();
        let pending_save = pending_save.clone();
        key_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            state.cloud_api_key = row.text().to_string();
            drop(state);
            schedule_settings_save(&settings, &pending_save);
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
        .title("Host companion")
        .subtitle(if host_integration::host_available() {
            "Connected — hotkey dictation and direct typing into focused apps are available."
        } else {
            "Not running — global hotkey dictation is unavailable until the host companion is installed."
        })
        .build();
    note_group.add(&row);

    if !host_integration::host_available() {
        let install_row = adw::ActionRow::builder()
            .title("Install host companion")
            .subtitle("Enables direct text typing into any focused app")
            .build();
        let install_info_btn = gtk::Button::with_label("How to install");
        install_info_btn.set_valign(Align::Center);
        install_info_btn.add_css_class("flat");
        install_info_btn.connect_clicked(move |btn| {
            let parent_window: Option<gtk::Window> = btn.root().and_then(|r| r.downcast().ok());
            let dialog = adw::MessageDialog::new(
                parent_window.as_ref(),
                Some("Install Host Companion"),
                Some("Build and install the host daemon:\n\n1. cargo build --release\n2. bash scripts/install-host.sh\n\nThis installs saywrite-host to ~/.local/bin and enables the systemd user service."),
            );
            dialog.add_response("ok", "OK");
            dialog.present();
        });
        install_row.add_suffix(&install_info_btn);
        note_group.add(&install_row);
    }

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

fn schedule_settings_save(
    settings: &Rc<RefCell<AppSettings>>,
    pending_save: &Rc<RefCell<Option<glib::SourceId>>>,
) {
    if let Some(source_id) = pending_save.borrow_mut().take() {
        source_id.remove();
    }

    let settings = settings.clone();
    let pending_save_for_closure = pending_save.clone();
    let source_id = glib::timeout_add_local(
        Duration::from_millis(SETTINGS_SAVE_DEBOUNCE_MS),
        move || {
            let _ = settings.borrow().save();
            pending_save_for_closure.borrow_mut().take();
            glib::ControlFlow::Break
        },
    );
    *pending_save.borrow_mut() = Some(source_id);
}

enum DownloadState {
    Progress { fraction: Option<f64>, label: String },
    Done,
}
