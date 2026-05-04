//! console_log.rs — Ring buffer for errors/warnings to display in CDC console
//!
//! Stores messages that happen before/during/after the console connects.
//! The console drains this buffer before every prompt.

use heapless::{String, Vec};
use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};

const MAX_MSGS: usize = 16;
const MAX_MSG_LEN: usize = 80;

struct LogBuf {
    msgs: Vec<String<MAX_MSG_LEN>, MAX_MSGS>,
}

impl LogBuf {
    const fn new() -> Self { Self { msgs: Vec::new() } }
    fn push(&mut self, msg: &str) {
        if self.msgs.is_full() { self.msgs.remove(0); }
        let mut s: String<MAX_MSG_LEN> = String::new();
        let _ = s.push_str(&msg[..msg.len().min(MAX_MSG_LEN)]);
        let _ = self.msgs.push(s);
    }
    fn drain(&mut self) -> Vec<String<MAX_MSG_LEN>, MAX_MSGS> {
        let out = self.msgs.clone();
        self.msgs.clear();
        out
    }
    fn is_empty(&self) -> bool { self.msgs.is_empty() }
}

static LOG: Mutex<CriticalSectionRawMutex, core::cell::RefCell<LogBuf>> =
    Mutex::new(core::cell::RefCell::new(LogBuf::new()));

pub fn push(msg: &str) {
    LOG.lock(|b| b.borrow_mut().push(msg));
}

pub fn is_empty() -> bool {
    LOG.lock(|b| b.borrow().is_empty())
}

/// Drain all messages — returns them as a Vec
pub fn drain() -> Vec<String<MAX_MSG_LEN>, MAX_MSGS> {
    LOG.lock(|b| b.borrow_mut().drain())
}
