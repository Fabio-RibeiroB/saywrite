use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk::{glib, Align};

use crate::{
    config::{AppSettings, ModelSize, ProviderMode},
    dictation, host_integration, model_installer,
    runtime::{probe_runtime, RuntimeProbe},
    ui::async_poll,
};

const SETTINGS_SAVE_DEBOUNCE_MS: u64 = 300;

#[derive(Clone)]
struct SaveToast {
    revealer: gtk::Revealer,
    label: gtk::Label,
    pending_hide: Rc<RefCell<Option<glib::SourceId>>>,
}

impl SaveToast {
    fn new() -> Self {
        let label = gtk::Label::new(Some("Settings saved"));
        label.add_css_class("notification-toast");

        let revealer = gtk::Revealer::new();
        revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        revealer.set_transition_duration(200);
        revealer.set_reveal_child(false);
        revealer.set_child(Some(&label));

        Self {
            revealer,
            label,
            pending_hide: Rc::new(RefCell::new(None)),
        }
    }

    fn widget(&self) -> gtk::Revealer {
        self.revealer.clone()
    }

    fn show(&self, message: &str) {
        self.label.set_label(message);
        self.revealer.set_reveal_child(true);
        if let Some(source_id) = self.pending_hide.borrow_mut().take() {
            source_id.remove();
        }
        let revealer = self.revealer.clone();
        let pending_hide = self.pending_hide.clone();
        let source_id = glib::timeout_add_local(Duration::from_secs(2), move || {
            revealer.set_reveal_child(false);
            pending_hide.borrow_mut().take();
            glib::ControlFlow::Break
        });
        *self.pending_hide.borrow_mut() = Some(source_id);
    }
}

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
    let save_toast = SaveToast::new();
    let page = adw::PreferencesPage::builder()
        .title("Engine")
        .icon_name("preferences-system-symbolic")
        .build();

    let toast_group = adw::PreferencesGroup::new();
    toast_group.add(&save_toast.widget());
    page.add(&toast_group);

    let mode_group = adw::PreferencesGroup::builder()
        .title("Transcription Mode")
        .build();

    let mode_row = adw::ComboRow::builder()
        .title("Use")
        .subtitle("Choose the default dictation engine")
        .build();
    mode_row.set_model(Some(&gtk::StringList::new(&["Local", "Cloud"])));
    mode_row.set_selected(if settings.borrow().provider_mode == ProviderMode::Cloud {
        1
    } else {
        0
    });
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

    let local_group = adw::PreferencesGroup::builder()
        .title("Local Model")
        .build();
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
        let save_toast = save_toast.clone();
        model_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            let value = row.text().trim().to_string();
            state.local_model_path = (!value.is_empty()).then(|| value.into());
            drop(state);
            schedule_settings_save(&settings, &pending_save, Some(&save_toast));
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

    let install_btn = gtk::Button::with_label(if installed { "Installed" } else { "Download" });
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
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                    }
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
                            progress_row.set_subtitle(
                                "Download failed. Check your connection and try again.",
                            );
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
        let save_toast = save_toast.clone();
        base_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            state.cloud_api_base = row.text().to_string();
            drop(state);
            schedule_settings_save(&settings, &pending_save, Some(&save_toast));
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
        let save_toast = save_toast.clone();
        key_row.connect_changed(move |row| {
            let mut state = settings.borrow_mut();
            state.cloud_api_key = row.text().to_string();
            drop(state);
            schedule_settings_save(&settings, &pending_save, Some(&save_toast));
        });
    }
    cloud_group.add(&key_row);

    let mic_group = adw::PreferencesGroup::builder()
        .title("Microphone")
        .description("Choose which input device SayWrite should record from.")
        .build();

    let devices = dictation::list_input_devices();
    let mut device_labels = vec!["System default".to_string()];
    device_labels.extend(devices.iter().map(|d| d.label.clone()));
    let selected_device = settings.borrow().input_device_name.clone();
    let selected_index = selected_device
        .as_ref()
        .and_then(|id| devices.iter().position(|d| d.id == *id))
        .map(|i| i + 1)
        .unwrap_or(0) as u32;

    let mic_row = adw::ComboRow::builder()
        .title("Input device")
        .subtitle("Used for dictation recording")
        .build();
    let labels: Vec<&str> = device_labels.iter().map(|s| s.as_str()).collect();
    mic_row.set_model(Some(&gtk::StringList::new(&labels)));
    mic_row.set_selected(selected_index);
    {
        let settings = settings.clone();
        mic_row.connect_selected_notify(move |row| {
            let mut state = settings.borrow_mut();
            let idx = row.selected() as usize;
            state.input_device_name = if idx == 0 {
                None
            } else {
                devices.get(idx - 1).map(|d| d.id.clone())
            };
            let _ = state.save();
        });
    }
    mic_group.add(&mic_row);

    page.add(&mode_group);
    page.add(&mic_group);
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
        .subtitle(
            "SayWrite listens for this shortcut system-wide and starts recording immediately.",
        )
        .build();
    note_row.set_activatable(false);

    group.add(&shortcut_row);
    group.add(&note_row);

    let behaviour_group = adw::PreferencesGroup::builder()
        .title("During dictation")
        .build();

    let pause_audio_row = adw::SwitchRow::builder()
        .title("Pause PC audio while recording")
        .subtitle("Mutes all playback when dictation starts and restores it when you stop.")
        .build();
    pause_audio_row.set_active(settings.borrow().pause_audio_during_dictation);
    {
        let settings = settings.clone();
        pause_audio_row.connect_active_notify(move |row| {
            let mut state = settings.borrow_mut();
            state.pause_audio_during_dictation = row.is_active();
            let _ = state.save();
        });
    }
    behaviour_group.add(&pause_audio_row);

    page.add(&group);
    page.add(&behaviour_group);
    page
}

