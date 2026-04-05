use std::{sync::mpsc, time::Duration};

use gtk::glib;

pub fn poll_receiver<T, FValue, FDisconnected>(
    rx: mpsc::Receiver<T>,
    interval: Duration,
    mut on_value: FValue,
    mut on_disconnected: FDisconnected,
) where
    T: 'static,
    FValue: FnMut(T) -> glib::ControlFlow + 'static,
    FDisconnected: FnMut() -> glib::ControlFlow + 'static,
{
    glib::timeout_add_local(interval, move || match rx.try_recv() {
        Ok(value) => on_value(value),
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => on_disconnected(),
    });
}
