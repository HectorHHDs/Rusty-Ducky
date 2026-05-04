//! ducky/keys.rs — Key name → HID keycode lookup
//!
//! Mirrors the `duckyKeys` dict and Keycode constants from adafruit_hid in
//! duckyinpython.py. Maps DuckyScript key name strings to HID Usage IDs.
//! HID Keyboard/Keypad page codes from USB HID Usage Tables spec.

/// Return the HID keycode for a key name string (case-insensitive).
/// None = unknown key (executor prints a warning, mirrors Python behavior).
/// Mirrors `duckyKeys.get(key)` + `hasattr(Keycode, key)` in convertLine().
pub fn keycode_for_name(name: &str) -> Option<u8> {
    let mut upper = [0u8; 32];
    let b = name.as_bytes();
    if b.len() > 32 { return None; }
    for (i, &c) in b.iter().enumerate() { upper[i] = c.to_ascii_uppercase(); }
    let s = core::str::from_utf8(&upper[..b.len()]).unwrap_or("");
    match s {
        // Modifiers — 0xE0..0xE7
        "CTRL"|"CONTROL"               => Some(0xE0),
        "SHIFT"                        => Some(0xE1),
        "ALT"|"OPTION"                 => Some(0xE2),
        "GUI"|"WINDOWS"|"COMMAND"      => Some(0xE3),
        "RCTRL"                        => Some(0xE4),
        "RSHIFT"                       => Some(0xE5),
        "RALT"|"ROPTION"               => Some(0xE6),
        "RGUI"|"RWINDOWS"|"RCOMMAND"   => Some(0xE7),
        // Special
        "ENTER"                        => Some(0x28),
        "ESC"|"ESCAPE"                 => Some(0x29),
        "BACKSPACE"                    => Some(0x2A),
        "TAB"                          => Some(0x2B),
        "SPACE"                        => Some(0x2C),
        "CAPSLOCK"                     => Some(0x39),
        "F1" =>Some(0x3A),"F2" =>Some(0x3B),"F3" =>Some(0x3C),"F4" =>Some(0x3D),
        "F5" =>Some(0x3E),"F6" =>Some(0x3F),"F7" =>Some(0x40),"F8" =>Some(0x41),
        "F9" =>Some(0x42),"F10"=>Some(0x43),"F11"=>Some(0x44),"F12"=>Some(0x45),
        "F13"=>Some(0x68),"F14"=>Some(0x69),"F15"=>Some(0x6A),"F16"=>Some(0x6B),
        "F17"=>Some(0x6C),"F18"=>Some(0x6D),"F19"=>Some(0x6E),"F20"=>Some(0x6F),
        "F21"=>Some(0x70),"F22"=>Some(0x71),"F23"=>Some(0x72),"F24"=>Some(0x73),
        "PRINTSCREEN"                  => Some(0x46),
        "SCROLLLOCK"                   => Some(0x47),
        "PAUSE"|"BREAK"                => Some(0x48),
        "INSERT"                       => Some(0x49),
        "HOME"                         => Some(0x4A),
        "PAGEUP"                       => Some(0x4B),
        "DELETE"                       => Some(0x4C),
        "END"                          => Some(0x4D),
        "PAGEDOWN"                     => Some(0x4E),
        "RIGHT"|"RIGHTARROW"           => Some(0x4F),
        "LEFT"|"LEFTARROW"             => Some(0x50),
        "DOWN"|"DOWNARROW"             => Some(0x51),
        "UP"|"UPARROW"                 => Some(0x52),
        "NUMLOCK"                      => Some(0x53),
        "APP"|"MENU"                   => Some(0x76),
        // Letters
        "A"=>Some(0x04),"B"=>Some(0x05),"C"=>Some(0x06),"D"=>Some(0x07),
        "E"=>Some(0x08),"F"=>Some(0x09),"G"=>Some(0x0A),"H"=>Some(0x0B),
        "I"=>Some(0x0C),"J"=>Some(0x0D),"K"=>Some(0x0E),"L"=>Some(0x0F),
        "M"=>Some(0x10),"N"=>Some(0x11),"O"=>Some(0x12),"P"=>Some(0x13),
        "Q"=>Some(0x14),"R"=>Some(0x15),"S"=>Some(0x16),"T"=>Some(0x17),
        "U"=>Some(0x18),"V"=>Some(0x19),"W"=>Some(0x1A),"X"=>Some(0x1B),
        "Y"=>Some(0x1C),"Z"=>Some(0x1D),
        // Digits
        "1"=>Some(0x1E),"2"=>Some(0x1F),"3"=>Some(0x20),"4"=>Some(0x21),
        "5"=>Some(0x22),"6"=>Some(0x23),"7"=>Some(0x24),"8"=>Some(0x25),
        "9"=>Some(0x26),"0"=>Some(0x27),
        _ => None,
    }
}

