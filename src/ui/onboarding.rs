use libadwaita as adw;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use adw::prelude::*;
use gtk::{glib, Align, Orientation};

use crate::{
    config::{AppSettings, ProviderMode},
    dictation::build_capture_args,
    model_installer,
    ui::async_poll,
};

const MIC_TEST_MIN_BYTES: u64 = 100;

pub fn present<F>(app: &adw::Application, settings: Rc<RefCell<AppSettings>>, on_complete: F)
where
    F: Fn() + 'static,
{
    let window = adw::Window::builder()
        .application(app)
        .title("Welcome to SayWrite")
        .default_width(480)
        .default_height(560)
        .resizable(false)
        .modal(true)
        .build();

    let carousel = adw::Carousel::new();
    carousel.set_allow_mouse_drag(false);
    carousel.set_allow_scroll_wheel(false);
    carousel.set_vexpand(true);

    carousel.append(&welcome_page(carousel.clone()));
    carousel.append(&mic_page(carousel.clone()));
    carousel.append(&shortcut_page(carousel.clone(), settings.clone()));
    carousel.append(&engine_page(settings.clone(), {
        let window = window.clone();
        move || {
            {
                let mut state = settings.borrow_mut();
                state.onboarding_complete = true;
                let _ = state.save();
            }
            window.close();
            on_complete();
        }
    }));

    let dots = adw::CarouselIndicatorDots::new();
    dots.set_carousel(Some(&carousel));

    let outer = gtk::Box::new(Orientation::Vertical, 0);
    outer.append(&carousel);
    outer.append(&dots);
    window.set_content(Some(&outer));
    window.present();
}

fn welcome_page(carousel: adw::Carousel) -> adw::StatusPage {
    let page = adw::StatusPage::builder()
        .icon_name("audio-input-microphone-symbolic")
        .title("Say it.\nSayWrite cleans it.")
        .description("Linux dictation should feel calm, obvious, and fast. One shortcut starts the flow. One polished result lands back in your text field.")
        .build();

    let button = gtk::Button::with_label("Get Started");
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.set_halign(Align::Center);
    button.connect_clicked(move |_| {
        let page = carousel.nth_page(1);
        carousel.scroll_to(&page, true);
    });
    page.set_child(Some(&button));
    page
}

