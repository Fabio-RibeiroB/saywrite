#![allow(dead_code)]

pub const BUS_NAME: &str = "io.github.saywrite.Host";
pub const OBJECT_PATH: &str = "/io/github/saywrite/Host";
pub const INTERFACE_NAME: &str = "io.github.saywrite.Host";

pub const STATE_IDLE: &str = "idle";
pub const STATE_LISTENING: &str = "listening";
pub const STATE_PROCESSING: &str = "processing";
pub const STATE_DONE: &str = "done";

pub const INSERTION_CAPABILITY_TYPING: &str = "typing";
pub const INSERTION_CAPABILITY_CLIPBOARD_ONLY: &str = "clipboard-only";
pub const INSERTION_CAPABILITY_NOTIFICATION_ONLY: &str = "notification-only";
pub const INSERTION_CAPABILITY_UNAVAILABLE: &str = "unavailable";

pub const INSERTION_RESULT_TYPED: &str = "typed";
pub const INSERTION_RESULT_COPIED: &str = "copied";
pub const INSERTION_RESULT_NOTIFIED: &str = "notified";
pub const INSERTION_RESULT_FAILED: &str = "failed";

#[derive(Debug, Clone)]
pub struct HostStatus {
    pub status: String,
    pub hotkey_active: bool,
    pub insertion_available: bool,
    pub insertion_capability: String,
    pub insertion_backend: String,
}

pub fn insertion_capability_label(capability: &str) -> &'static str {
    match capability {
        INSERTION_CAPABILITY_TYPING => "Direct typing",
        INSERTION_CAPABILITY_CLIPBOARD_ONLY => "Clipboard fallback",
        INSERTION_CAPABILITY_NOTIFICATION_ONLY => "Notification fallback",
        _ => "Unavailable",
    }
}

pub fn insertion_result_label(result: &str) -> &'static str {
    match result {
        INSERTION_RESULT_TYPED => "Typed into field",
        INSERTION_RESULT_COPIED => "Copied to clipboard",
        INSERTION_RESULT_NOTIFIED => "Shown in notification",
        _ => "Insertion failed",
    }
}

pub fn supports_direct_typing(capability: &str) -> bool {
    capability == INSERTION_CAPABILITY_TYPING
}
