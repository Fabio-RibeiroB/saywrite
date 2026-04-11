use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk::{glib, Align};

use crate::{
    config::{AppSettings, ModelSize, ProviderMode},
    dictation, host_integration, model_installer,
    runtime::probe_runtime,
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

pub fn build_inline_page<F>(settings: Rc<RefCell<AppSettings>>, on_back: F) -> gtk::Widget
where
    F: Fn() + 'static,
{
    let header = adw::HeaderBar::new();
    let back_btn = gtk::Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text("Back")
        .build();
    back_btn.add_css_class("flat");
    back_btn.connect_clicked(move |_| on_back());
    header.pack_start(&back_btn);
    header.set_title_widget(Some(&adw::WindowTitle::builder().title("Settings").build()));

    // Single preferences page — all groups live inside it. adw::PreferencesPage
    // handles its own scrolling, so no external ScrolledWindow needed.
    let page = adw::PreferencesPage::new();

    let pending_save: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let save_toast = SaveToast::new();

    let toast_group = adw::PreferencesGroup::new();
    toast_group.add(&save_toast.widget());
    page.add(&toast_group);

    let host_status = host_integration::host_status();
    let host_setup = crate::host_setup::host_setup_status();
    let direct_typing_active = host_status
        .as_ref()
        .map(|status| status.insertion_capability == crate::host_api::INSERTION_CAPABILITY_TYPING)
        .unwrap_or(false);

    let mode_group = adw::PreferencesGroup::builder()
        .title("Output mode")
        .description("Clipboard Mode copies text. Direct Typing needs the host companion outside the sandbox.")
        .build();

    let mode_status_row = adw::ActionRow::builder()
        .title("Current mode")
        .subtitle(if direct_typing_active {
            "Direct Typing"
        } else {
            "Clipboard Mode"
        })
        .build();
    mode_group.add(&mode_status_row);

    let host_row = adw::ActionRow::builder()
        .title("Host companion")
        .subtitle(
            host_status
                .as_ref()
                .map(|status| status.status.as_str())
                .unwrap_or("Not running"),
        )
        .build();
    mode_group.add(&host_row);

    if let Some(status) = host_status.as_ref() {
        let path_row = adw::ActionRow::builder()
            .title("Delivery path")
            .subtitle(format!(
                "{} via {}",
                crate::host_api::insertion_capability_label(&status.insertion_capability),
                status.insertion_backend
            ))
            .build();
        mode_group.add(&path_row);
    } else if crate::host_setup::can_install_in_app() {
        let install_row = adw::ActionRow::builder()
            .title("Turn on Direct Typing")
            .subtitle("Install the host companion to type directly into apps.")
            .build();

        let install_progress_row = adw::ActionRow::builder()
            .title("Installing…")
            .subtitle("Starting")
            .build();
        install_progress_row.set_visible(false);

        let install_btn = gtk::Button::with_label("Install");
        install_btn.set_valign(Align::Center);
        install_btn.add_css_class("suggested-action");

        {
            let install_row = install_row.clone();
            let install_progress_row = install_progress_row.clone();
            install_btn.connect_clicked(move |btn| {
                btn.set_sensitive(false);
                btn.set_label("Installing…");
                install_progress_row.set_visible(true);
                install_progress_row.set_subtitle("Starting…");
                install_row.set_subtitle("Installation in progress…");

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
                            install_row.set_subtitle("Host companion installed. Reopen Settings to confirm.");
                            install_progress_row.set_title("Done");
                            install_progress_row.set_subtitle("saywrite-host is running.");
                            glib::ControlFlow::Break
                        }
                        Err(msg) => {
                            btn.set_label("Retry");
                            btn.set_sensitive(true);
                            btn.add_css_class("suggested-action");
                            install_row.set_subtitle("Installation failed.");
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
        mode_group.add(&install_row);
        mode_group.add(&install_progress_row);
    } else {
        let manual_row = adw::ActionRow::builder()
            .title("Manual setup")
            .subtitle("Install the host companion package outside the Flatpak to enable Direct Typing.")
            .build();
        mode_group.add(&manual_row);
    }

    if host_setup.binary_installed {
        let install_state_row = adw::ActionRow::builder()
            .title("Installation")
            .subtitle(match (host_setup.systemd_service_installed, host_setup.dbus_service_installed) {
                (true, true) => "Fully installed",
                _ => "Partially installed — service files incomplete",
            })
            .build();
        mode_group.add(&install_state_row);
    }

    page.add(&mode_group);

    // ── Transcription ─────────────────────────────────────────────────────────
    let transcription_group = adw::PreferencesGroup::builder()
        .title("Transcription")
        .build();

    let mode_row = adw::ComboRow::builder()
        .title("Engine")
        .subtitle("Local runs entirely on your machine. Cloud uses an external API.")
        .build();
    mode_row.set_model(Some(&gtk::StringList::new(&["Local", "Cloud"])));
    mode_row.set_selected(
        if settings.borrow().provider_mode == ProviderMode::Cloud { 1 } else { 0 },
    );
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
    transcription_group.add(&mode_row);

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
        .title("Microphone")
        .subtitle("Input device used for dictation recording")
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
    transcription_group.add(&mic_row);

    let pause_audio_row = adw::SwitchRow::builder()
        .title("Pause audio while recording")
        .subtitle("Mutes all playback when dictation starts")
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
    transcription_group.add(&pause_audio_row);

    page.add(&transcription_group);

    // ── Local Model ───────────────────────────────────────────────────────────
    let local_group = adw::PreferencesGroup::builder()
        .title("Local Model")
        .description("Required for Local engine. Not used in Cloud mode.")
        .build();

    let size_row = adw::ComboRow::builder()
        .title("Model size")
        .subtitle("Larger models are more accurate but slower to load")
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

    let install_row = adw::ActionRow::builder()
        .title("Model file")
        .subtitle(if installed {
            format!("{} — installed", current_size.filename())
        } else {
            format!("{} — not downloaded yet", current_size.filename())
        })
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
                    Ok(_) => { let _ = tx.send(Ok(DownloadState::Done)); }
                    Err(e) => { let _ = tx.send(Err(e.to_string())); }
                }
            });

            let btn = btn.clone();
            let install_row = install_row.clone();
            let progress_row = progress_row.clone();
            let progress_bar = progress_bar.clone();
            let settings = settings.clone();
            let btn_dc = btn.clone();
            let install_row_dc = install_row.clone();
            let progress_row_dc = progress_row.clone();
            let progress_bar_dc = progress_bar.clone();
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
                            state.local_model_path =
                                Some(crate::config::model_path_for_size(size));
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
                    btn_dc.set_label("Retry");
                    btn_dc.set_sensitive(true);
                    install_row_dc.set_subtitle("Download interrupted");
                    progress_row_dc.set_subtitle("Download stopped unexpectedly.");
                    progress_bar_dc.set_fraction(0.0);
                    model_installer::cleanup_partial_for_size(size);
                    glib::ControlFlow::Break
                },
            );
        });
    }

    install_row.add_suffix(&install_btn);
    local_group.add(&install_row);
    local_group.add(&progress_row);

    let model_row = adw::EntryRow::builder()
        .title("Custom model path")
        .text(
            settings
                .borrow()
                .local_model_path
                .as_ref()
                .map(|p| p.display().to_string())
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

    page.add(&local_group);

    // ── Cloud API ─────────────────────────────────────────────────────────────
    let cloud_group = adw::PreferencesGroup::builder()
        .title("Cloud API")
        .description("Only used when engine is set to Cloud.")
        .build();

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

    page.add(&cloud_group);

    // ── Shortcut ──────────────────────────────────────────────────────────────
    let shortcut_group = adw::PreferencesGroup::builder()
        .title("Shortcut")
        .description("Press this key combination anywhere to start or stop dictation.")
        .build();

    let shortcut_row = adw::ActionRow::builder()
        .title("Global shortcut")
        .build();
    let shortcut_label = gtk::Label::builder()
        .label(&settings.borrow().global_shortcut_label)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    shortcut_label.add_css_class("shortcut-pill");
    shortcut_row.add_suffix(&shortcut_label);
    shortcut_row.set_activatable(false);
    shortcut_group.add(&shortcut_row);

    page.add(&shortcut_group);

    // ── Diagnostics ───────────────────────────────────────────────────────────
    let probe = probe_runtime(&settings.borrow());
    let diag_group = adw::PreferencesGroup::builder()
        .title("Diagnostics")
        .build();

    for (label, value) in [
        ("Provider", probe.provider_label.clone()),
        ("Dictation", probe.dictation_label.clone()),
        ("Acceleration", probe.acceleration_label.clone()),
        (
            "whisper.cpp",
            if probe.whisper_cli_found {
                format!("Found at {}", probe.whisper_cli_display)
            } else {
                "Not found".into()
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
        diag_group.add(&row);
    }

    page.add(&diag_group);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));
    toolbar.upcast()
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
            if let Some(toast) = save_toast.as_ref() {
                toast.show("Settings saved");
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
