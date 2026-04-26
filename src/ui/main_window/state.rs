use std::{cell::RefCell, rc::Rc, time::Duration};

use gtk::prelude::*;
use gtk::{gdk, glib, Align, Orientation};

use crate::integration_api;

// ---------------------------------------------------------------------------
// WaveformBox — animated listening indicator
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct WaveformBox {
    root: gtk::Box,
    bars: Vec<gtk::Box>,
    active: Rc<RefCell<bool>>,
    phase: Rc<RefCell<f64>>,
}

impl WaveformBox {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub(crate) fn set_active(&self, active: bool) {
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

// ---------------------------------------------------------------------------
// SetupAction — inline action shown in the setup panel
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) enum SetupAction {
    DownloadModel(crate::config::ModelSize),
    EnterApiKey,
    OpenSettings,
}

// ---------------------------------------------------------------------------
// MainWindowUi — shared widget state
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct MainWindowUi {
    pub(crate) spinner: gtk::Spinner,
    pub(crate) waveform: WaveformBox,
    pub(crate) activity_revealer: gtk::Revealer,
    pub(crate) state_label: gtk::Label,
    pub(crate) setup_revealer: gtk::Revealer,
    pub(crate) setup_title: gtk::Label,
    pub(crate) setup_detail: gtk::Label,
    pub(crate) setup_download_btn: gtk::Button,
    pub(crate) setup_settings_btn: gtk::Button,
    pub(crate) setup_api_row: gtk::Box,
    pub(crate) setup_api_entry: gtk::Entry,
    pub(crate) transcript_buffer: gtk::TextBuffer,
    pub(crate) transcript_revealer: gtk::Revealer,
    pub(crate) action_revealer: gtk::Revealer,
    pub(crate) dictate_btn: gtk::Button,
    pub(crate) copy_btn: gtk::Button,
    pub(crate) insertion_chip: gtk::Button,
    pub(crate) last_cleaned: Rc<RefCell<String>>,
    pub(crate) is_listening: Rc<RefCell<bool>>,
}

impl MainWindowUi {
    pub(crate) fn begin_toggle(&self, starting: bool) {
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

    pub(crate) fn set_setup_state(
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
        self.dictate_btn
            .set_sensitive(!blocks_dictation && *self.is_listening.borrow());
    }

    pub(crate) fn clear_setup_state(&self) {
        self.setup_revealer.set_reveal_child(false);
        self.setup_download_btn.set_visible(false);
        self.setup_settings_btn.set_visible(false);
        self.setup_api_row.set_visible(false);
        if !*self.is_listening.borrow() {
            self.dictate_btn
                .set_label("  Press Hotkey or Click to Start  ");
            self.dictate_btn.set_sensitive(true);
        }
    }

    pub(crate) fn apply_toggle_success(&self, starting: bool) {
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
            self.dictate_btn
                .set_label("  Press Hotkey or Click to Start  ");
            self.dictate_btn.set_sensitive(true);
        }
        if starting {
            self.dictate_btn.set_sensitive(true);
        }
    }

    pub(crate) fn apply_toggle_error(&self, error: &str) {
        *self.is_listening.borrow_mut() = false;
        self.spinner.stop();
        self.waveform.set_active(false);
        self.activity_revealer.set_reveal_child(false);
        self.state_label
            .set_label(&super::friendly_error_message(error));
        self.dictate_btn.remove_css_class("destructive-action");
        self.dictate_btn.add_css_class("suggested-action");
        self.dictate_btn
            .set_label("  Press Hotkey or Click to Start  ");
        self.dictate_btn.set_sensitive(true);
    }

    pub(crate) fn apply_integration_disconnect(&self) {
        *self.is_listening.borrow_mut() = false;
        self.spinner.stop();
        self.waveform.set_active(false);
        self.activity_revealer.set_reveal_child(false);
        self.state_label.set_label("Direct Typing unavailable");
        self.dictate_btn.remove_css_class("destructive-action");
        self.dictate_btn.add_css_class("suggested-action");
        self.dictate_btn
            .set_label("  Press Hotkey or Click to Start  ");
        self.dictate_btn.set_sensitive(false);
        self.refresh_insertion_chip(None);
    }

    pub(crate) fn apply_integration_state(&self, state: &str) {
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
                self.dictate_btn
                    .set_label("  Press Hotkey or Click to Start  ");
                self.dictate_btn.set_sensitive(true);
                self.state_label.set_label(if state == "done" {
                    "Transcript ready"
                } else {
                    "Press your hotkey to start dictation"
                });
            }
            _ => {}
        }
    }

    pub(crate) fn show_transcript(&self, cleaned: &str, raw_text: &str) {
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
        self.action_revealer.set_reveal_child(has_text);
    }

    pub(crate) fn apply_insertion_result(&self, ok: bool, result_kind: &str, message: &str) {
        if ok {
            self.state_label.set_label(&format!(
                "{}: {}",
                integration_api::insertion_result_label(result_kind),
                message
            ));
        } else {
            self.state_label
                .set_label(&format!("Insertion failed: {message}"));
        }
    }

    pub(crate) fn copy_last_cleaned_to_clipboard(&self) {
        let text = self.last_cleaned.borrow().clone();
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(&text);
            self.state_label.set_label("Copied to clipboard");
        }
    }

    pub(crate) fn dismiss_transcript(&self) {
        self.transcript_revealer.set_reveal_child(false);
        self.action_revealer.set_reveal_child(false);
        if !*self.is_listening.borrow() {
            self.state_label
                .set_label("Press your hotkey to start dictation");
        }
    }

    pub(crate) fn refresh_insertion_chip(
        &self,
        status: Option<crate::integration_api::IntegrationStatus>,
    ) {
        let (icon, label) = match status.as_ref() {
            Some(s) => match s.insertion_capability.as_str() {
                integration_api::INSERTION_CAPABILITY_TYPING => {
                    ("input-keyboard-symbolic", "Direct Typing")
                }
                integration_api::INSERTION_CAPABILITY_CLIPBOARD_ONLY => {
                    ("edit-copy-symbolic", "Clipboard")
                }
                integration_api::INSERTION_CAPABILITY_NOTIFICATION_ONLY => {
                    ("notification-symbolic", "Notification")
                }
                _ => ("help-browser-symbolic", "Unknown"),
            },
            None => ("network-offline-symbolic", "Offline"),
        };

        let row = gtk::Box::new(Orientation::Horizontal, 6);
        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(14);
        let label_widget = gtk::Label::new(Some(label));
        label_widget.add_css_class("caption");
        row.append(&image);
        row.append(&label_widget);
        self.insertion_chip.set_child(Some(&row));
    }
}
