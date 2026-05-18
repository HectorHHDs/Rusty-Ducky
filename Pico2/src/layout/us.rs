//! layout/us.rs — US keyboard layout
//!
//! Maps Unicode characters to (HID keycode, need_shift) pairs.
//! Mirrors the KeyboardLayoutUS class from adafruit_hid.
//!
//! This table covers all printable ASCII characters (0x20–0x7E).
//! Characters outside this range return (0, false) — not mapped.

use crate::ducky::keys::Keycode;

/// Map a character to its (keycode, need_shift) pair on a US keyboard.
/// Returns (0, false) for unmapped characters.
pub fn char_to_keycode_us(ch: char) -> (u8, bool) {
    match ch {
        ' '  => (Keycode::SPACE,     false),
        '!'  => (Keycode::N1,        true),
        '"'  => (0x34,              true),   // shift+apostrophe = "
        '#'  => (Keycode::N3,        true),
        '$'  => (Keycode::N4,        true),
        '%'  => (Keycode::N5,        true),
        '&'  => (Keycode::N7,        true),
        '\'' => (0x34,               false),  // apostrophe
        '('  => (Keycode::N9,        true),
        ')'  => (Keycode::N0,        true),
        '*'  => (Keycode::N8,        true),
        '+'  => (0x2E,               true),   // shift+= 
        ','  => (0x36,               false),
        '-'  => (0x2D,               false),
        '.'  => (0x37,               false),
        '/'  => (0x38,               false),
        '0'  => (Keycode::N0,        false),
        '1'  => (Keycode::N1,        false),
        '2'  => (Keycode::N2,        false),
        '3'  => (Keycode::N3,        false),
        '4'  => (Keycode::N4,        false),
        '5'  => (Keycode::N5,        false),
        '6'  => (Keycode::N6,        false),
        '7'  => (Keycode::N7,        false),
        '8'  => (Keycode::N8,        false),
        '9'  => (Keycode::N9,        false),
        ':'  => (0x33,               true),   // shift+;
        ';'  => (0x33,               false),
        '<'  => (0x36,               true),   // shift+,
        '='  => (0x2E,               false),
        '>'  => (0x37,               true),   // shift+.
        '?'  => (0x38,               true),   // shift+/
        '@'  => (Keycode::N2,        true),   // shift+2  (same as ")  — actually shift+' on US
        // Note: On US layout @ = shift+2, but adafruit_hid uses shift+2 for @
        // We match adafruit_hid behaviour exactly here.
        'A'  => (Keycode::A,         true),
        'B'  => (Keycode::B,         true),
        'C'  => (Keycode::C,         true),
        'D'  => (Keycode::D,         true),
        'E'  => (Keycode::E,         true),
        'F'  => (Keycode::F,         true),
        'G'  => (Keycode::G,         true),
        'H'  => (Keycode::H,         true),
        'I'  => (Keycode::I,         true),
        'J'  => (Keycode::J,         true),
        'K'  => (Keycode::K,         true),
        'L'  => (Keycode::L,         true),
        'M'  => (Keycode::M,         true),
        'N'  => (Keycode::N,         true),
        'O'  => (Keycode::O,         true),
        'P'  => (Keycode::P,         true),
        'Q'  => (Keycode::Q,         true),
        'R'  => (Keycode::R,         true),
        'S'  => (Keycode::S,         true),
        'T'  => (Keycode::T,         true),
        'U'  => (Keycode::U,         true),
        'V'  => (Keycode::V,         true),
        'W'  => (Keycode::W,         true),
        'X'  => (Keycode::X,         true),
        'Y'  => (Keycode::Y,         true),
        'Z'  => (Keycode::Z,         true),
        '['  => (0x2F,               false),
        '\\' => (0x31,               false),
        ']'  => (0x30,               false),
        '^'  => (Keycode::N6,        true),   // shift+6
        '_'  => (0x2D,               true),   // shift+-
        '`'  => (0x35,               false),  // backtick
        'a'  => (Keycode::A,         false),
        'b'  => (Keycode::B,         false),
        'c'  => (Keycode::C,         false),
        'd'  => (Keycode::D,         false),
        'e'  => (Keycode::E,         false),
        'f'  => (Keycode::F,         false),
        'g'  => (Keycode::G,         false),
        'h'  => (Keycode::H,         false),
        'i'  => (Keycode::I,         false),
        'j'  => (Keycode::J,         false),
        'k'  => (Keycode::K,         false),
        'l'  => (Keycode::L,         false),
        'm'  => (Keycode::M,         false),
        'n'  => (Keycode::N,         false),
        'o'  => (Keycode::O,         false),
        'p'  => (Keycode::P,         false),
        'q'  => (Keycode::Q,         false),
        'r'  => (Keycode::R,         false),
        's'  => (Keycode::S,         false),
        't'  => (Keycode::T,         false),
        'u'  => (Keycode::U,         false),
        'v'  => (Keycode::V,         false),
        'w'  => (Keycode::W,         false),
        'x'  => (Keycode::X,         false),
        'y'  => (Keycode::Y,         false),
        'z'  => (Keycode::Z,         false),
        '{'  => (0x2F,               true),   // shift+[
        '|'  => (0x31,               true),   // shift+backslash
        '}'  => (0x30,               true),   // shift+]
        '~'  => (0x35,               true),   // shift+backtick
        '\n' => (Keycode::ENTER,     false),
        '\t' => (Keycode::TAB,       false),
        _    => (0,                  false),  // unmapped
    }
}

pub fn map(ch: char) -> (u8, bool) { char_to_keycode_us(ch) }
