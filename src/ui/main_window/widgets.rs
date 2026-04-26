use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk::{gdk, glib, Align, Orientation};

use crate::{
    config::AppSettings,
    integration_api, model_installer,
    native_integration::{self, IntegrationEvent},
    runtime,
    ui::async_poll,
};

use super::state::{MainWindowUi, SetupAction, WaveformBox};

const ASYNC_POLL_INTERVAL: Duration = Duration::from_millis(80);

// ---------------------------------------------------------------------------
// InlineDownloadState — progress messages for in-window model download
// ---------------------------------------------------------------------------

enum InlineDownloadState {
    Progress { label: String },
    Done,
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

pub(super) fn build_header(
    stack: &gtk::Stack,
    settings: Rc<RefCell<AppSettings>>,
    insertion_chip: &gtk::Button,
) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();

    let mode_chip = gtk::Button::new();
    mode_chip.add_css_class("flat");
    mode_chip.add_css_class("mode-chip");
    mode_chip.set_child(Some(&mode_chip_content(&settings.borrow())));
    mode_chip.set_tooltip_text(Some("Open settings"));
    {
        let stack = stack.clone();
        mode_chip.connect_clicked(move |_| {
            stack.set_visible_child_name("settings");
        });
    }
    header.pack_start(&mode_chip);
    header.pack_start(insertion_chip);

    let prefs = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .build();
    prefs.add_css_class("flat");
    prefs.set_tooltip_text(Some("Open settings"));
    {
        let stack = stack.clone();
        prefs.connect_clicked(move |_| {
            stack.set_visible_child_name("settings");
        });
    }
    header.pack_end(&prefs);

    header
}

// ---------------------------------------------------------------------------
// Body
// ---------------------------------------------------------------------------

