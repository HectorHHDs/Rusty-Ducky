//! layout/de.rs — German (DE) keyboard layout
//!
//! Key differences from US layout:
//!   y ↔ z swapped
//!   Umlauts: ä=0x34, ö=0x33, ü=0x2F (unshifted apostrophe/semicolon/bracket positions)
//!   @ = AltGr+Q (0xE6 + Q) — handled as dead key; for scripts use clipboard paste
//!   Special symbols shifted differently from US
//!
//! This table covers printable ASCII + common German characters.
//! Characters not on a standard DE keyboard return (0, false).
//!
//! Reference: Windows German keyboard layout (keyboard_layout_win_de)

use crate::ducky::keys::Keycode;

pub fn map(ch: char) -> (u8, bool) {
    match ch {
        // ── Unchanged from US ──────────────────────────────────────────────
        '\t'  => (Keycode::TAB,   false),
        '\n'  => (Keycode::ENTER, false),
        ' '   => (Keycode::SPACE, false),

        // ── Digits (same position, different shift symbols) ────────────────
        '1'   => (Keycode::N1, false),
        '2'   => (Keycode::N2, false),
        '3'   => (Keycode::N3, false),
        '4'   => (Keycode::N4, false),
        '5'   => (Keycode::N5, false),
        '6'   => (Keycode::N6, false),
        '7'   => (Keycode::N7, false),
        '8'   => (Keycode::N8, false),
        '9'   => (Keycode::N9, false),
        '0'   => (Keycode::N0, false),

        // ── Letters — note y/z swap ────────────────────────────────────────
        'a'   => (Keycode::A, false), 'A' => (Keycode::A, true),
        'b'   => (Keycode::B, false), 'B' => (Keycode::B, true),
        'c'   => (Keycode::C, false), 'C' => (Keycode::C, true),
        'd'   => (Keycode::D, false), 'D' => (Keycode::D, true),
        'e'   => (Keycode::E, false), 'E' => (Keycode::E, true),
        'f'   => (Keycode::F, false), 'F' => (Keycode::F, true),
        'g'   => (Keycode::G, false), 'G' => (Keycode::G, true),
        'h'   => (Keycode::H, false), 'H' => (Keycode::H, true),
        'i'   => (Keycode::I, false), 'I' => (Keycode::I, true),
        'j'   => (Keycode::J, false), 'J' => (Keycode::J, true),
        'k'   => (Keycode::K, false), 'K' => (Keycode::K, true),
        'l'   => (Keycode::L, false), 'L' => (Keycode::L, true),
        'm'   => (Keycode::M, false), 'M' => (Keycode::M, true),
        'n'   => (Keycode::N, false), 'N' => (Keycode::N, true),
        'o'   => (Keycode::O, false), 'O' => (Keycode::O, true),
        'p'   => (Keycode::P, false), 'P' => (Keycode::P, true),
        'q'   => (Keycode::Q, false), 'Q' => (Keycode::Q, true),
        'r'   => (Keycode::R, false), 'R' => (Keycode::R, true),
        's'   => (Keycode::S, false), 'S' => (Keycode::S, true),
        't'   => (Keycode::T, false), 'T' => (Keycode::T, true),
        'u'   => (Keycode::U, false), 'U' => (Keycode::U, true),
        'v'   => (Keycode::V, false), 'V' => (Keycode::V, true),
        'w'   => (Keycode::W, false), 'W' => (Keycode::W, true),
        'x'   => (Keycode::X, false), 'X' => (Keycode::X, true),
        // y and z are swapped on DE layout
        'y'   => (Keycode::Z, false), 'Y' => (Keycode::Z, true),
        'z'   => (Keycode::Y, false), 'Z' => (Keycode::Y, true),

        // ── Symbols ───────────────────────────────────────────────────────
        // DE positions differ from US for most symbols.
        // Keycode values below reference HID Usage IDs for the DE key positions.
        '!'   => (Keycode::N1, true),
        '"'   => (Keycode::N2, true),
        // § is shift+3 on DE
        '$'   => (Keycode::N4, true),
        '%'   => (Keycode::N5, true),
        '&'   => (Keycode::N6, true),
        '/'   => (Keycode::N7, true),
        '('   => (Keycode::N8, true),
        ')'   => (Keycode::N9, true),
        '='   => (Keycode::N0, true),

        // DE: ß = key right of 0 (0x2D in HID = minus position on US)
        'ß'   => (0x2D, false),
        '?'   => (0x2D, true),

        // DE: ` position (US backtick) → dead acute on DE, skip

        // DE: - is on shift+/ position (0x38 in US is slash)
        '-'   => (0x38, false),
        '_'   => (0x38, true),

        // DE: . and , same
        '.'   => (0x37, false),
        ':'   => (0x37, true),
        ','   => (0x36, false),
        ';'   => (0x36, true),

        // DE: < > are on the extra key between LShift and Y (0x64 = non-US backslash)
        '<'   => (0x64, false),
        '>'   => (0x64, true),

        // DE: # and ' are on key 0x31 (US backslash)
        '#'   => (0x31, false),
        '\''  => (0x31, true),

        // DE: + * on key 0x30 (US right bracket)
        '+'   => (0x30, false),
        '*'   => (0x30, true),

        // DE: ü on key 0x2F (US left bracket)
        'ü'   => (0x2F, false),
        'Ü'   => (0x2F, true),

        // DE: ö on key 0x33 (US semicolon)
        'ö'   => (0x33, false),
        'Ö'   => (0x33, true),

        // DE: ä on key 0x34 (US apostrophe)
        'ä'   => (0x34, false),
        'Ä'   => (0x34, true),

        // DE: ^ on key 0x35 (US grave/backtick) — circumflex
        '^'   => (0x35, false),

        // Unmapped
        _     => (0, false),
    }
}
