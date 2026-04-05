use libadwaita as adw;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc,
    thread,
    time::Duration,
};

use adw::prelude::*;
use gtk::{gdk, glib, Align, Orientation};

use crate::{
    config::AppSettings,
    dictation::{self, TranscriptResult},
    host_integration,
    model_installer,
    runtime,
    ui::preferences,
};

pub fn present(app: &adw::Application, settings: Rc<RefCell<AppSettings>>) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("SayWrite")
        .default_width(640)
        .default_height(560)
        .resizable(false)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&build_header(&window, settings.clone()));
    toolbar.set_content(Some(&build_body(&window, settings)));
    window.set_content(Some(&toolbar));
    window.present();
}

fn build_header(
    window: &adw::ApplicationWindow,
    settings: Rc<RefCell<AppSettings>>,
) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();

    let mode_chip = gtk::Button::new();
    mode_chip.add_css_class("flat");
    mode_chip.add_css_class("mode-chip");
    mode_chip.set_child(Some(&mode_chip_content(&settings.borrow())));
    {
        let window = window.clone();
        let settings = settings.clone();
        mode_chip.connect_clicked(move |_| preferences::present(&window, settings.clone()));
    }
    header.pack_start(&mode_chip);

    let prefs = gtk::Button::builder()
        .icon_name("preferences-system-symbolic")
        .build();
    prefs.add_css_class("flat");
    {
        let window = window.clone();
        let settings = settings.clone();
        prefs.connect_clicked(move |_| preferences::present(&window, settings.clone()));
    }
    header.pack_end(&prefs);

    header
}