fn mic_page(carousel: adw::Carousel) -> gtk::Box {
    let box_ = vertical_card();

    let icon = gtk::Image::from_icon_name("audio-input-microphone-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("onboarding-icon");

    let title = gtk::Label::builder()
        .label("Let's check your microphone")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    title.add_css_class("title-2");

    let body = gtk::Label::builder()
        .label("SayWrite records through your default microphone. Tap the button below to make sure it's working.")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    body.add_css_class("body");

    let status_label = gtk::Label::new(None);
    status_label.add_css_class("caption");
    status_label.set_margin_top(8);

    let recording_label = gtk::Label::new(None);
    recording_label.add_css_class("caption");
    recording_label.set_visible(false);

    let recording_progress = gtk::ProgressBar::new();
    recording_progress.set_show_text(false);
    recording_progress.set_visible(false);

    let test_btn = gtk::Button::with_label("Test Microphone");
    test_btn.add_css_class("suggested-action");
    test_btn.add_css_class("pill");
    test_btn.set_halign(Align::Center);

    let continue_btn = gtk::Button::with_label("Continue");
    continue_btn.add_css_class("suggested-action");
    continue_btn.add_css_class("pill");
    continue_btn.set_halign(Align::Center);
    continue_btn.set_visible(false);

    {
        let status_label = status_label.clone();
        let test_btn = test_btn.clone();
        let continue_btn = continue_btn.clone();
        let recording_progress = recording_progress.clone();
        let recording_label = recording_label.clone();

        test_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            status_label.set_label("Recording a short sample\u{2026}");
            recording_label.set_label("Recording…");
            recording_label.set_visible(true);
            recording_progress.set_visible(true);

            let pulse_id = Rc::new(RefCell::new(Some(glib::timeout_add_local(
                Duration::from_millis(100),
                {
                    let recording_progress = recording_progress.clone();
                    move || {
                        recording_progress.pulse();
                        glib::ControlFlow::Continue
                    }
                },
            ))));

            let (tx, rx) = mpsc::channel::<Result<bool, String>>();
            thread::spawn(move || {
                let result = test_mic_access();
                let _ = tx.send(result);
            });

            let status_label = status_label.clone();
            let test_btn = btn.clone();
            let continue_btn = continue_btn.clone();
            let status_label_on_disconnect = status_label.clone();
            let test_btn_on_disconnect = test_btn.clone();
            let recording_progress_on_disconnect = recording_progress.clone();
            let recording_label_on_disconnect = recording_label.clone();
            let pulse_id_on_disconnect = pulse_id.clone();
            let recording_progress_for_value = recording_progress.clone();
            let recording_label_for_value = recording_label.clone();

            async_poll::poll_receiver(
                rx,
                Duration::from_millis(100),
                move |result| {
                    if let Some(source_id) = pulse_id.borrow_mut().take() {
                        source_id.remove();
                    }
                    recording_progress_for_value.set_visible(false);
                    match result {
                        Ok(true) => {
                            status_label.set_label("Microphone is working!");
                            status_label.add_css_class("success");
                            recording_label_for_value.set_label("Done!");
                            test_btn.set_visible(false);
                            continue_btn.set_visible(true);
                        }
                        Ok(false) | Err(_) => {
                            status_label
                                .set_label("Could not detect audio. Check your mic and try again.");
                            recording_label_for_value.set_label("No usable audio detected");
                            test_btn.set_sensitive(true);
                            test_btn.set_label("Try Again");
                        }
                    }
                    glib::ControlFlow::Break
                },
                move || {
                    if let Some(source_id) = pulse_id_on_disconnect.borrow_mut().take() {
                        source_id.remove();
                    }
                    recording_progress_on_disconnect.set_visible(false);
                    recording_label_on_disconnect.set_label("Mic test stopped");
                    recording_label_on_disconnect.set_visible(true);
                    status_label_on_disconnect.set_label("Mic test failed unexpectedly.");
                    test_btn_on_disconnect.set_sensitive(true);
                    glib::ControlFlow::Break
                },
            );
        });
    }

    {
        let carousel = carousel.clone();
        continue_btn.connect_clicked(move |_| {
            let page = carousel.nth_page(2);
            carousel.scroll_to(&page, true);
        });
    }

    box_.append(&icon);
    box_.append(&title);
    box_.append(&body);
    box_.append(&test_btn);
    box_.append(&recording_progress);
    box_.append(&recording_label);
    box_.append(&status_label);
    box_.append(&continue_btn);
    box_
}

