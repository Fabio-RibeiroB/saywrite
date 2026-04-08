use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk::{gdk, glib, Align, Orientation};

use crate::{
    config::AppSettings,
    host_api,
    host_integration::{self, HostEvent},
    model_installer, runtime,
    ui::async_poll,
    ui::preferences,
};

const ASYNC_POLL_INTERVAL: Duration = Duration::from_millis(80);

#[derive(Clone)]
struct MainWindowUi {
    spinner: gtk::Spinner,
    spinner_row: gtk::Box,
    listening_visual: gtk::Box,
    waveform_bars: Vec<gtk::Box>,
    waveform_tick: Rc<RefCell<u32>>,
    waveform_source: Rc<RefCell<Option<glib::SourceId>>>,
    state_label: gtk::Label,
    setup_panel: gtk::Box,
    setup_title: gtk::Label,
    setup_detail: gtk::Label,
    transcript_label: gtk::Label,
    transcript_bubble: gtk::Box,
    action_row: gtk::Box,
    dictate_btn: gtk::Button,
    copy_btn: gtk::Button,
    type_btn: gtk::Button,
    last_cleaned: Rc<RefCell<String>>,
    is_listening: Rc<RefCell<bool>>,
}

impl MainWindowUi {
    fn begin_toggle(&self, starting: bool) {
        self.dictate_btn.set_sensitive(false);
        self.spinner_row.set_visible(true);
        self.spinner.start();
        self.setup_panel.set_visible(false);
        self.state_label.set_label(if starting {
            "Starting…"
        } else {
            "Processing…"
        });
        self.transcript_bubble.set_visible(false);
        self.action_row.set_visible(false);
    }

    fn set_setup_state(&self, state_label: &str, title: &str, detail: &str) {
        self.state_label.set_label(state_label);
        self.setup_title.set_label(title);
        self.setup_detail.set_label(detail);
        self.setup_panel.set_visible(true);
        self.dictate_btn.set_sensitive(false);
    }

    fn apply_toggle_success(&self, starting: bool) {
        self.spinner.stop();
        self.spinner_row.set_visible(false);

        if starting {
            *self.is_listening.borrow_mut() = true;
            self.state_label.set_label("Listening…");
            self.dictate_btn.set_label("  Stop Dictation  ");
            self.dictate_btn.remove_css_class("suggested-action");
            self.dictate_btn.add_css_class("destructive-action");
        } else {
            *self.is_listening.borrow_mut() = false;
            self.state_label.set_label("Transcript ready");
            self.dictate_btn.remove_css_class("destructive-action");
            self.dictate_btn.add_css_class("suggested-action");
            self.dictate_btn.set_label("  Start Dictation  ");
        }
        self.dictate_btn.set_sensitive(true);
    }

    fn apply_toggle_error(&self, error: &str) {
        *self.is_listening.borrow_mut() = false;
        self.spinner.stop();
        self.spinner_row.set_visible(false);
        self.state_label.set_label(&friendly_error_message(error));
        self.dictate_btn.remove_css_class("destructive-action");
        self.dictate_btn.add_css_class("suggested-action");
        self.dictate_btn.set_label("  Retry Dictation  ");
        self.dictate_btn.set_sensitive(true);
    }

    fn apply_host_disconnect(&self) {
        *self.is_listening.borrow_mut() = false;
        self.spinner.stop();
        self.spinner_row.set_visible(false);
        self.state_label
            .set_label("The host companion disconnected.");
        self.dictate_btn.remove_css_class("destructive-action");
        self.dictate_btn.add_css_class("suggested-action");
        self.dictate_btn.set_label("  Start Dictation  ");
        self.dictate_btn.set_sensitive(true);
    }

