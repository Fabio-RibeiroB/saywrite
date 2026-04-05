use libadwaita as adw;
use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::{Align, Orientation};

use crate::config::{AppSettings, ProviderMode};

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
        .label("Microphone access stays inside the app")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    title.add_css_class("title-2");

    let body = gtk::Label::builder()
        .label("SayWrite records through your default microphone. No drivers to install, no permissions to hunt down — it just works.")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    body.add_css_class("body");

    let button = gtk::Button::with_label("Continue");
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.set_halign(Align::Center);
    button.connect_clicked(move |_| {
        let page = carousel.nth_page(2);
        carousel.scroll_to(&page, true);
    });

    box_.append(&icon);
    box_.append(&title);
    box_.append(&body);
    box_.append(&button);
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
        .label("Press one shortcut to start dictating. Press it again to stop. Your words appear wherever you're typing.")
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

    let button = gtk::Button::with_label("Open SayWrite");
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.set_halign(Align::Center);
    button.connect_clicked(move |_| on_complete());

    box_.append(&icon);
    box_.append(&title);
    box_.append(&local_card);
    box_.append(&cloud_card);
    box_.append(&button);
    box_
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