pub(super) fn build_body(
    window: &adw::ApplicationWindow,
    stack: &gtk::Stack,
    settings: Rc<RefCell<AppSettings>>,
    insertion_chip: gtk::Button,
) -> gtk::Widget {
    let outer = gtk::Box::new(Orientation::Vertical, 0);
    outer.set_valign(Align::Center);
    outer.set_vexpand(true);
    outer.set_margin_top(40);
    outer.set_margin_bottom(48);
    outer.set_margin_start(48);
    outer.set_margin_end(48);

    // Activity area: spinner + waveform, wrapped in a revealer
    let spinner = gtk::Spinner::new();
    spinner.set_size_request(48, 48);
    spinner.set_halign(Align::Center);

    let waveform = WaveformBox::new();

    let activity_stack = gtk::Stack::new();
    activity_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    activity_stack.set_transition_duration(150);
    activity_stack.set_halign(Align::Center);

    let spinner_box = gtk::Box::new(Orientation::Vertical, 0);
    spinner_box.set_halign(Align::Center);
    spinner_box.append(&spinner);
    activity_stack.add_named(&spinner_box, Some("spinner"));
    activity_stack.add_named(&waveform.widget(), Some("waveform"));

    let activity_revealer = gtk::Revealer::new();
    activity_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    activity_revealer.set_transition_duration(200);
    activity_revealer.set_reveal_child(false);
    activity_revealer.set_child(Some(&activity_stack));

    // State label
    let state_label = gtk::Label::new(Some("Press your hotkey to start dictation"));
    state_label.add_css_class("state-label");
    state_label.set_margin_top(12);

    // Setup panel (wrapped in a revealer)
    let setup_panel = gtk::Box::new(Orientation::Vertical, 10);
    setup_panel.add_css_class("setup-panel");
    setup_panel.set_halign(Align::Center);
    setup_panel.set_margin_top(18);
    setup_panel.set_margin_bottom(8);

    let setup_icon = gtk::Image::from_icon_name("dialog-information-symbolic");
    setup_icon.set_pixel_size(24);
    setup_icon.add_css_class("onboarding-icon");
    setup_icon.set_halign(Align::Center);

    let setup_title = gtk::Label::new(None);
    setup_title.add_css_class("title-4");
    setup_title.set_wrap(true);
    setup_title.set_justify(gtk::Justification::Center);
    setup_title.set_halign(Align::Center);

    let setup_detail = gtk::Label::new(None);
    setup_detail.add_css_class("caption");
    setup_detail.set_wrap(true);
    setup_detail.set_justify(gtk::Justification::Center);
    setup_detail.set_halign(Align::Center);
    setup_detail.set_max_width_chars(48);

    let setup_download_btn = gtk::Button::with_label("Download");
    setup_download_btn.add_css_class("pill");
    setup_download_btn.set_halign(Align::Center);
    setup_download_btn.set_visible(false);

    let setup_settings_btn = gtk::Button::with_label("Open Settings");
    setup_settings_btn.add_css_class("pill");
    setup_settings_btn.set_halign(Align::Center);
    setup_settings_btn.set_visible(false);
    {
        let stack = stack.clone();
        setup_settings_btn.connect_clicked(move |_| {
            stack.set_visible_child_name("settings");
        });
    }

    // Inline API key entry
    let setup_api_entry = gtk::Entry::new();
    setup_api_entry.set_placeholder_text(Some("Paste API key here…"));
    setup_api_entry.set_input_purpose(gtk::InputPurpose::Password);
    setup_api_entry.set_visibility(false);

    let setup_api_save_btn = gtk::Button::with_label("Save");
    setup_api_save_btn.add_css_class("pill");
    setup_api_save_btn.add_css_class("suggested-action");

    let setup_api_row = gtk::Box::new(Orientation::Horizontal, 8);
    setup_api_row.set_halign(Align::Center);
    setup_api_row.append(&setup_api_entry);
    setup_api_row.append(&setup_api_save_btn);
    setup_api_row.set_visible(false);

    setup_panel.append(&setup_icon);
    setup_panel.append(&setup_title);
    setup_panel.append(&setup_detail);
    setup_panel.append(&setup_download_btn);
    setup_panel.append(&setup_settings_btn);
    setup_panel.append(&setup_api_row);

    let setup_revealer = gtk::Revealer::new();
    setup_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    setup_revealer.set_transition_duration(200);
    setup_revealer.set_reveal_child(false);
    setup_revealer.set_child(Some(&setup_panel));

    // Transcript area (editable TextView + count labels, wrapped in revealer)
    let transcript_buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);

    let transcript_view = gtk::TextView::with_buffer(&transcript_buffer);
    transcript_view.set_editable(true);
    transcript_view.set_wrap_mode(gtk::WrapMode::Word);
    transcript_view.set_cursor_visible(true);
    transcript_view.add_css_class("transcript-textview");

    let word_count_label = gtk::Label::new(Some("0 words"));
    word_count_label.add_css_class("caption");
    word_count_label.set_xalign(0.0);

    let char_count_label = gtk::Label::new(Some("0 chars"));
    char_count_label.add_css_class("caption");
    char_count_label.set_xalign(1.0);
    char_count_label.set_hexpand(true);

    let count_row = gtk::Box::new(Orientation::Horizontal, 0);
    count_row.append(&word_count_label);
    count_row.append(&char_count_label);
    count_row.set_margin_top(6);

    let transcript_bubble = gtk::Box::new(Orientation::Vertical, 0);
    transcript_bubble.add_css_class("transcript-bubble");
    transcript_bubble.append(&transcript_view);
    transcript_bubble.append(&count_row);
    transcript_bubble.set_margin_top(32);

    let transcript_revealer = gtk::Revealer::new();
    transcript_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    transcript_revealer.set_transition_duration(200);
    transcript_revealer.set_reveal_child(false);
    transcript_revealer.set_child(Some(&transcript_bubble));

    // Wire up count labels to buffer changes
    {
        let word_count_label = word_count_label.clone();
        let char_count_label = char_count_label.clone();
        transcript_buffer.connect_changed(move |buf| {
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let words = text.split_whitespace().count();
            let chars = text.chars().count();
            word_count_label.set_label(&format!("{words} words"));
            char_count_label.set_label(&format!("{chars} chars"));
        });
    }

    // Action row (copy + dismiss)
    let copy_btn = gtk::Button::with_label("Copy to Clipboard");
    copy_btn.add_css_class("pill");

    let done_btn = gtk::Button::with_label("Done");
    done_btn.add_css_class("pill");

    let integration_status = native_integration::integration_status();

    let action_row = gtk::Box::new(Orientation::Horizontal, 16);
    action_row.set_halign(Align::Center);
    action_row.set_margin_top(16);
    action_row.append(&copy_btn);
    action_row.append(&done_btn);

    let action_revealer = gtk::Revealer::new();
    action_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    action_revealer.set_transition_duration(200);
    action_revealer.set_reveal_child(false);
    action_revealer.set_child(Some(&action_row));

    // Dictation control: clickable fallback for users whose hotkey is not
    // active yet. This calls the in-process integration controller directly.
    let dictate_btn = gtk::Button::with_label("  Press Hotkey or Click to Start  ");
    dictate_btn.add_css_class("suggested-action");
    dictate_btn.add_css_class("pill");
    dictate_btn.add_css_class("record-button");
    dictate_btn.set_sensitive(false);
    dictate_btn.set_halign(Align::Center);
    dictate_btn.set_margin_top(36);

    let ui = MainWindowUi {
        spinner: spinner.clone(),
        waveform,
        activity_revealer: activity_revealer.clone(),
        state_label: state_label.clone(),
        setup_revealer: setup_revealer.clone(),
        setup_title: setup_title.clone(),
        setup_detail: setup_detail.clone(),
        setup_download_btn: setup_download_btn.clone(),
        setup_settings_btn: setup_settings_btn.clone(),
        setup_api_row: setup_api_row.clone(),
        setup_api_entry: setup_api_entry.clone(),
        transcript_buffer: transcript_buffer.clone(),
        transcript_revealer: transcript_revealer.clone(),
        action_revealer: action_revealer.clone(),
        dictate_btn: dictate_btn.clone(),
        copy_btn: copy_btn.clone(),
        insertion_chip: insertion_chip.clone(),
        last_cleaned: Rc::new(RefCell::new(String::new())),
        is_listening: Rc::new(RefCell::new(false)),
    };

    // Initialise insertion chip content
    ui.refresh_insertion_chip(integration_status.clone());

    // Check readiness and gate the button
    refresh_window_state(&ui, &settings);

    outer.append(&activity_revealer);
    outer.append(&state_label);
    outer.append(&setup_revealer);
    outer.append(&transcript_revealer);
    outer.append(&action_revealer);
    outer.append(&dictate_btn);

    // --- Dictation control button ---
    {
        let ui = ui.clone();
        dictate_btn.clone().connect_clicked(move |_| {
            let starting = !*ui.is_listening.borrow();
            ui.begin_toggle(starting);

            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            thread::spawn(move || {
                let result = native_integration::toggle_dictation().map_err(|e| e.to_string());
                let _ = tx.send(result);
            });

            let ui_for_value = ui.clone();
            let ui_for_disconnect = ui.clone();
            async_poll::poll_receiver(
                rx,
                ASYNC_POLL_INTERVAL,
                move |result| {
                    match result {
                        Ok(_) => ui_for_value.apply_toggle_success(starting),
                        Err(err) => ui_for_value.apply_toggle_error(&err),
                    }
                    glib::ControlFlow::Break
                },
                move || {
                    ui_for_disconnect.apply_integration_disconnect();
                    glib::ControlFlow::Break
                },
            );
        });
    }

    // --- Copy button ---
    {
        let ui = ui.clone();
        copy_btn.connect_clicked(move |_| {
            ui.copy_last_cleaned_to_clipboard();
        });
    }

    // --- Done button ---
    {
        let ui = ui.clone();
        done_btn.connect_clicked(move |_| ui.dismiss_transcript());
    }

    // --- Inline model download button ---
    {
        let ui = ui.clone();
        let settings = settings.clone();
        setup_download_btn.connect_clicked(move |btn| {
            let size = settings.borrow().model_size;
            btn.set_sensitive(false);
            btn.set_label("Downloading…");
            ui.state_label.set_label("Downloading local model…");
            ui.setup_detail
                .set_label("This can take a minute on slower connections.");

            let (tx, rx) = mpsc::channel::<Result<InlineDownloadState, String>>();
            thread::spawn(move || {
                let result = model_installer::download_model(size, |progress| {
                    let label = match progress.total_bytes {
                        Some(total) => format!(
                            "{} / {}",
                            model_installer::format_bytes(progress.bytes_downloaded),
                            model_installer::format_bytes(total),
                        ),
                        None => model_installer::format_bytes(progress.bytes_downloaded),
                    };
                    let _ = tx.send(Ok(InlineDownloadState::Progress { label }));
                });
                match result {
                    Ok(_) => {
                        let _ = tx.send(Ok(InlineDownloadState::Done));
                    }
                    Err(err) => {
                        let _ = tx.send(Err(err.to_string()));
                    }
                }
            });

            let ui_for_value = ui.clone();
            let ui_for_disconnect = ui.clone();
            let settings_for_value = settings.clone();
            let button = btn.clone();
            let btn_for_disconnect = button.clone();
            async_poll::poll_receiver(
                rx,
                Duration::from_millis(150),
                move |result| {
                    match result {
                        Ok(InlineDownloadState::Progress { label }) => {
                            ui_for_value.setup_detail.set_label(&label);
                            return glib::ControlFlow::Continue;
                        }
                        Ok(InlineDownloadState::Done) => {
                            {
                                let mut state = settings_for_value.borrow_mut();
                                state.local_model_path =
                                    Some(crate::config::model_path_for_size(size));
                                state.model_size = size;
                                let _ = state.save();
                            }
                            button.set_label("Model ready");
                            button.set_sensitive(true);
                            refresh_window_state(&ui_for_value, &settings_for_value);
                        }
                        Err(err) => {
                            button.set_label("Retry Download");
                            button.set_sensitive(true);
                            ui_for_value
                                .state_label
                                .set_label("Local dictation is still waiting on a model");
                            ui_for_value
                                .setup_detail
                                .set_label(&super::friendly_error_message(&err));
                            model_installer::cleanup_partial_for_size(size);
                        }
                    }
                    glib::ControlFlow::Break
                },
                move || {
                    btn_for_disconnect.set_label("Retry Download");
                    btn_for_disconnect.set_sensitive(true);
                    ui_for_disconnect
                        .setup_detail
                        .set_label("The model download stopped unexpectedly.");
                    model_installer::cleanup_partial_for_size(size);
                    glib::ControlFlow::Break
                },
            );
        });
    }

    // --- Inline API key save button ---
    {
        let ui = ui.clone();
        let settings = settings.clone();
        setup_api_save_btn.connect_clicked(move |_| {
            let api_key = ui.setup_api_entry.text().trim().to_string();
            if api_key.is_empty() {
                ui.state_label.set_label("Add an API key to use Cloud mode");
                return;
            }
            {
                let mut state = settings.borrow_mut();
                state.cloud_api_key = api_key;
                let _ = state.save();
            }
            ui.state_label.set_label("API key saved");
            refresh_window_state(&ui, &settings);
        });
    }

    // --- Native integration events ---
    {
        if let Some(rx) = native_integration::subscribe_integration_events() {
            let ui = ui.clone();
            let settings = settings.clone();
            let activity_stack = activity_stack.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        IntegrationEvent::StateChanged(state) => {
                            if state == integration_api::STATE_LISTENING {
                                activity_stack.set_visible_child_name("waveform");
                            } else {
                                activity_stack.set_visible_child_name("spinner");
                            }
                            ui.apply_integration_state(&state);
                            refresh_window_state(&ui, &settings);
                        }
                        IntegrationEvent::TextReady { cleaned, raw_text } => {
                            ui.show_transcript(&cleaned, &raw_text)
                        }
                        IntegrationEvent::InsertionResult {
                            ok,
                            result_kind,
                            message,
                        } => {
                            ui.apply_insertion_result(ok, &result_kind, &message);
                            refresh_window_state(&ui, &settings);
                        }
                    }
                }
                glib::ControlFlow::Continue
            });
        }
    }

    // --- Keyboard shortcuts ---
    {
        let key_controller = gtk::EventControllerKey::new();
        let dictate_btn = dictate_btn.clone();
        let ui = ui.clone();
        let transcript_revealer = transcript_revealer.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, modifier| match keyval {
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if dictate_btn.is_sensitive() {
                    dictate_btn.emit_clicked();
                }
                glib::Propagation::Stop
            }
            gdk::Key::Escape => {
                if *ui.is_listening.borrow() {
                    if dictate_btn.is_sensitive() {
                        dictate_btn.emit_clicked();
                    }
                } else if transcript_revealer.reveals_child() {
                    ui.dismiss_transcript();
                }
                glib::Propagation::Stop
            }
            gdk::Key::c if modifier.contains(gdk::ModifierType::CONTROL_MASK) => {
                if !*ui.is_listening.borrow() && transcript_revealer.reveals_child() {
                    let text = ui.last_cleaned.borrow().clone();
                    if !text.is_empty() {
                        if let Some(display) = gdk::Display::default() {
                            display.clipboard().set_text(&text);
                            ui.state_label.set_label("Copied to clipboard");
                        }
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
            _ => glib::Propagation::Proceed,
        });
        window.add_controller(key_controller);
    }

    outer.upcast()
}