    fn apply_host_state(&self, state: &str) {
        match state {
            "listening" => {
                *self.is_listening.borrow_mut() = true;
                self.start_listening_visual();
                self.spinner.stop();
                self.spinner_row.set_visible(false);
                self.state_label.set_label("Listening…");
                self.dictate_btn.set_label("  Stop Dictating  ");
                self.dictate_btn.remove_css_class("suggested-action");
                self.dictate_btn.add_css_class("destructive-action");
                self.dictate_btn.set_sensitive(true);
            }
            "processing" => {
                self.stop_listening_visual();
                self.spinner_row.set_visible(true);
                self.spinner.start();
                self.state_label.set_label("Processing your transcript…");
                self.dictate_btn.set_sensitive(false);
            }
            "done" | "idle" => {
                *self.is_listening.borrow_mut() = false;
                self.stop_listening_visual();
                self.spinner.stop();
                self.spinner_row.set_visible(false);
                self.dictate_btn.remove_css_class("destructive-action");
                self.dictate_btn.add_css_class("suggested-action");
                self.dictate_btn.set_label("  Start Dictation  ");
                self.dictate_btn.set_sensitive(true);
                self.state_label.set_label(if state == "done" {
                    "Transcript ready"
                } else {
                    "Ready"
                });
            }
            _ => {}
        }
    }

    fn start_listening_visual(&self) {
        self.listening_visual.set_visible(true);
        if self.waveform_source.borrow().is_some() {
            return;
        }

        let bars = self.waveform_bars.clone();
        let tick = self.waveform_tick.clone();
        let source = glib::timeout_add_local(Duration::from_millis(90), move || {
            let mut frame = tick.borrow_mut();
            *frame = frame.wrapping_add(1);
            let current = *frame as f64;

            for (index, bar) in bars.iter().enumerate() {
                let phase = current / 2.4 + index as f64 * 0.8;
                let height =
                    14.0 + (phase.sin().abs() * 28.0) + (((phase * 0.55).cos() + 1.0) * 7.0);
                bar.set_height_request(height.round() as i32);
            }

            glib::ControlFlow::Continue
        });
        *self.waveform_source.borrow_mut() = Some(source);
    }

    fn stop_listening_visual(&self) {
        self.listening_visual.set_visible(false);
        if let Some(source) = self.waveform_source.borrow_mut().take() {
            source.remove();
        }
        for bar in &self.waveform_bars {
            bar.set_height_request(12);
        }
    }

    fn show_transcript(&self, cleaned: &str, raw_text: &str) {
        let final_text = if cleaned.is_empty() {
            raw_text
        } else {
            cleaned
        };
        let display = if final_text.is_empty() {
            "Nothing captured."
        } else {
            final_text
        };
        self.transcript_label.set_label(display);
        self.transcript_bubble.set_visible(true);
        *self.last_cleaned.borrow_mut() = final_text.to_string();
        let has_text = !final_text.is_empty();
        self.copy_btn.set_sensitive(has_text);
        self.type_btn.set_sensitive(has_text);
        self.action_row.set_visible(has_text);
    }

    fn apply_insertion_result(&self, ok: bool, result_kind: &str, message: &str) {
        if ok {
            self.state_label.set_label(&format!(
                "{}: {}",
                host_api::insertion_result_label(result_kind),
                message
            ));
        } else {
            self.state_label
                .set_label(&format!("Insertion failed: {message}"));
        }
    }

    fn start_send_to_app(&self) {
        self.type_btn.set_sensitive(false);
        self.state_label.set_label("Sending to focused app…");
    }

    fn finish_send_to_app(&self, result: Result<String, String>, text: &str) {
        match result {
            Ok(message) => self.state_label.set_label(&message),
            Err(err) => {
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(text);
                    self.state_label.set_label(&format!(
                        "{} Copied the text to your clipboard instead.",
                        friendly_error_message(&err)
                    ));
                } else {
                    self.state_label.set_label(&friendly_error_message(&err));
                }
            }
        }
        self.type_btn.set_sensitive(true);
    }

    fn copy_last_cleaned_to_clipboard(&self) {
        let text = self.last_cleaned.borrow().clone();
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(&text);
            self.state_label.set_label("Copied to clipboard");
        }
    }
}