fn shortcut_page(carousel: adw::Carousel, settings: Rc<RefCell<AppSettings>>) -> gtk::Box {
    let box_ = vertical_card();

    let icon = gtk::Image::from_icon_name("input-keyboard-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("onboarding-icon");

    let title = gtk::Label::builder()
        .label("One shortcut, not a maze")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    title.add_css_class("title-2");

    let body = gtk::Label::builder()
        .label("Press one shortcut to start dictating. Press it again to stop. The host companion handles capture and delivers your cleaned text back to the field you were using. On supported desktops, that means direct typing. Otherwise, SayWrite falls back to the clipboard.")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    body.add_css_class("body");

    let pill = gtk::Label::builder()
        .label(&settings.borrow().global_shortcut_label)
        .halign(Align::Center)
        .build();
    pill.add_css_class("shortcut-pill");

    let button = gtk::Button::with_label("Choose Engine");
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.set_halign(Align::Center);
    button.connect_clicked(move |_| {
        let page = carousel.nth_page(3);
        carousel.scroll_to(&page, true);
    });

    box_.append(&icon);
    box_.append(&title);
    box_.append(&body);
    box_.append(&pill);
    box_.append(&button);
    box_
}

fn engine_page<F>(settings: Rc<RefCell<AppSettings>>, on_complete: F) -> gtk::Box
where
    F: Fn() + 'static,
{
    let box_ = vertical_card();

    let icon = gtk::Image::from_icon_name("system-run-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("onboarding-icon");

    let title = gtk::Label::builder()
        .label("Choose your engine")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    title.add_css_class("title-2");

    let local = gtk::CheckButton::with_label("Local");
    local.add_css_class("pill");
    let cloud = gtk::CheckButton::with_label("Cloud");
    cloud.set_group(Some(&local));
    cloud.add_css_class("pill");

    let current_mode = settings.borrow().provider_mode.clone();
    local.set_active(current_mode != ProviderMode::Cloud);
    cloud.set_active(current_mode == ProviderMode::Cloud);

    {
        let settings = settings.clone();
        local.connect_toggled(move |button| {
            if button.is_active() {
                let mut state = settings.borrow_mut();
                state.provider_mode = ProviderMode::Local;
                let _ = state.save();
            }
        });
    }
    {
        let settings = settings.clone();
        cloud.connect_toggled(move |button| {
            if button.is_active() {
                let mut state = settings.borrow_mut();
                state.provider_mode = ProviderMode::Cloud;
                let _ = state.save();
            }
        });
    }

    let local_card = option_card(
        &local,
        "Everything stays on your machine. Private, fast, and works offline.",
    );
    let cloud_card = option_card(
        &cloud,
        "Uses an external API for transcription. Good for older hardware.",
    );

    // Download progress area (shown when downloading model)
    let progress_label = gtk::Label::new(None);
    progress_label.add_css_class("caption");
    progress_label.set_margin_top(8);

    let progress_bar = gtk::ProgressBar::new();
    progress_bar.set_visible(false);
    progress_bar.set_margin_top(4);

    let cancel_btn = gtk::Button::with_label("Cancel");
    cancel_btn.add_css_class("pill");
    cancel_btn.set_halign(Align::Center);
    cancel_btn.set_visible(false);

    let finish_btn = gtk::Button::with_label("Open SayWrite");
    finish_btn.add_css_class("suggested-action");
    finish_btn.add_css_class("pill");
    finish_btn.set_halign(Align::Center);

    let cancel_requested = Arc::new(AtomicBool::new(false));

    {
        let settings_click = settings.clone();
        let progress_label = progress_label.clone();
        let progress_bar = progress_bar.clone();
        let finish_btn = finish_btn.clone();
        let cancel_btn = cancel_btn.clone();
        let cancel_requested = cancel_requested.clone();
        let on_complete = Rc::new(on_complete);

        finish_btn.connect_clicked(move |btn| {
            let mode = settings_click.borrow().provider_mode.clone();

            // Cloud mode: no model needed, just finish
            if mode == ProviderMode::Cloud {
                on_complete();
                return;
            }

            // Local mode: check if model exists
            if model_installer::model_exists() {
                on_complete();
                return;
            }

            // Need to download model
            btn.set_sensitive(false);
            btn.set_label("Downloading model\u{2026}");
            progress_bar.set_visible(true);
            progress_label.set_label("Starting download\u{2026}");
            cancel_btn.set_visible(true);
            cancel_btn.set_sensitive(true);
            cancel_requested.store(false, Ordering::Relaxed);

            let (tx, rx) = mpsc::channel::<Result<DownloadMsg, String>>();
            let cancel_flag = cancel_requested.clone();

            thread::spawn(move || {
                let result = model_installer::download_model_cancellable(
                    crate::config::ModelSize::Base,
                    |progress| {
                        let pct = progress
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
                        let _ = tx.send(Ok(DownloadMsg::Progress {
                            pct,
                            label,
                            bytes_downloaded: progress.bytes_downloaded,
                            total_bytes: progress.total_bytes,
                        }));
                    },
                    || cancel_flag.load(Ordering::Relaxed),
                );
                match result {
                    Ok(_) => {
                        let _ = tx.send(Ok(DownloadMsg::Done));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                    }
                }
            });

            let progress_label = progress_label.clone();
            let progress_bar = progress_bar.clone();
            let finish_btn = btn.clone();
            let cancel_btn = cancel_btn.clone();
            let on_complete = on_complete.clone();
            let settings_for_save = settings_click.clone();
            let progress_label_on_disconnect = progress_label.clone();
            let progress_bar_on_disconnect = progress_bar.clone();
            let finish_btn_on_disconnect = finish_btn.clone();
            let cancel_btn_on_disconnect = cancel_btn.clone();
            let started_at = Instant::now();

            async_poll::poll_receiver(
                rx,
                Duration::from_millis(100),
                move |result| {
                    match result {
                        Ok(DownloadMsg::Progress {
                            pct,
                            label,
                            bytes_downloaded,
                            total_bytes,
                        }) => {
                            let elapsed = started_at.elapsed().as_secs_f64().max(1.0);
                            let speed = bytes_downloaded as f64 / elapsed;
                            let eta_label = total_bytes.and_then(|total| {
                                if speed > 0.0 && total > bytes_downloaded {
                                    let remaining = (total - bytes_downloaded) as f64 / speed;
                                    Some(format!("about {}s left", remaining.ceil() as u64))
                                } else {
                                    None
                                }
                            });
                            progress_label.set_label(&match eta_label {
                                Some(eta) => format!("{label} • {eta}"),
                                None => label,
                            });
                            if let Some(fraction) = pct {
                                progress_bar.set_fraction(fraction);
                            } else {
                                progress_bar.pulse();
                            }
                            return glib::ControlFlow::Continue;
                        }
                        Ok(DownloadMsg::Done) => {
                            progress_bar.set_fraction(1.0);
                            progress_label.set_label("Model ready!");
                            cancel_btn.set_visible(false);
                            {
                                let mut state = settings_for_save.borrow_mut();
                                state.local_model_path = Some(crate::config::default_model_path());
                                let _ = state.save();
                            }
                            on_complete();
                        }
                        Err(err) => {
                            progress_bar.set_visible(false);
                            cancel_btn.set_visible(false);
                            if err.contains("download canceled") {
                                progress_label.set_label("Download canceled");
                            } else {
                                progress_label.set_label(&format!("Download failed: {err}"));
                            }
                            finish_btn.set_sensitive(true);
                            finish_btn.set_label("Try Again");
                            model_installer::cleanup_partial();
                        }
                    }
                    glib::ControlFlow::Break
                },
                move || {
                    progress_bar_on_disconnect.set_visible(false);
                    cancel_btn_on_disconnect.set_visible(false);
                    progress_label_on_disconnect.set_label("Download failed unexpectedly.");
                    finish_btn_on_disconnect.set_sensitive(true);
                    finish_btn_on_disconnect.set_label("Try Again");
                    model_installer::cleanup_partial();
                    glib::ControlFlow::Break
                },
            );
        });
    }

    {
        let progress_label = progress_label.clone();
        let progress_bar = progress_bar.clone();
        let finish_btn = finish_btn.clone();
        let cancel_btn = cancel_btn.clone();
        let cancel_requested = cancel_requested.clone();
        cancel_btn.clone().connect_clicked(move |_| {
            cancel_requested.store(true, Ordering::Relaxed);
            progress_label.set_label("Canceling download…");
            progress_bar.set_visible(false);
            cancel_btn.set_sensitive(false);
            finish_btn.set_sensitive(true);
            finish_btn.set_label("Try Again");
            model_installer::cleanup_partial();
        });
    }

    box_.append(&icon);
    box_.append(&title);
    box_.append(&local_card);
    box_.append(&cloud_card);
    box_.append(&progress_bar);
    box_.append(&progress_label);
    box_.append(&cancel_btn);
    box_.append(&finish_btn);
    box_
}

enum DownloadMsg {
    Progress {
        pct: Option<f64>,
        label: String,
        bytes_downloaded: u64,
        total_bytes: Option<u64>,
    },
    Done,
}

fn vertical_card() -> gtk::Box {
    let box_ = gtk::Box::new(Orientation::Vertical, 24);
    box_.set_margin_top(40);
    box_.set_margin_bottom(40);
    box_.set_margin_start(40);
    box_.set_margin_end(40);
    box_.set_valign(Align::Center);
    box_
}

fn option_card(button: &gtk::CheckButton, copy: &str) -> gtk::Box {
    let card = gtk::Box::new(Orientation::Vertical, 8);
    card.add_css_class("onboarding-option-card");

    let label = gtk::Label::builder()
        .label(copy)
        .wrap(true)
        .xalign(0.0)
        .build();
    label.add_css_class("caption");

    card.append(button);
    card.append(&label);
    card
}

/// Test microphone access by running a short GStreamer capture.
/// Returns Ok(true) if audio data was captured, Ok(false) if silent/empty.
fn test_mic_access() -> Result<bool, String> {
    use std::{fs, path::PathBuf, process::Command};

    let tmp_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("saywrite");
    let _ = fs::create_dir_all(&tmp_dir);
    let test_file = tmp_dir.join("mic-test.wav");

    // Record 1.5 seconds of audio
    let capture_args = build_capture_args(&test_file.display().to_string());
    let status = Command::new("timeout")
        .arg("2")
        .arg("gst-launch-1.0")
        .args(&capture_args)
        .status()
        .map_err(|e| format!("failed to run mic test: {e}"))?;

    // timeout returns 124 on timeout (which is expected — we want it to stop)
    // gst-launch returns non-zero on actual errors
    let file_ok = test_file.exists()
        && fs::metadata(&test_file)
            .map(|m| m.len() > MIC_TEST_MIN_BYTES)
            .unwrap_or(false);

    let _ = fs::remove_file(&test_file);

    if file_ok {
        Ok(true)
    } else if !status.success() && status.code() != Some(124) {
        Err("GStreamer could not access the microphone.".into())
    } else {
        Ok(false)
    }
}
