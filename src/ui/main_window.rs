use libadwaita as adw;
use std::{cell::RefCell, rc::Rc, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk::{gdk, glib, Align, Orientation};

use crate::{
    config::{AppSettings, ModelSize, ProviderMode},
    host_api,
    host_integration::{self, HostEvent},
    model_installer, runtime,
    ui::async_poll,
    ui::preferences,
};

const ASYNC_POLL_INTERVAL: Duration = Duration::from_millis(80);

enum InlineDownloadState {
    Progress { label: String },
    Done,
}

#[derive(Clone, Copy)]
enum SetupAction {
    DownloadModel(ModelSize),
    EnterApiKey,
    OpenSettings,
}

#[derive(Clone)]
struct WaveformBox {
    root: gtk::Box,
    bars: Vec<gtk::Box>,
    active: Rc<RefCell<bool>>,
    phase: Rc<RefCell<f64>>,
}

impl WaveformBox {
    fn new() -> Self {
        let root = gtk::Box::new(Orientation::Horizontal, 6);
        root.set_halign(Align::Center);
        root.set_valign(Align::Center);
        root.set_height_request(48);

        let bars = (0..5)
            .map(|_| {
                let bar = gtk::Box::new(Orientation::Vertical, 0);
                bar.add_css_class("waveform-bar");
                bar.set_valign(Align::Center);
                bar.set_size_request(10, 18);
                root.append(&bar);
                bar
            })
            .collect::<Vec<_>>();

        let waveform = Self {
            root,
            bars,
            active: Rc::new(RefCell::new(false)),
            phase: Rc::new(RefCell::new(0.0)),
        };
        waveform.start_animation();
        waveform
    }

    fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    fn set_active(&self, active: bool) {
        *self.active.borrow_mut() = active;
        if !active {
            self.reset();
        }
    }

    fn reset(&self) {
        for bar in &self.bars {
            bar.set_size_request(10, 18);
        }
    }

    fn start_animation(&self) {
        let bars = self.bars.clone();
        let active = self.active.clone();
        let phase = self.phase.clone();
        glib::timeout_add_local(Duration::from_millis(90), move || {
            if *active.borrow() {
                let mut current = phase.borrow_mut();
                *current += 0.45;
                let phase_value = *current;
                drop(current);

                for (index, bar) in bars.iter().enumerate() {
                    let wave = ((phase_value + index as f64 * 0.8).sin() + 1.0) / 2.0;
                    let eased = 0.25 + wave.powf(1.2) * 0.75;
                    let height = 12.0 + eased * 36.0;
                    bar.set_size_request(10, height.round() as i32);
                }
            }
            glib::ControlFlow::Continue
        });
    }
}

#[derive(Clone)]
struct MainWindowUi {
    spinner: gtk::Spinner,
    waveform: WaveformBox,
    activity_revealer: gtk::Revealer,
    state_label: gtk::Label,
    setup_revealer: gtk::Revealer,
    setup_title: gtk::Label,
    setup_detail: gtk::Label,
    setup_download_btn: gtk::Button,
    setup_settings_btn: gtk::Button,
    setup_api_row: gtk::Box,
    setup_api_entry: gtk::Entry,
    transcript_buffer: gtk::TextBuffer,
    transcript_revealer: gtk::Revealer,
    action_revealer: gtk::Revealer,
    word_count_label: gtk::Label,
    char_count_label: gtk::Label,
    dictate_btn: gtk::Button,
    copy_btn: gtk::Button,
    type_btn: gtk::Button,
    retry_btn: gtk::Button,
    insertion_chip: gtk::Button,
    last_cleaned: Rc<RefCell<String>>,
    is_listening: Rc<RefCell<bool>>,
}

impl MainWindowUi {
    fn begin_toggle(&self, starting: bool) {
        self.dictate_btn.set_sensitive(false);
        self.activity_revealer.set_reveal_child(true);
        self.waveform.set_active(false);
        self.spinner.start();
        self.setup_revealer.set_reveal_child(false);
        self.state_label.set_label(if starting {
            "Starting…"
        } else {
            "Processing…"
        });
        self.transcript_revealer.set_reveal_child(false);
        self.action_revealer.set_reveal_child(false);
    }

    fn set_setup_state(
        &self,
        state_label: &str,
        title: &str,
        detail: &str,
        action: SetupAction,
        blocks_dictation: bool,
    ) {
        self.state_label.set_label(state_label);
        self.setup_title.set_label(title);
        self.setup_detail.set_label(detail);
        self.setup_download_btn.set_visible(false);
        self.setup_settings_btn.set_visible(false);
        self.setup_api_row.set_visible(false);
        match action {
            SetupAction::DownloadModel(size) => {
                self.setup_download_btn
                    .set_label(&format!("Download {}", size.label()));
                self.setup_download_btn.set_visible(true);
            }
            SetupAction::EnterApiKey => {
                self.setup_api_entry.set_text("");
                self.setup_api_row.set_visible(true);
            }
            SetupAction::OpenSettings => {
                self.setup_settings_btn.set_visible(true);
            }
        }
        self.setup_revealer.set_reveal_child(true);
        self.dictate_btn.set_sensitive(!blocks_dictation);
    }

    fn clear_setup_state(&self) {
        self.setup_revealer.set_reveal_child(false);
        self.setup_download_btn.set_visible(false);
        self.setup_settings_btn.set_visible(false);
        self.setup_api_row.set_visible(false);
        if !*self.is_listening.borrow() {
            self.dictate_btn.set_sensitive(true);
        }
    }

    fn apply_toggle_success(&self, starting: bool) {
        self.spinner.stop();

        if starting {
            *self.is_listening.borrow_mut() = true;
            self.waveform.set_active(true);
            self.activity_revealer.set_reveal_child(true);
            self.state_label.set_label("Listening…");
            self.dictate_btn.set_label("  Stop Dictation  ");
            self.dictate_btn.remove_css_class("suggested-action");
            self.dictate_btn.add_css_class("destructive-action");
        } else {
            *self.is_listening.borrow_mut() = false;
            self.waveform.set_active(false);
            self.activity_revealer.set_reveal_child(false);
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
        self.waveform.set_active(false);
        self.activity_revealer.set_reveal_child(false);
        self.state_label.set_label(&friendly_error_message(error));
        self.dictate_btn.remove_css_class("destructive-action");
        self.dictate_btn.add_css_class("suggested-action");
        self.dictate_btn.set_label("  Retry Dictation  ");
        self.dictate_btn.set_sensitive(true);
    }

    fn apply_host_disconnect(&self) {
        *self.is_listening.borrow_mut() = false;
        self.spinner.stop();
        self.waveform.set_active(false);
        self.activity_revealer.set_reveal_child(false);
        self.state_label
            .set_label("The host companion disconnected.");
        self.dictate_btn.remove_css_class("destructive-action");
        self.dictate_btn.add_css_class("suggested-action");
        self.dictate_btn.set_label("  Start Dictation  ");
        self.dictate_btn.set_sensitive(true);
        self.refresh_insertion_chip(None);
    }

    fn apply_host_state(&self, state: &str) {
        match state {
            "listening" => {
                *self.is_listening.borrow_mut() = true;
                self.spinner.stop();
                self.waveform.set_active(true);
                self.activity_revealer.set_reveal_child(true);
                self.state_label.set_label("Listening…");
                self.dictate_btn.set_label("  Stop Dictating  ");
                self.dictate_btn.remove_css_class("suggested-action");
                self.dictate_btn.add_css_class("destructive-action");
                self.dictate_btn.set_sensitive(true);
            }
            "processing" => {
                self.waveform.set_active(false);
                self.activity_revealer.set_reveal_child(true);
                self.spinner.start();
                self.state_label.set_label("Processing your transcript…");
                self.dictate_btn.set_sensitive(false);
            }
            "done" | "idle" => {
                *self.is_listening.borrow_mut() = false;
                self.spinner.stop();
                self.waveform.set_active(false);
                self.activity_revealer.set_reveal_child(false);
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
        self.transcript_buffer.set_text(display);
        self.transcript_revealer.set_reveal_child(true);
        *self.last_cleaned.borrow_mut() = final_text.to_string();
        let has_text = !final_text.is_empty();
        self.copy_btn.set_sensitive(has_text);
        self.type_btn.set_sensitive(has_text);
        self.retry_btn.set_sensitive(true);
        self.action_revealer.set_reveal_child(has_text);
        self.update_counts();
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

    fn dismiss_transcript(&self) {
        self.transcript_revealer.set_reveal_child(false);
        self.action_revealer.set_reveal_child(false);
        self.state_label.set_label("Ready");
        self.dictate_btn.set_sensitive(true);
        self.dictate_btn.remove_css_class("destructive-action");
        self.dictate_btn.add_css_class("suggested-action");
        self.dictate_btn.set_label("  Start Dictation  ");
    }

    fn update_counts(&self) {
        let (start, end) = self.transcript_buffer.bounds();
        let text = self.transcript_buffer.text(&start, &end, true).to_string();
        let trimmed = text.trim();
        let words = trimmed.split_whitespace().count();
        let chars = trimmed.chars().count();
        self.word_count_label.set_label(&format!(
            "{words} word{}",
            if words == 1 { "" } else { "s" }
        ));
        self.char_count_label.set_label(&format!(
            "{chars} char{}",
            if chars == 1 { "" } else { "s" }
        ));
        *self.last_cleaned.borrow_mut() = trimmed.to_string();
        let has_text = !trimmed.is_empty();
        self.copy_btn.set_sensitive(has_text);
        self.type_btn.set_sensitive(has_text);
        self.action_revealer
            .set_reveal_child(has_text && self.transcript_revealer.reveals_child());
    }

    fn refresh_insertion_chip(&self, status: Option<host_api::HostStatus>) {
        let (icon_name, label) = match status
            .as_ref()
            .map(|status| status.insertion_capability.as_str())
        {
            Some(host_api::INSERTION_CAPABILITY_TYPING) => {
                ("input-keyboard-symbolic", "Direct Typing")
            }
            Some(host_api::INSERTION_CAPABILITY_CLIPBOARD_ONLY) => {
                ("edit-copy-symbolic", "Clipboard")
            }
            Some(host_api::INSERTION_CAPABILITY_NOTIFICATION_ONLY) => {
                ("preferences-system-notifications-symbolic", "Notification")
            }
            _ => ("network-offline-symbolic", "Offline"),
        };
        self.insertion_chip
            .set_child(Some(&status_chip_content(icon_name, label)));
        self.type_btn.set_visible(status.is_some());
        if let Some(status) = status {
            self.type_btn
                .set_label(match status.insertion_capability.as_str() {
                    host_api::INSERTION_CAPABILITY_TYPING => "Type into App",
                    host_api::INSERTION_CAPABILITY_CLIPBOARD_ONLY => "Copy for App",
                    host_api::INSERTION_CAPABILITY_NOTIFICATION_ONLY => "Show Result",
                    _ => "Send to App",
                });
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

    let toolbar = adw::ToolbarView::new();
    let (header, insertion_chip) = build_header(&window, settings.clone());
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&build_body(&window, settings, insertion_chip)));
    window.set_content(Some(&toolbar));
    window.present();
}

fn build_header(
    window: &adw::ApplicationWindow,
    settings: Rc<RefCell<AppSettings>>,
) -> (adw::HeaderBar, gtk::Button) {
    let header = adw::HeaderBar::new();

    let mode_chip = gtk::Button::new();
    mode_chip.add_css_class("flat");
    mode_chip.add_css_class("mode-chip");
    mode_chip.set_child(Some(&mode_chip_content(&settings.borrow())));
    mode_chip.set_tooltip_text(Some("Open settings"));
    {
        let window = window.clone();
        let settings = settings.clone();
        mode_chip.connect_clicked(move |_| preferences::present(&window, settings.clone()));
    }
    header.pack_start(&mode_chip);

    let insertion_chip = gtk::Button::new();
    insertion_chip.add_css_class("flat");
    insertion_chip.add_css_class("insertion-chip");
    insertion_chip.set_tooltip_text(Some("Open settings"));
    insertion_chip.set_child(Some(&status_chip_content(
        "network-offline-symbolic",
        "Offline",
    )));
    {
        let window = window.clone();
        let settings = settings.clone();
        insertion_chip.connect_clicked(move |_| preferences::present(&window, settings.clone()));
    }
    header.pack_start(&insertion_chip);

    let prefs = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .build();
    prefs.add_css_class("flat");
    prefs.set_tooltip_text(Some("Open settings"));
    {
        let window = window.clone();
        let settings = settings.clone();
        prefs.connect_clicked(move |_| preferences::present(&window, settings.clone()));
    }
    header.pack_end(&prefs);

    (header, insertion_chip)
}

fn build_body(
    window: &adw::ApplicationWindow,
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

    // Activity indicator
    let spinner = gtk::Spinner::new();
    spinner.set_size_request(48, 48);
    spinner.set_halign(Align::Center);

    let waveform = WaveformBox::new();

    let activity_stack = gtk::Stack::new();
    activity_stack.set_halign(Align::Center);
    activity_stack.add_named(&waveform.widget(), Some("waveform"));
    activity_stack.add_named(&spinner, Some("spinner"));
    activity_stack.set_visible_child_name("spinner");

    let activity_row = gtk::Box::new(Orientation::Vertical, 12);
    activity_row.set_halign(Align::Center);
    activity_row.append(&activity_stack);

    let activity_revealer = gtk::Revealer::new();
    activity_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    activity_revealer.set_transition_duration(300);
    activity_revealer.set_reveal_child(false);
    activity_revealer.set_child(Some(&activity_row));

    // State label
    let state_label = gtk::Label::new(Some("Ready"));
    state_label.add_css_class("state-label");
    state_label.set_margin_top(12);

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

    let setup_download_btn = gtk::Button::with_label("Download Model");
    setup_download_btn.add_css_class("suggested-action");
    setup_download_btn.add_css_class("pill");
    setup_download_btn.set_halign(Align::Center);
    setup_download_btn.set_visible(false);

    let setup_api_entry = gtk::Entry::new();
    setup_api_entry.set_placeholder_text(Some("Paste your API key"));
    setup_api_entry.set_visibility(false);
    setup_api_entry.set_hexpand(true);

    let setup_api_save_btn = gtk::Button::with_label("Save API Key");
    setup_api_save_btn.add_css_class("suggested-action");
    setup_api_save_btn.add_css_class("pill");

    let setup_api_row = gtk::Box::new(Orientation::Horizontal, 8);
    setup_api_row.set_halign(Align::Center);
    setup_api_row.set_visible(false);
    setup_api_row.append(&setup_api_entry);
    setup_api_row.append(&setup_api_save_btn);

    let setup_settings_btn = gtk::Button::with_label("Open Settings");
    setup_settings_btn.add_css_class("pill");
    setup_settings_btn.set_halign(Align::Center);
    setup_settings_btn.set_visible(false);
    {
        let window = window.clone();
        let settings = settings.clone();
        setup_settings_btn
            .connect_clicked(move |_| preferences::present(&window, settings.clone()));
    }

    setup_panel.append(&setup_icon);
    setup_panel.append(&setup_title);
    setup_panel.append(&setup_detail);
    setup_panel.append(&setup_download_btn);
    setup_panel.append(&setup_api_row);
    setup_panel.append(&setup_settings_btn);

    let setup_revealer = gtk::Revealer::new();
    setup_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    setup_revealer.set_transition_duration(300);
    setup_revealer.set_reveal_child(false);
    setup_revealer.set_child(Some(&setup_panel));

    // Transcript bubble
    let transcript_buffer = gtk::TextBuffer::new(None);
    let transcript_view = gtk::TextView::with_buffer(&transcript_buffer);
    transcript_view.set_wrap_mode(gtk::WrapMode::WordChar);
    transcript_view.set_accepts_tab(false);
    transcript_view.add_css_class("transcript-textview");

    let transcript_bubble = gtk::Box::new(Orientation::Vertical, 0);
    transcript_bubble.add_css_class("transcript-bubble");
    transcript_bubble.append(&transcript_view);
    transcript_bubble.set_margin_top(32);

    let transcript_revealer = gtk::Revealer::new();
    transcript_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    transcript_revealer.set_transition_duration(300);
    transcript_revealer.set_reveal_child(false);
    transcript_revealer.set_child(Some(&transcript_bubble));

    // Action row (copy + type, shown after result)
    let word_count_label = gtk::Label::new(Some("0 words"));
    word_count_label.add_css_class("caption");
    word_count_label.set_xalign(0.0);

    let char_count_label = gtk::Label::new(Some("0 chars"));
    char_count_label.add_css_class("caption");
    char_count_label.set_hexpand(true);
    char_count_label.set_halign(Align::Center);

    let retry_btn = gtk::Button::with_label("Retry");
    retry_btn.add_css_class("pill");

    let copy_btn = gtk::Button::with_label("Copy to Clipboard");
    copy_btn.add_css_class("pill");

    let type_btn = gtk::Button::with_label("Type into App");
    type_btn.add_css_class("pill");
    type_btn.set_visible(false);

    let action_row = gtk::Box::new(Orientation::Horizontal, 16);
    action_row.set_halign(Align::Fill);
    action_row.set_margin_top(16);
    action_row.append(&word_count_label);
    action_row.append(&char_count_label);
    action_row.append(&retry_btn);
    action_row.append(&copy_btn);
    action_row.append(&type_btn);

    let action_revealer = gtk::Revealer::new();
    action_revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    action_revealer.set_transition_duration(300);
    action_revealer.set_reveal_child(false);
    action_revealer.set_child(Some(&action_row));

    // Dictate button
    let dictate_btn = gtk::Button::with_label("  Start Dictation  ");
    dictate_btn.add_css_class("suggested-action");
    dictate_btn.add_css_class("pill");
    dictate_btn.add_css_class("record-button");
    dictate_btn.set_halign(Align::Center);
    dictate_btn.set_margin_top(36);

    let ui = MainWindowUi {
        spinner: spinner.clone(),
        waveform: waveform.clone(),
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
        word_count_label: word_count_label.clone(),
        char_count_label: char_count_label.clone(),
        dictate_btn: dictate_btn.clone(),
        copy_btn: copy_btn.clone(),
        type_btn: type_btn.clone(),
        retry_btn: retry_btn.clone(),
        insertion_chip: insertion_chip.clone(),
        last_cleaned: Rc::new(RefCell::new(String::new())),
        is_listening: Rc::new(RefCell::new(false)),
    };

    refresh_window_state(&ui, &settings);

    outer.append(&activity_revealer);
    outer.append(&state_label);
    outer.append(&setup_revealer);
    outer.append(&transcript_revealer);
    outer.append(&action_revealer);
    outer.append(&dictate_btn);

    {
        let ui = ui.clone();
        transcript_buffer.connect_changed(move |_| ui.update_counts());
    }

    // --- Dictate button ---
    {
        let ui = ui.clone();
        let activity_stack = activity_stack.clone();
        dictate_btn.clone().connect_clicked(move |_| {
            let starting = !*ui.is_listening.borrow();
            activity_stack.set_visible_child_name("spinner");
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

    // --- Inline setup actions ---
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
                                .set_label(&friendly_error_message(&err));
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

    // --- Copy button ---
    {
        let ui = ui.clone();
        copy_btn.connect_clicked(move |_| {
            ui.copy_last_cleaned_to_clipboard();
        });
    }

    // --- Retry button ---
    {
        let ui = ui.clone();
        retry_btn.connect_clicked(move |_| ui.dismiss_transcript());
    }

    // --- D-Bus signal subscription ---
    {
        if let Some(rx) = host_integration::subscribe_host_signals() {
            let ui = ui.clone();
            let settings = settings.clone();
            let activity_stack = activity_stack.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        HostEvent::StateChanged(state) => {
                            if state == host_api::STATE_LISTENING {
                                activity_stack.set_visible_child_name("waveform");
                            } else {
                                activity_stack.set_visible_child_name("spinner");
                            }
                            ui.apply_host_state(&state);
                            refresh_window_state(&ui, &settings);
                        }
                        HostEvent::TextReady { cleaned, raw_text } => {
                            ui.show_transcript(&cleaned, &raw_text)
                        }
                        HostEvent::InsertionResult {
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
                let result = host_integration::send_text(&text_for_send).map_err(|e| e.to_string());
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

fn refresh_window_state(ui: &MainWindowUi, settings: &Rc<RefCell<AppSettings>>) {
    let settings_ref = settings.borrow();
    let host_status = host_integration::host_status();
    ui.refresh_insertion_chip(host_status.clone());

    let host_setup = crate::host_setup::host_setup_status();
    let probe = runtime::probe_runtime(&settings_ref);

    if host_status.is_none() {
        ui.set_setup_state(
            "Complete setup to start dictation",
            "Direct Typing Mode is not ready",
            if host_setup.binary_installed {
                "The host companion looks installed, but the app cannot reach it yet. Open Settings to check its status."
            } else {
                "Install and run the host companion to use global shortcut dictation and send text back to other apps."
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
                    "SayWrite can fetch the selected local model now, or you can open Settings for more control.",
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

    if let Some(status) = host_status {
        if !host_api::supports_direct_typing(&status.insertion_capability) {
            ui.set_setup_state(
                "Host companion is running",
                "Direct typing is not active on this desktop",
                &format!(
                    "SayWrite will fall back to {} via {}. Dictation still works, but text will not be typed directly into the focused app.",
                    host_api::insertion_capability_label(&status.insertion_capability),
                    status.insertion_backend
                ),
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

fn status_chip_content(icon: &str, label_text: &str) -> gtk::Box {
    let row = gtk::Box::new(Orientation::Horizontal, 6);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(14);
    let label = gtk::Label::new(Some(label_text));
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