pub fn present(app: &adw::Application, settings: Rc<RefCell<AppSettings>>) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("SayWrite")
        .default_width(640)
        .default_height(560)
        .build();
    window.set_size_request(480, 520);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    stack.set_transition_duration(200);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&build_header(&stack, settings.clone()));
    toolbar.set_content(Some(&build_body(&window, &stack, settings.clone())));
    stack.add_named(&toolbar, Some("main"));

    let settings_page = preferences::build_inline_page(settings, {
        let stack = stack.clone();
        move || {
            stack.set_visible_child_name("main");
        }
    });
    stack.add_named(&settings_page, Some("settings"));

    window.set_content(Some(&stack));
    window.present();
}

fn build_header(stack: &gtk::Stack, settings: Rc<RefCell<AppSettings>>) -> adw::HeaderBar {
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

fn build_body(
    window: &adw::ApplicationWindow,
    stack: &gtk::Stack,
    settings: Rc<RefCell<AppSettings>>,
) -> gtk::Widget {
    let outer = gtk::Box::new(Orientation::Vertical, 0);
    outer.set_valign(Align::Center);
    outer.set_vexpand(true);
    outer.set_margin_top(40);
    outer.set_margin_bottom(48);
    outer.set_margin_start(48);
    outer.set_margin_end(48);

    // Spinner (shown while starting/stopping)
    let spinner = gtk::Spinner::new();
    spinner.set_size_request(48, 48);
    spinner.set_halign(Align::Center);

    let spinner_row = gtk::Box::new(Orientation::Vertical, 12);
    spinner_row.set_halign(Align::Center);
    spinner_row.append(&spinner);
    spinner_row.set_visible(false);

    // Waveform visual (shown while listening)
    let listening_visual = gtk::Box::new(Orientation::Horizontal, 6);
    listening_visual.set_halign(Align::Center);
    listening_visual.set_valign(Align::Center);
    listening_visual.set_margin_top(8);
    listening_visual.set_margin_bottom(8);
    listening_visual.set_visible(false);

    let mut waveform_bars = Vec::new();
    for _ in 0..5 {
        let bar = gtk::Box::new(Orientation::Vertical, 0);
        bar.add_css_class("waveform-bar");
        bar.set_size_request(10, 12);
        listening_visual.append(&bar);
        waveform_bars.push(bar);
    }

    // State label
    let state_label = gtk::Label::new(Some("Ready"));
    state_label.add_css_class("state-label");
    state_label.set_margin_top(12);

    let setup_panel = gtk::Box::new(Orientation::Vertical, 10);
    setup_panel.add_css_class("setup-panel");
    setup_panel.set_halign(Align::Center);
    setup_panel.set_margin_top(18);
    setup_panel.set_margin_bottom(8);
    setup_panel.set_visible(false);

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

    let setup_action = gtk::Button::with_label("Open Settings");
    setup_action.add_css_class("pill");
    setup_action.set_halign(Align::Center);
    {
        let stack = stack.clone();
        setup_action.connect_clicked(move |_| {
            stack.set_visible_child_name("settings");
        });
    }

    setup_panel.append(&setup_icon);
    setup_panel.append(&setup_title);
    setup_panel.append(&setup_detail);
    setup_panel.append(&setup_action);

    // Transcript bubble
    let transcript_label = gtk::Label::new(None);
    transcript_label.set_wrap(true);
    transcript_label.set_selectable(true);
    transcript_label.set_xalign(0.0);
    transcript_label.add_css_class("transcript-text");

    let transcript_bubble = gtk::Box::new(Orientation::Vertical, 0);
    transcript_bubble.add_css_class("transcript-bubble");
    transcript_bubble.append(&transcript_label);
    transcript_bubble.set_visible(false);
    transcript_bubble.set_margin_top(32);

    // Action row (copy + type, shown after result)
    let copy_btn = gtk::Button::with_label("Copy to Clipboard");
    copy_btn.add_css_class("pill");

    let type_btn = gtk::Button::with_label("Type into App");
    type_btn.add_css_class("pill");
    let host_status = host_integration::host_status();
    if let Some(status) = host_status.as_ref() {
        type_btn.set_visible(true);
        type_btn.set_label(match status.insertion_capability.as_str() {
            host_api::INSERTION_CAPABILITY_TYPING => "Type into App",
            host_api::INSERTION_CAPABILITY_CLIPBOARD_ONLY => "Copy for App",
            host_api::INSERTION_CAPABILITY_NOTIFICATION_ONLY => "Show Result",
            _ => "Send to App",
        });
    } else {
        type_btn.set_visible(false);
    }

    let action_row = gtk::Box::new(Orientation::Horizontal, 16);
    action_row.set_halign(Align::Center);
    action_row.set_margin_top(16);
    action_row.append(&copy_btn);
    action_row.append(&type_btn);
    action_row.set_visible(false);

    // Dictate button
    let dictate_btn = gtk::Button::with_label("  Start Dictation  ");
    dictate_btn.add_css_class("suggested-action");
    dictate_btn.add_css_class("pill");
    dictate_btn.add_css_class("record-button");
    dictate_btn.set_halign(Align::Center);
    dictate_btn.set_margin_top(36);

    let ui = MainWindowUi {
        spinner: spinner.clone(),
        spinner_row: spinner_row.clone(),
        listening_visual: listening_visual.clone(),
        waveform_bars: waveform_bars.clone(),
        waveform_tick: Rc::new(RefCell::new(0)),
        waveform_source: Rc::new(RefCell::new(None)),
        state_label: state_label.clone(),
        setup_panel: setup_panel.clone(),
        setup_title: setup_title.clone(),
        setup_detail: setup_detail.clone(),
        transcript_label: transcript_label.clone(),
        transcript_bubble: transcript_bubble.clone(),
        action_row: action_row.clone(),
        dictate_btn: dictate_btn.clone(),
        copy_btn: copy_btn.clone(),
        type_btn: type_btn.clone(),
        last_cleaned: Rc::new(RefCell::new(String::new())),
        is_listening: Rc::new(RefCell::new(false)),
    };

    // Check readiness and gate the button
    {
        let host_ready = host_status.is_some();
        let host_setup = crate::host_setup::host_setup_status();
        let probe = runtime::probe_runtime(&settings.borrow());
        let mut blocking_setup_issue = false;
        if !host_ready {
            blocking_setup_issue = true;
            ui.set_setup_state(
                "Complete setup to start dictation",
                "Host companion required",
                if host_setup.binary_installed {
                    "The host companion looks installed, but the app cannot reach it yet. Open Settings to see the install and status steps."
                } else {
                    "Install and run the host companion to use global shortcut dictation and type text into other apps."
                },
            );
        } else if settings.borrow().provider_mode == crate::config::ProviderMode::Local {
            if !probe.whisper_cli_found {
                blocking_setup_issue = true;
                ui.set_setup_state(
                    "Local dictation is not ready yet",
                    "whisper.cpp not found",
                    "Open Settings to check diagnostics and finish the local runtime setup.",
                );
            } else if !probe.local_model_present && !model_installer::model_exists() {
                blocking_setup_issue = true;
                ui.set_setup_state(
                    "Local dictation is not ready yet",
                    "Download a local model",
                    "Open Settings and install a whisper.cpp model before starting local dictation.",
                );
            }
        }
        if !blocking_setup_issue {
            if let Some(status) = host_status.as_ref() {
                if !host_api::supports_direct_typing(&status.insertion_capability) {
                    ui.set_setup_state(
                        "Host companion is running",
                        "Direct typing is not active on this desktop",
                        &format!(
                            "SayWrite is currently using {} via {}. Dictation still works, but the result will be delivered as a fallback instead of direct typing.",
                            host_api::insertion_capability_label(&status.insertion_capability),
                            status.insertion_backend
                        ),
                    );
                }
            }
        }
    }

    outer.append(&spinner_row);
    outer.append(&listening_visual);
    outer.append(&state_label);
    outer.append(&setup_panel);
    outer.append(&transcript_bubble);
    outer.append(&action_row);
    outer.append(&dictate_btn);

    // --- Dictate button ---
    {
        let ui = ui.clone();
        dictate_btn.clone().connect_clicked(move |_| {
            let starting = !*ui.is_listening.borrow();
            ui.begin_toggle(starting);

            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            thread::spawn(move || {
                let result = host_integration::toggle_dictation().map_err(|e| e.to_string());
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
                    ui_for_disconnect.apply_host_disconnect();
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

    // --- D-Bus signal subscription ---
    {
        if let Some(rx) = host_integration::subscribe_host_signals() {
            let ui = ui.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        HostEvent::StateChanged(state) => ui.apply_host_state(&state),
                        HostEvent::TextReady { cleaned, raw_text } => {
                            ui.show_transcript(&cleaned, &raw_text)
                        }
                        HostEvent::InsertionResult {
                            ok,
                            result_kind,
                            message,
                        } => ui.apply_insertion_result(ok, &result_kind, &message),
                    }
                }
                glib::ControlFlow::Continue
            });
        }
    }

    // --- Type into app button ---
    {
        let ui = ui.clone();
        type_btn.clone().connect_clicked(move |btn| {
            let text = ui.last_cleaned.borrow().clone();
            if text.is_empty() {
                return;
            }
            ui.start_send_to_app();

            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            let text_for_send = text.clone();
            thread::spawn(move || {
                let result =
                    host_integration::send_text(&text_for_send).map_err(|e| e.to_string());
                let _ = tx.send(result);
            });

            let ui_for_value = ui.clone();
            let ui_for_disconnect = ui.clone();
            let btn = btn.clone();
            let text_for_value = text.clone();
            let text_for_disconnect = text.clone();
            async_poll::poll_receiver(
                rx,
                ASYNC_POLL_INTERVAL,
                move |result| {
                    ui_for_value.finish_send_to_app(result, &text_for_value);
                    btn.set_sensitive(true);
                    glib::ControlFlow::Break
                },
                move || {
                    ui_for_disconnect.finish_send_to_app(
                        Err("The host companion disconnected.".into()),
                        &text_for_disconnect,
                    );
                    glib::ControlFlow::Break
                },
            );
        });
    }

    // --- Keyboard shortcuts ---
    {
        let key_controller = gtk::EventControllerKey::new();
        let dictate_btn = dictate_btn.clone();
        let ui = ui.clone();
        let transcript_bubble = transcript_bubble.clone();
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
                } else if transcript_bubble.is_visible() {
                    transcript_bubble.set_visible(false);
                    ui.state_label.set_label("Ready");
                }
                glib::Propagation::Stop
            }
            gdk::Key::c if modifier.contains(gdk::ModifierType::CONTROL_MASK) => {
                if !*ui.is_listening.borrow() && transcript_bubble.is_visible() {
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

fn friendly_error_message(error: &str) -> String {
    if error.contains("unexpected error") {
        return "Something went wrong while talking to the host companion.".into();
    }
    if error.contains("Host integration is not running") || error.contains("host companion") {
        return "The host companion is not available right now.".into();
    }
    if error.contains("No dictation session is running") {
        return "There is no active dictation to stop.".into();
    }
    if error.contains("private runtime directory") {
        return "SayWrite could not access a private recording directory.".into();
    }
    error.to_string()
}