fn build_body(
    _window: &adw::ApplicationWindow,
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

    // State label
    let state_label = gtk::Label::new(Some("Ready"));
    state_label.add_css_class("state-label");
    state_label.set_margin_top(12);

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
    let copy_btn = gtk::Button::with_label("Copy");
    copy_btn.add_css_class("pill");

    let type_btn = gtk::Button::with_label("Type into app");
    type_btn.add_css_class("pill");
    type_btn.set_visible(host_integration::host_available());

    let action_row = gtk::Box::new(Orientation::Horizontal, 16);
    action_row.set_halign(Align::Center);
    action_row.set_margin_top(16);
    action_row.append(&copy_btn);
    action_row.append(&type_btn);
    action_row.set_visible(false);

    // Dictate button
    let dictate_btn = gtk::Button::with_label("  Start Dictating  ");
    dictate_btn.add_css_class("suggested-action");
    dictate_btn.add_css_class("pill");
    dictate_btn.add_css_class("record-button");
    dictate_btn.set_halign(Align::Center);
    dictate_btn.set_margin_top(36);

    // Check readiness and gate the button
    {
        let probe = runtime::probe_runtime(&settings.borrow());
        if settings.borrow().provider_mode == crate::config::ProviderMode::Local {
            if !probe.whisper_cli_found {
                state_label.set_label("whisper.cpp not found — check Diagnostics in Settings");
                dictate_btn.set_sensitive(false);
            } else if !probe.local_model_present && !model_installer::model_exists() {
                state_label.set_label("No model downloaded — open Settings to install one");
                dictate_btn.set_sensitive(false);
            }
        }
    }

    outer.append(&spinner_row);
    outer.append(&state_label);
    outer.append(&transcript_bubble);
    outer.append(&action_row);
    outer.append(&dictate_btn);

    let last_cleaned: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let is_listening: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    // --- Dictate button ---
    {
        let spinner = spinner.clone();
        let spinner_row = spinner_row.clone();
        let state_label = state_label.clone();
        let transcript_bubble = transcript_bubble.clone();
        let transcript_label = transcript_label.clone();
        let action_row = action_row.clone();
        let copy_btn = copy_btn.clone();
        let type_btn = type_btn.clone();
        let dictate_btn = dictate_btn.clone();
        let last_cleaned = last_cleaned.clone();
        let is_listening = is_listening.clone();
        let settings = settings.clone();

        dictate_btn.clone().connect_clicked(move |btn| {
            if !*is_listening.borrow() {
                // START
                btn.set_sensitive(false);
                spinner_row.set_visible(true);
                spinner.start();
                state_label.set_label("Starting\u{2026}");
                transcript_bubble.set_visible(false);
                action_row.set_visible(false);

                let (tx, rx) = mpsc::channel::<Result<String, String>>();
                let settings_snapshot = settings.borrow().clone();
                thread::spawn(move || {
                    let result = dictation::start_live(&settings_snapshot).map_err(|e| e.to_string());
                    let _ = tx.send(result);
                });

                let spinner = spinner.clone();
                let spinner_row = spinner_row.clone();
                let state_label = state_label.clone();
                let dictate_btn = dictate_btn.clone();
                let is_listening = is_listening.clone();

                glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                    Ok(Ok(_)) => {
                        *is_listening.borrow_mut() = true;
                        spinner.stop();
                        spinner_row.set_visible(false);
                        state_label.set_label("Listening\u{2026}");
                        dictate_btn.set_label("  Stop Dictating  ");
                        dictate_btn.remove_css_class("suggested-action");
                        dictate_btn.add_css_class("destructive-action");
                        dictate_btn.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                    Ok(Err(err)) => {
                        spinner.stop();
                        spinner_row.set_visible(false);
                        state_label.set_label(&format!("Error: {}", err));
                        dictate_btn.set_label("  Try Again  ");
                        dictate_btn.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        spinner.stop();
                        spinner_row.set_visible(false);
                        state_label.set_label("Worker disconnected.");
                        dictate_btn.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                });
            } else {
                // STOP
                btn.set_sensitive(false);
                spinner_row.set_visible(true);
                spinner.start();
                state_label.set_label("Processing\u{2026}");

                let (tx, rx) = mpsc::channel::<Result<TranscriptResult, String>>();
                let settings_snapshot = settings.borrow().clone();
                thread::spawn(move || {
                    let result = dictation::stop_live(&settings_snapshot).map_err(|e| e.to_string());
                    let _ = tx.send(result);
                });

                let spinner = spinner.clone();
                let spinner_row = spinner_row.clone();
                let state_label = state_label.clone();
                let transcript_label = transcript_label.clone();
                let transcript_bubble = transcript_bubble.clone();
                let action_row = action_row.clone();
                let copy_btn = copy_btn.clone();
                let type_btn = type_btn.clone();
                let dictate_btn = dictate_btn.clone();
                let last_cleaned = last_cleaned.clone();
                let is_listening = is_listening.clone();
                let settings = settings.clone();

                glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                    Ok(Ok(result)) => {
                        *is_listening.borrow_mut() = false;
                        spinner.stop();
                        spinner_row.set_visible(false);

                        let cleaned = if result.cleaned_text.is_empty() {
                            result.raw_text.clone()
                        } else {
                            result.cleaned_text.clone()
                        };
                        let display = if cleaned.is_empty() {
                            "Nothing captured.".to_string()
                        } else {
                            cleaned.clone()
                        };
                        transcript_label.set_label(&display);
                        transcript_bubble.set_visible(true);
                        *last_cleaned.borrow_mut() = cleaned.clone();

                        let has_result = !cleaned.is_empty();
                        copy_btn.set_sensitive(has_result);
                        type_btn.set_sensitive(has_result);
                        action_row.set_visible(has_result);

                        dictate_btn.remove_css_class("destructive-action");
                        dictate_btn.add_css_class("suggested-action");
                        dictate_btn.set_label("  Dictate Again  ");
                        dictate_btn.set_sensitive(true);

                        if has_result && settings.borrow().auto_copy_cleaned_text {
                            if let Some(disp) = gdk::Display::default() {
                                disp.clipboard().set_text(&cleaned);
                            }
                            state_label.set_label("Done \u{2014} copied to clipboard");
                        } else if has_result && settings.borrow().auto_type_into_focused_app {
                            state_label.set_label("Delivering to focused app\u{2026}");
                            type_btn.set_sensitive(false);
                            let (dtx, drx) = mpsc::channel::<Result<String, String>>();
                            let text = cleaned.clone();
                            thread::spawn(move || {
                                let r = host_integration::send_text(&text, 0.0).map_err(|e| e.to_string());
                                let _ = dtx.send(r);
                            });
                            let state_label = state_label.clone();
                            let type_btn = type_btn.clone();
                            let cleaned = cleaned.clone();
                            glib::timeout_add_local(Duration::from_millis(80), move || {
                                match drx.try_recv() {
                                    Ok(Ok(msg)) => {
                                        state_label.set_label(&msg);
                                        type_btn.set_sensitive(true);
                                        glib::ControlFlow::Break
                                    }
                                    Ok(Err(err)) => {
                                        if let Some(disp) = gdk::Display::default() {
                                            disp.clipboard().set_text(&cleaned);
                                            state_label.set_label(&format!("{err} Copied to clipboard instead."));
                                        } else {
                                            state_label.set_label(&format!("Delivery failed: {}", err));
                                        }
                                        type_btn.set_sensitive(true);
                                        glib::ControlFlow::Break
                                    }
                                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                                    Err(mpsc::TryRecvError::Disconnected) => {
                                        type_btn.set_sensitive(true);
                                        glib::ControlFlow::Break
                                    }
                                }
                            });
                        } else {
                            state_label.set_label("Done");
                        }

                        glib::ControlFlow::Break
                    }
                    Ok(Err(err)) => {
                        *is_listening.borrow_mut() = false;
                        spinner.stop();
                        spinner_row.set_visible(false);
                        state_label.set_label(&format!("Error: {}", err));
                        dictate_btn.remove_css_class("destructive-action");
                        dictate_btn.add_css_class("suggested-action");
                        dictate_btn.set_label("  Try Again  ");
                        dictate_btn.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        *is_listening.borrow_mut() = false;
                        spinner.stop();
                        spinner_row.set_visible(false);
                        state_label.set_label("Worker disconnected.");
                        dictate_btn.set_label("  Start Dictating  ");
                        dictate_btn.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                });
            }
        });
    }

    // --- Copy button ---
    {
        let last_cleaned = last_cleaned.clone();
        let state_label = state_label.clone();
        copy_btn.connect_clicked(move |_| {
            let text = last_cleaned.borrow().clone();
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&text);
                state_label.set_label("Copied to clipboard");
            }
        });
    }

    // --- Type into app button ---
    {
        type_btn.clone().connect_clicked(move |btn| {
            let text = last_cleaned.borrow().clone();
            if text.is_empty() {
                return;
            }
            btn.set_sensitive(false);
            state_label.set_label("Sending to focused app\u{2026}");

            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            let text_for_send = text.clone();
            thread::spawn(move || {
                let result = host_integration::send_text(&text_for_send, 0.0).map_err(|e| e.to_string());
                let _ = tx.send(result);
            });

            let state_label = state_label.clone();
            let btn = btn.clone();
            let text = text.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(Ok(msg)) => {
                    state_label.set_label(&msg);
                    btn.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Ok(Err(err)) => {
                    if let Some(display) = gdk::Display::default() {
                        display.clipboard().set_text(&text);
                        state_label.set_label(&format!("{err} Copied to clipboard instead."));
                    } else {
                        state_label.set_label(&format!("Send failed: {}", err));
                    }
                    btn.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    btn.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        });
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
