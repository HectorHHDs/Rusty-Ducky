//! attackmode.rs — ATTACKMODE parsing and mode storage
//!
//! Reads the first line of the boot payload before USB starts.
//! Syntax:
//!   ATTACKMODE HID              — keyboard + mouse only
//!   ATTACKMODE HID CDC          — keyboard + mouse + serial (default)
//!   ATTACKMODE STORAGE          — USB mass storage only (SD required)
//!   ATTACKMODE HID STORAGE      — keyboard + mouse + mass storage (SD required)
//!   ATTACKMODE HID CDC STORAGE  — everything
//!
//! If STORAGE is requested but no SD card is detected, falls back to HID CDC.
//! If no ATTACKMODE line is found, defaults to HID CDC.

use defmt::*;

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
pub struct AttackMode {
    pub hid:      bool,  // keyboard + mouse
    pub cdc:      bool,  // key_listener port (ttyACM1)
    pub storage:  bool,  // USB mass storage
    pub terminal: bool,  // mgmt console (ttyACM0)
}

impl AttackMode {
    pub const fn new(hid: bool, cdc: bool, storage: bool, terminal: bool) -> Self {
        Self { hid, cdc, storage, terminal }
    }
    pub fn has_hid(&self)      -> bool { self.hid }
    pub fn has_cdc(&self)      -> bool { self.cdc }
    pub fn has_storage(&self)  -> bool { self.storage }
    pub fn has_terminal(&self) -> bool { self.terminal }

    // Default: HID + CDC + TERMINAL (hid=true, cdc=true, storage=false, terminal=true)
    pub const DEFAULT: Self = Self::new(true, true, false, true);
    // Prog mode: always has terminal
    pub fn with_terminal(mut self) -> Self { self.terminal = true; self }
}

static mut CURRENT_MODE: AttackMode = AttackMode::DEFAULT;
static mut FALLBACK_REASON: Option<&'static str> = None;

pub fn get() -> AttackMode {
    unsafe { CURRENT_MODE }
}

/// Returns Some(reason) if a fallback occurred, None if requested mode is active
pub fn fallback_reason() -> Option<&'static str> {
    unsafe { FALLBACK_REASON }
}

pub fn set(mode: AttackMode) {
    unsafe { CURRENT_MODE = mode; }
    info!("[attackmode] set to {:?}", mode);
}

/// Parse ATTACKMODE from payload text.
/// Reads only the first non-comment, non-empty line.
pub fn parse_from_payload(text: &str, sd_available: bool) -> AttackMode {
    for line in text.lines() {
        let stripped = line.trim();
        if stripped.is_empty() || stripped.starts_with("REM") { continue; }

        let upper = to_upper(stripped);
        let u = upper.as_str();

        if !u.starts_with("ATTACKMODE") { break; }

        let rest = to_upper(stripped["ATTACKMODE".len()..].trim());
        let r = rest.as_str();

        let hid      = r.contains("HID");
        let cdc      = r.contains("CDC");
        let storage  = r.contains("STORAGE");
        let terminal = r.contains("TERMINAL");

        let mut mode = AttackMode::new(hid, cdc, storage, terminal);

        // Check SD availability
        if mode.storage && !sd_available {
            warn!("[attackmode] STORAGE requested but no SD card — disabling storage");
            let reason = "ATTACKMODE STORAGE requested but no SD card — storage disabled";
            unsafe { FALLBACK_REASON = Some(reason); }
            crate::console_log::push(reason);
            mode.storage = false;
        }

        return mode;
    }

    AttackMode::DEFAULT
}

/// Parse ATTACKMODE without applying SD fallback — for display/warning purposes only
pub fn parse_requested(text: &str) -> AttackMode {
    parse_from_payload(text, true)
}

fn to_upper(s: &str) -> heapless::String<64> {
    let mut out: heapless::String<64> = heapless::String::new();
    for c in s.chars().take(64) { let _ = out.push(c.to_ascii_uppercase()); }
    out
}