fn build_diagnostics_page(settings: Rc<RefCell<AppSettings>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Diagnostics")
        .icon_name("utilities-system-monitor-symbolic")
        .build();
    let probe = probe_runtime(&settings.borrow());
    let host_status = host_integration::host_status();
    let host_setup = crate::host_setup::host_setup_status();

    page.add(&build_probe_group("Runtime", &probe));

    let note_group = adw::PreferencesGroup::builder().title("Status").build();
    let row = adw::ActionRow::builder()
        .title("Host companion")
        .subtitle(
            host_status
                .as_ref()
                .map(|status| status.status.as_str())
                .unwrap_or(
                    "Not running — global hotkey dictation is unavailable until the host companion is installed.",
                ),
        )
        .build();
    note_group.add(&row);

    let install_state_row = adw::ActionRow::builder()
        .title("Host installation")
        .subtitle(
            match (
                host_setup.binary_installed,
                host_setup.systemd_service_installed,
                host_setup.dbus_service_installed,
            ) {
                (true, true, true) => "Installed in your user account",
                (true, _, _) => "Partially installed — service files are incomplete",
                _ => "Not installed yet",
            },
        )
        .build();
    note_group.add(&install_state_row);

    let running_state_row = adw::ActionRow::builder()
        .title("Host daemon")
        .subtitle(if host_setup.host_running {
            "Running now"
        } else if host_setup.binary_installed {
            "Installed, but not reachable from the app"
        } else {
            "Not running"
        })
        .build();
    note_group.add(&running_state_row);

    if let Some(status) = host_status.as_ref() {
        let insertion_row = adw::ActionRow::builder()
            .title("Insertion mode")
            .subtitle(format!(
                "{} via {}",
                crate::host_api::insertion_capability_label(&status.insertion_capability),
                status.insertion_backend
            ))
            .build();
        note_group.add(&insertion_row);

        let shortcut_row = adw::ActionRow::builder()
            .title("Global shortcut")
            .subtitle(if status.hotkey_active {
                "Active"
            } else {
                "Configured, but not active yet"
            })
            .build();
        note_group.add(&shortcut_row);
    } else if crate::host_setup::can_install_in_app() {
        // Source repo is available — offer one-click install.
        let install_row = adw::ActionRow::builder()
            .title("Direct Typing Mode")
            .subtitle(
                "Install the host companion to enable hotkey dictation and direct text insertion.",
            )
            .build();

        let install_progress_row = adw::ActionRow::builder()
            .title("Installing\u{2026}")
            .subtitle("Starting")
            .build();
        install_progress_row.set_visible(false);

        let install_btn = gtk::Button::with_label("Enable Direct Typing");
        install_btn.set_valign(Align::Center);
        install_btn.add_css_class("suggested-action");

        {
            let install_btn = install_btn.clone();
            let install_row = install_row.clone();
            let install_progress_row = install_progress_row.clone();
            install_btn.connect_clicked(move |btn| {
                btn.set_sensitive(false);
                btn.set_label("Installing\u{2026}");
                install_progress_row.set_visible(true);
                install_progress_row.set_subtitle("Starting\u{2026}");
                install_row.set_subtitle("Installation in progress\u{2026}");

                let rx = crate::host_setup::install_host_companion();

                let btn = btn.clone();
                let install_row = install_row.clone();
                let install_progress_row = install_progress_row.clone();
                let btn_dc = btn.clone();
                let install_row_dc = install_row.clone();
                let install_progress_row_dc = install_progress_row.clone();
                async_poll::poll_receiver(
                    rx,
                    Duration::from_millis(200),
                    move |result| match result {
                        Ok(crate::host_setup::HostInstallUpdate::Progress(msg)) => {
                            install_progress_row.set_subtitle(&msg);
                            glib::ControlFlow::Continue
                        }
                        Ok(crate::host_setup::HostInstallUpdate::Done) => {
                            btn.set_label("Installed");
                            btn.remove_css_class("suggested-action");
                            install_row.set_subtitle(
                                "Host companion installed. Reopen Settings to confirm status.",
                            );
                            install_progress_row.set_title("Done");
                            install_progress_row
                                .set_subtitle("saywrite-host is running as a user service.");
                            glib::ControlFlow::Break
                        }
                        Err(msg) => {
                            btn.set_label("Retry");
                            btn.set_sensitive(true);
                            btn.add_css_class("suggested-action");
                            install_row.set_subtitle("Installation failed — see details below.");
                            install_progress_row.set_title("Failed");
                            install_progress_row.set_subtitle(&msg);
                            glib::ControlFlow::Break
                        }
                    },
                    move || {
                        btn_dc.set_label("Retry");
                        btn_dc.set_sensitive(true);
                        btn_dc.add_css_class("suggested-action");
                        install_row_dc.set_subtitle("Installation stopped unexpectedly.");
                        install_progress_row_dc.set_title("Stopped");
                        install_progress_row_dc
                            .set_subtitle("The install process exited without completing.");
                        glib::ControlFlow::Break
                    },
                );
            });
        }

        install_row.add_suffix(&install_btn);
        note_group.add(&install_row);
        note_group.add(&install_progress_row);
    } else {
        // No source repo available (packaged/Flatpak install) — show manual
        // install guidance instead of a button that would always fail.
        let instructions = crate::host_setup::host_install_instructions();
        let manual_row = adw::ActionRow::builder()
            .title("Direct Typing Mode")
            .subtitle(&instructions)
            .build();
        note_group.add(&manual_row);
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
        (
            "whisper.cpp",
            if probe.whisper_cli_found {
                format!("Found at {}", probe.whisper_cli_display)
            } else {
                "Not found yet".into()
            },
        ),
        (
            "Model",
            if probe.local_model_present {
                probe.local_model_display.clone()
            } else {
                "No local model downloaded yet".into()
            },
        ),
        ("Insertion", probe.insertion_label.clone()),
    ] {
        let row = adw::ActionRow::builder()
            .title(label)
            .subtitle(&value)
            .build();
        group.add(&row);
    }

    group
}

fn schedule_settings_save(
    settings: &Rc<RefCell<AppSettings>>,
    pending_save: &Rc<RefCell<Option<glib::SourceId>>>,
    save_toast: Option<&SaveToast>,
) {
    if let Some(source_id) = pending_save.borrow_mut().take() {
        source_id.remove();
    }

    let settings = settings.clone();
    let pending_save_for_closure = pending_save.clone();
    let save_toast = save_toast.cloned();
    let source_id = glib::timeout_add_local(
        Duration::from_millis(SETTINGS_SAVE_DEBOUNCE_MS),
        move || {
            let _ = settings.borrow().save();
            if let Some(save_toast) = save_toast.as_ref() {
                save_toast.show("Settings saved");
            }
            pending_save_for_closure.borrow_mut().take();
            glib::ControlFlow::Break
        },
    );
    *pending_save.borrow_mut() = Some(source_id);
}

enum DownloadState {
    Progress {
        fraction: Option<f64>,
        label: String,
    },
    Done,
}
