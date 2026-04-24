use std::{cell::Cell, cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::prelude::*;
use gtk::{gdk, glib};
use libadwaita as adw;

use crate::desktop_setup;

pub fn present<F>(parent: &impl IsA<gtk::Widget>, current_shortcut: &str, on_apply: F)
where
    F: Fn(String) + 'static,
{
    let transient = parent
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());

    let dialog = adw::Window::builder()
        .title("Choose Shortcut")
        .default_width(420)
        .default_height(220)
        .modal(true)
        .build();
    if let Some(parent) = transient.as_ref() {
        dialog.set_transient_for(Some(parent));
    }

    let captured = Rc::new(RefCell::new(None::<String>));
    let saved = Rc::new(Cell::new(false));

    let title = gtk::Label::builder()
        .label("Press the shortcut you want to use")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    title.add_css_class("title-3");

    let body = gtk::Label::builder()
        .label("Use a function key or combine a key with Ctrl, Alt, Shift, or Super.")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    body.add_css_class("body");

    let current = gtk::Label::builder()
        .label(format!("Current: {current_shortcut}"))
        .halign(gtk::Align::Center)
        .build();
    current.add_css_class("caption");
    current.add_css_class("dim-label");

    let captured_label = gtk::Label::builder()
        .label("Waiting for input…")
        .halign(gtk::Align::Center)
        .build();
    captured_label.add_css_class("shortcut-pill");

    let status_label = gtk::Label::builder()
        .label("Press the new shortcut now.")
        .wrap(true)
        .justify(gtk::Justification::Center)
        .halign(gtk::Align::Center)
        .build();
    status_label.add_css_class("caption");
    status_label.add_css_class("dim-label");

    let cancel_btn = gtk::Button::with_label("Cancel");
    let save_btn = gtk::Button::with_label("Use Shortcut");
    save_btn.add_css_class("suggested-action");
    save_btn.set_sensitive(false);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    actions.set_halign(gtk::Align::Center);
    actions.append(&cancel_btn);
    actions.append(&save_btn);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.append(&title);
    content.append(&body);
    content.append(&current);
    content.append(&captured_label);
    content.append(&status_label);
    content.append(&actions);

    dialog.set_content(Some(&content));

    {
        let dialog = dialog.clone();
        cancel_btn.connect_clicked(move |_| dialog.close());
    }

    {
        let dialog = dialog.clone();
        let captured = captured.clone();
        let saved = saved.clone();
        save_btn.connect_clicked(move |_| {
            if let Some(shortcut) = captured.borrow().clone() {
                saved.set(true);
                on_apply(shortcut);
                dialog.close();
            }
        });
    }

    let controller = gtk::EventControllerKey::new();
    {
        let dialog = dialog.clone();
        let captured = captured.clone();
        let captured_label = captured_label.clone();
        let status_label = status_label.clone();
        let save_btn = save_btn.clone();
        controller.connect_key_pressed(move |_, key, _keycode, state| {
            if key == gdk::Key::Escape && relevant_modifiers(state).is_empty() {
                dialog.close();
                return glib::Propagation::Stop;
            }

            match capture_shortcut(key, state) {
                Ok(shortcut) => {
                    captured_label.set_label(&shortcut);
                    status_label.set_label("Shortcut looks good.");
                    *captured.borrow_mut() = Some(shortcut);
                    save_btn.set_sensitive(true);
                }
                Err(message) => {
                    status_label.set_label(message);
                    save_btn.set_sensitive(false);
                }
            }

            glib::Propagation::Stop
        });
    }
    dialog.add_controller(controller);

    // Temporarily disable the GNOME keybinding so the compositor does not
    // swallow keys that are already bound (e.g. the current SayWrite hotkey).
    // Restore the old binding on close unless the user saved a new one.
    desktop_setup::suspend_gnome_shortcut();
    {
        let restore_shortcut = current_shortcut.to_owned();
        let saved = saved.clone();
        dialog.connect_close_request(move |_| {
            if !saved.get() {
                desktop_setup::restore_gnome_shortcut(&restore_shortcut);
            }
            glib::Propagation::Proceed
        });
    }

    dialog.present();
}

fn capture_shortcut(key: gdk::Key, state: gdk::ModifierType) -> Result<String, &'static str> {
    if is_modifier_key(key) {
        return Err("Press a real key together with your modifiers.");
    }

    let key_label = key_label(key).ok_or("That key cannot be used as a shortcut.")?;
    let modifiers = relevant_modifiers(state);
    let function_key =
        key_label.starts_with('F') && key_label[1..].chars().all(|ch| ch.is_ascii_digit());

    if modifiers.is_empty() && !function_key {
        return Err("Use at least one modifier like Super, Ctrl, Alt, or Shift.");
    }

    let mut parts = Vec::new();
    if modifiers.contains(gdk::ModifierType::SUPER_MASK) {
        parts.push("Super".to_string());
    }
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl".to_string());
    }
    if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        parts.push("Alt".to_string());
    }
    if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        parts.push("Shift".to_string());
    }
    parts.push(key_label);
    Ok(parts.join("+"))
}

fn relevant_modifiers(state: gdk::ModifierType) -> gdk::ModifierType {
    state
        & (gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SUPER_MASK)
}

fn is_modifier_key(key: gdk::Key) -> bool {
    matches!(
        key,
        gdk::Key::Shift_L
            | gdk::Key::Shift_R
            | gdk::Key::Control_L
            | gdk::Key::Control_R
            | gdk::Key::Alt_L
            | gdk::Key::Alt_R
            | gdk::Key::Meta_L
            | gdk::Key::Meta_R
            | gdk::Key::Super_L
            | gdk::Key::Super_R
    )
}

fn key_label(key: gdk::Key) -> Option<String> {
    if key == gdk::Key::space {
        return Some("Space".into());
    }

    if let Some(ch) = key.to_unicode() {
        if ch.is_ascii_alphanumeric() {
            return Some(ch.to_ascii_uppercase().to_string());
        }
    }

    key.name().map(|name| name.replace('_', ""))
}
