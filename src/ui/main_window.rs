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
    bridge::{self, TranscriptResult},
    config::AppSettings,
    runtime::{probe_runtime, RuntimeProbe},
    ui::preferences,
};

pub fn present(app: &adw::Application, settings: Rc<RefCell<AppSettings>>) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("SayWrite")
        .default_width(960)
        .default_height(720)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&build_header(&window, settings.clone()));
    toolbar.set_content(Some(&build_content(settings)));
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

fn build_content(settings: Rc<RefCell<AppSettings>>) -> gtk::Widget {
    let outer = gtk::Box::new(Orientation::Vertical, 24);
    outer.set_margin_top(28);
    outer.set_margin_bottom(28);
    outer.set_margin_start(28);
    outer.set_margin_end(28);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(860);
    clamp.set_tightening_threshold(620);

    let content = gtk::Box::new(Orientation::Vertical, 24);
    content.append(&build_hero(settings.clone()));
    content.append(&build_setup_overview(settings.clone()));
    content.append(&build_transcript_panel(settings));
    clamp.set_child(Some(&content));

    outer.append(&clamp);
    outer.upcast()
}

fn build_hero(settings: Rc<RefCell<AppSettings>>) -> gtk::Box {
    let probe = probe_runtime(&settings.borrow());

    let card = gtk::Box::new(Orientation::Vertical, 18);
    card.add_css_class("hero-card");

    let kicker = gtk::Label::builder()
        .label("SAYWRITE")
        .xalign(0.0)
        .build();
    kicker.add_css_class("hero-kicker");

    let title = gtk::Label::builder()
        .label("Dictation should feel like one calm decision.")
        .wrap(true)
        .xalign(0.0)
        .build();
    title.add_css_class("hero-title");

    let copy = gtk::Label::builder()
        .label("The old prototype exposed every moving part at once. This rebuild narrows the surface to one obvious global action, a clean onboarding flow, and a separate place for diagnostics when you actually need them.")
        .wrap(true)
        .xalign(0.0)
        .build();
    copy.add_css_class("hero-copy");

    let action_row = gtk::Box::new(Orientation::Horizontal, 24);
    action_row.set_valign(Align::Center);

    let orb = gtk::Button::with_label(&settings.borrow().global_shortcut_label);
    orb.add_css_class("orb-button");

    let orb_copy = gtk::Label::builder()
        .label("Hands-free shortcut\nThe host helper owns listening and insertion. This window stays focused on setup, confidence, and clarity.")
        .wrap(true)
        .xalign(0.0)
        .build();
    orb_copy.add_css_class("orb-caption");

    action_row.append(&orb);
    action_row.append(&orb_copy);

    let chips = chips_row(&probe, &settings.borrow());

    card.append(&kicker);
    card.append(&title);
    card.append(&copy);
    card.append(&action_row);
    card.append(&chips);
    card
}

fn build_setup_overview(settings: Rc<RefCell<AppSettings>>) -> gtk::Box {
    let probe = probe_runtime(&settings.borrow());

    let row = gtk::Box::new(Orientation::Horizontal, 16);

    row.append(&info_card(
        "Engine",
        &format!(
            "{} mode is selected. {}",
            capitalize(&settings.borrow().provider_mode),
            if probe.local_model_present {
                "A local model is already present."
            } else {
                "Local model download still needs to happen."
            }
        ),
    ));
    row.append(&info_card(
        "Insertion",
        "The current bridge still relies on the host helper. The long-term target remains a real service boundary and IBus-oriented delivery.",
    ));
    row.append(&info_card(
        "Migration",
        "Rust now owns the visible app shell. The backend pipeline can be migrated incrementally instead of carrying the old Python UI forever.",
    ));

    row
}

