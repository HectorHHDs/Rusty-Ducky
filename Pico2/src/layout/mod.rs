//! layout/mod.rs — Keyboard layout system (Step 7: DE + FR implemented)
//!
//! All layouts are compiled-in static tables — no dynamic loading needed.
//! DUCKY_LANG switches the active layout at runtime by changing ctx.active_layout.
//!
//! Implemented:  US, DE, FR
//! Stubbed:      ES, IT, PT, UK (fall back to US with a warning)
//! Adding a new layout: add a module under layout/, implement char_to_keycode_XX(),
//! add a variant to Layout, wire it in char_to_keycode() below.

use crate::ducky::executor::ScriptContext;

pub mod us;
pub mod de;
pub mod fr;

// ---------------------------------------------------------------------------
// Layout enum
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, defmt::Format)]
pub enum Layout {
    Us, De, Fr, Es, It, Pt, Uk, Unknown,
}

impl Layout {
    pub fn from_str(s: &str) -> Self {
        // match on a stack-allocated uppercase copy to avoid heap
        let mut upper = [0u8; 4];
        for (i, c) in s.bytes().take(4).enumerate() {
            upper[i] = c.to_ascii_uppercase();
        }
        match &upper[..s.len().min(4)] {
            b"US"   => Layout::Us,
            b"DE"   => Layout::De,
            b"FR"   => Layout::Fr,
            b"ES"   => Layout::Es,
            b"IT"   => Layout::It,
            b"PT"   => Layout::Pt,
            b"UK"   => Layout::Uk,
            other   => {
                defmt::warn!("[DUCKY_LANG] Unknown: {}", core::str::from_utf8(other).unwrap_or("?"));
                Layout::Unknown
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Layout::Us      => "US",
            Layout::De      => "DE",
            Layout::Fr      => "FR",
            Layout::Es      => "ES",
            Layout::It      => "IT",
            Layout::Pt      => "PT",
            Layout::Uk      => "UK",
            Layout::Unknown => "UNKNOWN",
        }
    }
}

// ---------------------------------------------------------------------------
// char_to_keycode — main dispatch
// ---------------------------------------------------------------------------

pub fn char_to_keycode(ch: char, layout_id: &str) -> (u8, bool) {
    match Layout::from_str(layout_id) {
        Layout::Us             => us::map(ch),
        Layout::De             => de::map(ch),
        Layout::Fr             => fr::map(ch),
        Layout::Es | Layout::It | Layout::Pt | Layout::Uk => {
            defmt::warn!("[layout] {} not yet implemented, using US", layout_id);
            us::map(ch)
        }
        Layout::Unknown => us::map(ch),
    }
}

// ---------------------------------------------------------------------------
// Runtime switch
// ---------------------------------------------------------------------------

pub fn switch_layout(lang: &str, ctx: &mut ScriptContext) {
    let l = Layout::from_str(lang);
    if l == Layout::Unknown {
        defmt::warn!("[DUCKY_LANG] Unknown: {} — keeping {}", lang, ctx.active_layout.as_str());
        return;
    }
    ctx.active_layout.clear();
    let _ = ctx.active_layout.push_str(l.as_str());
    defmt::info!("[DUCKY_LANG] → {}", l.as_str());
}
