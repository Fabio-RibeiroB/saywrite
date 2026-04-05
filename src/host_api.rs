#![allow(dead_code)]

pub const BUS_NAME: &str = "io.github.saywrite.Host";
pub const OBJECT_PATH: &str = "/io/github/saywrite/Host";
pub const INTERFACE_NAME: &str = "io.github.saywrite.Host";

pub const STATE_IDLE: &str = "idle";
pub const STATE_LISTENING: &str = "listening";
pub const STATE_PROCESSING: &str = "processing";
pub const STATE_DONE: &str = "done";

#[derive(Debug, Clone)]
pub struct HostStatus {
    pub status: String,
    pub hotkey_active: bool,
    pub insertion_available: bool,
}
