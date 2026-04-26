mod state;
mod widgets;

use libadwaita as adw;
use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;

use crate::{
    config::AppSettings,
    ui::{onboarding, preferences},
};

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

    // Insertion chip is shared between header (display) and body (dynamic updates)
    let insertion_chip = gtk::Button::new();
    insertion_chip.add_css_class("flat");
    insertion_chip.add_css_class("insertion-chip");
    insertion_chip.set_tooltip_text(Some("Insertion mode"));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&widgets::build_header(
        &stack,
        settings.clone(),
        &insertion_chip,
    ));
    toolbar.set_content(Some(&widgets::build_body(
        &window,
        &stack,
        settings.clone(),
        insertion_chip,
    )));
    stack.add_named(&toolbar, Some("main"));

    let settings_page = preferences::build_inline_page(
        settings.clone(),
        {
            let stack = stack.clone();
            move || {
                stack.set_visible_child_name("main");
            }
        },
        {
            let app = app.clone();
            let window = window.clone();
            let settings = settings.clone();
            move || {
                {
                    let mut state = settings.borrow_mut();
                    state.onboarding_complete = false;
                    let _ = state.save();
                }

                let app_for_finish = app.clone();
                let settings_for_finish = settings.clone();
                onboarding::present(&app, settings.clone(), move || {
                    present(&app_for_finish, settings_for_finish.clone());
                });
                window.close();
            }
        },
    );
    stack.add_named(&settings_page, Some("settings"));

    window.set_content(Some(&stack));
    window.present();
}

fn friendly_error_message(error: &str) -> String {
    if error.contains("unexpected error") {
        return "Something went wrong while handling direct typing.".into();
    }
    if error.contains("native integration is not running") {
        return "Direct typing is not available right now.".into();
    }
    if error.contains("No dictation session is running") {
        return "There is no active dictation to stop.".into();
    }
    if error.contains("private runtime directory") {
        return "SayWrite could not access a private recording directory.".into();
    }
    error.to_string()
}