/// Return the modifier bitmask bit for a modifier key name.
/// Bit 0=LCtrl, 1=LShift, 2=LAlt, 3=LGUI, 4=RCtrl, 5=RShift, 6=RAlt, 7=RGUI.
pub fn modifier_bit(name: &str) -> u8 {
    let mut _upper = [0u8; 32];
    let _b = name.as_bytes();
    for (i, &c) in _b.iter().enumerate().take(32) { _upper[i] = c.to_ascii_uppercase(); }
    let _us = core::str::from_utf8(&_upper[.._b.len().min(32)]).unwrap_or("");
    match _us {
        "CTRL"|"CONTROL"              => 1<<0,
        "SHIFT"                       => 1<<1,
        "ALT"|"OPTION"                => 1<<2,
        "GUI"|"WINDOWS"|"COMMAND"     => 1<<3,
        "RCTRL"                       => 1<<4,
        "RSHIFT"                      => 1<<5,
        "RALT"|"ROPTION"              => 1<<6,
        "RGUI"|"RWINDOWS"|"RCOMMAND"  => 1<<7,
        _                             => 0,
    }
}

pub fn is_modifier(name: &str) -> bool {
    modifier_bit(name) != 0
}

// ---------------------------------------------------------------------------
// Aliases so existing imports keep working
// ---------------------------------------------------------------------------

/// Alias for keycode_for_name — matches the import in parser.rs
pub fn key_name_to_code(name: &str) -> Option<u8> {
    keycode_for_name(name)
}

/// Parse a space-separated key combo line into a Vec of keycodes.
/// Mirrors convertLine() from duckyinpython.py.
pub fn convert_line(line: &str) -> heapless::Vec<u8, 8> {
    let mut codes: heapless::Vec<u8, 8> = heapless::Vec::new();
    for token in line.split_whitespace() {
        match key_name_to_code(token) {
            Some(code) => { let _ = codes.push(code); }
            None       => { defmt::warn!("Unknown key: <{}>", token); }
        }
    }
    codes
}

/// HID keycode constants — used by layout files and parser.rs.
/// These mirror adafruit_hid.keycode.Keycode.
pub mod Keycode {
    pub const LEFT_CTRL:      u8 = 0xE0;
    pub const LEFT_SHIFT:     u8 = 0xE1;
    pub const LEFT_ALT:       u8 = 0xE2;
    pub const LEFT_GUI:       u8 = 0xE3;
    pub const RIGHT_CTRL:     u8 = 0xE4;
    pub const RIGHT_SHIFT:    u8 = 0xE5;
    pub const RIGHT_ALT:      u8 = 0xE6;
    pub const RIGHT_GUI:      u8 = 0xE7;
    pub const ENTER:          u8 = 0x28;
    pub const ESCAPE:         u8 = 0x29;
    pub const BACKSPACE:      u8 = 0x2A;
    pub const TAB:            u8 = 0x2B;
    pub const SPACE:          u8 = 0x2C;
    pub const CAPS_LOCK:      u8 = 0x39;
    pub const PRINT_SCREEN:   u8 = 0x46;
    pub const SCROLL_LOCK:    u8 = 0x47;
    pub const PAUSE:          u8 = 0x48;
    pub const INSERT:         u8 = 0x49;
    pub const HOME:           u8 = 0x4A;
    pub const PAGE_UP:        u8 = 0x4B;
    pub const DELETE:         u8 = 0x4C;
    pub const END:            u8 = 0x4D;
    pub const PAGE_DOWN:      u8 = 0x4E;
    pub const RIGHT_ARROW:    u8 = 0x4F;
    pub const LEFT_ARROW:     u8 = 0x50;
    pub const DOWN_ARROW:     u8 = 0x51;
    pub const UP_ARROW:       u8 = 0x52;
    pub const KEYPAD_NUMLOCK: u8 = 0x53;
    pub const APPLICATION:    u8 = 0x65;
    pub const F1:  u8 = 0x3A; pub const F2:  u8 = 0x3B; pub const F3:  u8 = 0x3C;
    pub const F4:  u8 = 0x3D; pub const F5:  u8 = 0x3E; pub const F6:  u8 = 0x3F;
    pub const F7:  u8 = 0x40; pub const F8:  u8 = 0x41; pub const F9:  u8 = 0x42;
    pub const F10: u8 = 0x43; pub const F11: u8 = 0x44; pub const F12: u8 = 0x45;
    pub const A: u8=0x04; pub const B: u8=0x05; pub const C: u8=0x06; pub const D: u8=0x07;
    pub const E: u8=0x08; pub const F: u8=0x09; pub const G: u8=0x0A; pub const H: u8=0x0B;
    pub const I: u8=0x0C; pub const J: u8=0x0D; pub const K: u8=0x0E; pub const L: u8=0x0F;
    pub const M: u8=0x10; pub const N: u8=0x11; pub const O: u8=0x12; pub const P: u8=0x13;
    pub const Q: u8=0x14; pub const R: u8=0x15; pub const S: u8=0x16; pub const T: u8=0x17;
    pub const U: u8=0x18; pub const V: u8=0x19; pub const W: u8=0x1A; pub const X: u8=0x1B;
    pub const Y: u8=0x1C; pub const Z: u8=0x1D;
    pub const N0: u8=0x27; pub const N1: u8=0x1E; pub const N2: u8=0x1F; pub const N3: u8=0x20;
    pub const N4: u8=0x21; pub const N5: u8=0x22; pub const N6: u8=0x23; pub const N7: u8=0x24;
    pub const N8: u8=0x25; pub const N9: u8=0x26;
}