fn build_transcript_panel(settings: Rc<RefCell<AppSettings>>) -> gtk::Box {
    let card = gtk::Box::new(Orientation::Vertical, 12);
    card.add_css_class("transcript-card");

    let title = gtk::Label::builder()
        .label("DICTATION")
        .xalign(0.0)
        .build();
    title.add_css_class("transcript-title");

    let status = gtk::Label::builder()
        .label("Ready. Start dictation, speak naturally, then stop when you are done.")
        .wrap(true)
        .xalign(0.0)
        .build();
    status.add_css_class("muted");

    let raw_title = gtk::Label::builder()
        .label("RAW")
        .xalign(0.0)
        .build();
    raw_title.add_css_class("transcript-title");

    let raw_text = gtk::Label::builder()
        .label("No transcript yet.")
        .wrap(true)
        .xalign(0.0)
        .build();
    raw_text.add_css_class("transcript-text");
    raw_text.set_selectable(true);

    let cleaned_title = gtk::Label::builder()
        .label("CLEANED")
        .xalign(0.0)
        .build();
    cleaned_title.add_css_class("transcript-title");

    let cleaned_text = gtk::Label::builder()
        .label("Nothing cleaned yet.")
        .wrap(true)
        .xalign(0.0)
        .build();
    cleaned_text.add_css_class("transcript-text");
    cleaned_text.set_selectable(true);

    let action_row = gtk::Box::new(Orientation::Horizontal, 12);
    let dictate_button = gtk::Button::with_label("Start Dictation");
    dictate_button.add_css_class("suggested-action");
    dictate_button.add_css_class("pill");
    let copy_button = gtk::Button::with_label("Copy Cleaned Text");
    copy_button.add_css_class("pill");
    copy_button.set_sensitive(false);
    let type_button = gtk::Button::with_label("Retry Delivery");
    type_button.add_css_class("pill");
    type_button.set_sensitive(false);
    action_row.append(&dictate_button);
    action_row.append(&copy_button);
    action_row.append(&type_button);

    let last_cleaned = Rc::new(RefCell::new(String::new()));
    let is_listening = Rc::new(RefCell::new(false));

    {
        let cleaned_text = cleaned_text.clone();
        let last_cleaned = last_cleaned.clone();
        let status = status.clone();
        copy_button.connect_clicked(move |_| {
            let current = last_cleaned.borrow().clone();
            if current.is_empty() {
                return;
            }
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&current);
                status.set_label("Cleaned transcript copied to the clipboard.");
                cleaned_text.set_label(&current);
            }
        });
    }

    {
        let status = status.clone();
        let raw_text = raw_text.clone();
        let cleaned_text = cleaned_text.clone();
        let copy_button = copy_button.clone();
        let type_button = type_button.clone();
        let dictate_button = dictate_button.clone();
        let last_cleaned = last_cleaned.clone();
        let settings = settings.clone();
        let is_listening = is_listening.clone();
        dictate_button.clone().connect_clicked(move |_| {
            let status = status.clone();
            let raw_text = raw_text.clone();
            let cleaned_text = cleaned_text.clone();
            let copy_button = copy_button.clone();
            let type_button = type_button.clone();
            let dictate_button = dictate_button.clone();
            let last_cleaned = last_cleaned.clone();
            let settings = settings.clone();
            let is_listening = is_listening.clone();

            if !*is_listening.borrow() {
                status.set_label("Starting live dictation...");
                dictate_button.set_sensitive(false);
                copy_button.set_sensitive(false);
                type_button.set_sensitive(false);

                let (tx, rx) = mpsc::channel::<Result<String, String>>();
                thread::spawn(move || {
                    let result = bridge::start_live().map_err(|err| err.to_string());
                    let _ = tx.send(result);
                });

                glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                    Ok(Ok(message)) => {
                        *is_listening.borrow_mut() = true;
                        status.set_label(&message);
                        dictate_button.set_label("Stop Dictation");
                        dictate_button.set_sensitive(true);
                        raw_text.set_label("Listening...");
                        cleaned_text.set_label("Waiting for final transcript...");
                        glib::ControlFlow::Break
                    }
                    Ok(Err(error)) => {
                        status.set_label(&format!("Could not start dictation: {error}"));
                        dictate_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        status.set_label("Dictation start worker disconnected unexpectedly.");
                        dictate_button.set_sensitive(true);
                        glib::ControlFlow::Break
                    }
                });
                return;
            }

            status.set_label("Stopping dictation and transcribing...");
            dictate_button.set_sensitive(false);

            let (tx, rx) = mpsc::channel::<Result<TranscriptResult, String>>();
            thread::spawn(move || {
                let result = bridge::stop_live().map_err(|err| err.to_string());
                let _ = tx.send(result);
            });

            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(Ok(result)) => {
                    *is_listening.borrow_mut() = false;
                    dictate_button.set_label("Start Dictation");
                    raw_text.set_label(if result.raw_text.is_empty() {
                        "No transcript produced."
                    } else {
                        &result.raw_text
                    });
                    cleaned_text.set_label(if result.cleaned_text.is_empty() {
                        "No cleaned transcript produced."
                    } else {
                        &result.cleaned_text
                    });
                    *last_cleaned.borrow_mut() = result.cleaned_text.clone();
                    let has_cleaned = !result.cleaned_text.is_empty();
                    copy_button.set_sensitive(has_cleaned);
                    type_button.set_sensitive(has_cleaned);
                    dictate_button.set_sensitive(true);
                    if has_cleaned && settings.borrow().auto_copy_cleaned_text {
                        if let Some(display) = gdk::Display::default() {
                            display.clipboard().set_text(&result.cleaned_text);
                        }
                    }
                    if has_cleaned && settings.borrow().auto_type_into_focused_app {
                        status.set_label("Dictation complete. Delivering text to the focused app...");
                        type_button.set_sensitive(false);

                        let (deliver_tx, deliver_rx) = mpsc::channel::<Result<String, String>>();
                        let text = result.cleaned_text.clone();
                        thread::spawn(move || {
                            let result = bridge::send_text(&text, 0.0).map_err(|err| err.to_string());
                            let _ = deliver_tx.send(result);
                        });

                        let status = status.clone();
                        let type_button = type_button.clone();
                        glib::timeout_add_local(Duration::from_millis(80), move || match deliver_rx.try_recv() {
                            Ok(Ok(message)) => {
                                status.set_label(&message);
                                type_button.set_sensitive(true);
                                glib::ControlFlow::Break
                            }
                            Ok(Err(error)) => {
                                status.set_label(&format!("Automatic delivery failed: {error}"));
                                type_button.set_sensitive(true);
                                glib::ControlFlow::Break
                            }
                            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                            Err(mpsc::TryRecvError::Disconnected) => {
                                status.set_label("Delivery worker disconnected unexpectedly.");
                                type_button.set_sensitive(true);
                                glib::ControlFlow::Break
                            }
                        });
                    } else if has_cleaned {
                        status.set_label("Dictation complete.");
                    } else {
                        status.set_label("Dictation complete, but no cleaned transcript was produced.");
                    }
                    glib::ControlFlow::Break
                }
                Ok(Err(error)) => {
                    *is_listening.borrow_mut() = false;
                    dictate_button.set_label("Start Dictation");
                    status.set_label(&format!("Dictation failed: {error}"));
                    dictate_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    *is_listening.borrow_mut() = false;
                    dictate_button.set_label("Start Dictation");
                    status.set_label("Dictation worker disconnected unexpectedly.");
                    dictate_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    {
        let status = status.clone();
        let last_cleaned = last_cleaned.clone();
        let type_button = type_button.clone();
        type_button.clone().connect_clicked(move |_| {
            let text = last_cleaned.borrow().clone();
            if text.is_empty() {
                status.set_label("No cleaned transcript available yet.");
                return;
            }
            status.set_label("Sending cleaned transcript to the host helper...");
            type_button.set_sensitive(false);

            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            thread::spawn(move || {
                let result = bridge::send_text(&text, 0.0).map_err(|err| err.to_string());
                let _ = tx.send(result);
            });

            let status = status.clone();
            let type_button = type_button.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || match rx.try_recv() {
                Ok(Ok(message)) => {
                    status.set_label(&message);
                    type_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Ok(Err(error)) => {
                    status.set_label(&format!("Host helper send failed: {error}"));
                    type_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    status.set_label("Host helper worker disconnected unexpectedly.");
                    type_button.set_sensitive(true);
                    glib::ControlFlow::Break
                }
            });
        });
    }

    let note = gtk::Label::builder()
        .label("This is the existing Python speech pipeline behind a new Rust shell. The next step is to migrate the pipeline itself without regressing the working experience.")
        .wrap(true)
        .xalign(0.0)
        .build();
    note.add_css_class("muted");

    card.append(&title);
    card.append(&status);
    card.append(&action_row);
    card.append(&raw_title);
    card.append(&raw_text);
    card.append(&cleaned_title);
    card.append(&cleaned_text);
    card.append(&note);
    card
}

fn mode_chip_content(settings: &AppSettings) -> gtk::Box {
    let row = gtk::Box::new(Orientation::Horizontal, 6);
    let icon = if settings.provider_mode == "cloud" {
        "network-wireless-symbolic"
    } else {
        "drive-harddisk-symbolic"
    };
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(14);
    let label = gtk::Label::new(Some(&capitalize(&settings.provider_mode)));
    row.append(&image);
    row.append(&label);
    row
}

fn chips_row(probe: &RuntimeProbe, settings: &AppSettings) -> gtk::FlowBox {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(8)
        .row_spacing(8)
        .max_children_per_line(4)
        .build();

    for label in [
        format!("Shortcut: {}", settings.global_shortcut_label),
        format!("Mode: {}", capitalize(&settings.provider_mode)),
        format!("Acceleration: {}", probe.acceleration_label),
        format!(
            "Model: {}",
            if probe.local_model_present {
                "Ready"
            } else {
                "Missing"
            }
        ),
    ] {
        let chip = gtk::Label::new(Some(&label));
        chip.add_css_class("status-chip");
        flow.insert(&chip, -1);
    }

    flow
}

fn info_card(title: &str, copy: &str) -> gtk::Box {
    let card = gtk::Box::new(Orientation::Vertical, 8);
    card.add_css_class("info-card");
    card.set_hexpand(true);

    let heading = gtk::Label::builder().label(title).xalign(0.0).build();
    heading.add_css_class("card-title");

    let body = gtk::Label::builder()
        .label(copy)
        .wrap(true)
        .xalign(0.0)
        .build();
    body.add_css_class("muted");

    card.append(&heading);
    card.append(&body);
    card
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}