// ---------------------------------------------------------------------------
// refresh_window_state — re-evaluates readiness and updates setup panel
// ---------------------------------------------------------------------------

pub(super) fn refresh_window_state(ui: &MainWindowUi, settings: &Rc<RefCell<AppSettings>>) {
    use crate::config::ProviderMode;

    let settings_ref = settings.borrow();
    let integration_status = native_integration::integration_status();
    ui.refresh_insertion_chip(integration_status.clone());

    let integration_setup = crate::desktop_setup::integration_setup_status();
    let probe = runtime::probe_runtime(&settings_ref);

    if integration_status.is_none() {
        ui.set_setup_state(
            "Starting direct typing support",
            "Clipboard Mode is active",
            if integration_setup.integration_running {
                "SayWrite is still starting direct typing support. Open Settings for diagnostics if it does not recover."
            } else {
                "SayWrite is still bringing up direct typing support. Clipboard Mode still works while it starts."
            },
            SetupAction::OpenSettings,
            true,
        );
        return;
    }

    match settings_ref.provider_mode {
        ProviderMode::Local => {
            if !probe.whisper_cli_found {
                ui.set_setup_state(
                    "Local dictation is not ready yet",
                    "whisper.cpp is missing",
                    "Open Settings to check diagnostics and finish the local runtime setup.",
                    SetupAction::OpenSettings,
                    true,
                );
                return;
            }

            if !probe.local_model_present
                && !model_installer::model_exists_for_size(settings_ref.model_size)
            {
                ui.set_setup_state(
                    "Local dictation is not ready yet",
                    "Download a local model",
                    "SayWrite can fetch the selected local model now, or open Settings for more control.",
                    SetupAction::DownloadModel(settings_ref.model_size),
                    true,
                );
                return;
            }
        }
        ProviderMode::Cloud => {
            if settings_ref.cloud_api_key.trim().is_empty() {
                ui.set_setup_state(
                    "Cloud dictation is not ready yet",
                    "Add your API key",
                    "Paste an OpenAI-compatible API key here so SayWrite can send audio to your configured cloud endpoint.",
                    SetupAction::EnterApiKey,
                    true,
                );
                return;
            }
        }
    }

    if let Some(status) = integration_status {
        if !integration_api::supports_direct_typing(&status.insertion_capability) {
            let (state_label, detail) = match status.insertion_capability.as_str() {
                integration_api::INSERTION_CAPABILITY_CLIPBOARD_ONLY => (
                    "Clipboard Mode is active",
                    "Clipboard Mode is active on this desktop. Open Settings for Direct Typing status and setup.",
                ),
                integration_api::INSERTION_CAPABILITY_NOTIFICATION_ONLY => (
                    "Direct Typing is unavailable here",
                    "This desktop can only show the result instead of typing it directly. Open Settings for details.",
                ),
                _ => (
                    "Direct Typing is unavailable here",
                    "Direct Typing is unavailable on this desktop. Open Settings for setup details.",
                ),
            };
            ui.set_setup_state(
                state_label,
                "Direct Typing is unavailable here",
                detail,
                SetupAction::OpenSettings,
                false,
            );
            return;
        }
    }

    ui.clear_setup_state();
    if !*ui.is_listening.borrow() && !ui.transcript_revealer.reveals_child() {
        ui.state_label.set_label("Ready");
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mode_chip_content(settings: &AppSettings) -> gtk::Box {
    let row = gtk::Box::new(Orientation::Horizontal, 6);
    let icon = if matches!(settings.provider_mode, crate::config::ProviderMode::Cloud) {
        "network-wireless-symbolic"
    } else {
        "drive-harddisk-symbolic"
    };
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(14);
    let label = gtk::Label::new(Some(match settings.provider_mode {
        crate::config::ProviderMode::Cloud => "Cloud",
        crate::config::ProviderMode::Local => "Local",
    }));
    label.add_css_class("caption");
    row.append(&image);
    row.append(&label);
    row
}
