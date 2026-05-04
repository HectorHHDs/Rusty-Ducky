//! layout/fr.rs — French (AZERTY) keyboard layout
//!
//! Key differences from US:
//!   a/q and z/w swapped (AZERTY arrangement)
//!   Digits require shift on FR (unshifted = &é"'(-è_çà)
//!   Many symbols in different positions
//!
//! Reference: Windows French keyboard layout (keyboard_layout_win_fr)

use crate::ducky::keys::Keycode;

pub fn map(ch: char) -> (u8, bool) {
    match ch {
        '\t'  => (Keycode::TAB,   false),
        '\n'  => (Keycode::ENTER, false),
        ' '   => (Keycode::SPACE, false),

        // ── Digits (need Shift on FR to get numeric digit) ─────────────────
        '0'   => (Keycode::N0, true),
        '1'   => (Keycode::N1, true),
        '2'   => (Keycode::N2, true),
        '3'   => (Keycode::N3, true),
        '4'   => (Keycode::N4, true),
        '5'   => (Keycode::N5, true),
        '6'   => (Keycode::N6, true),
        '7'   => (Keycode::N7, true),
        '8'   => (Keycode::N8, true),
        '9'   => (Keycode::N9, true),

        // ── French unshifted digit row characters ──────────────────────────
        '&'   => (Keycode::N1, false),
        'é'   => (Keycode::N2, false),
        '"'   => (Keycode::N3, false),
        '\''  => (Keycode::N4, false),
        '('   => (Keycode::N5, false),
        '-'   => (Keycode::N6, false),
        'è'   => (Keycode::N7, false),
        '_'   => (Keycode::N8, false),
        'ç'   => (Keycode::N9, false),
        'à'   => (Keycode::N0, false),
        ')'   => (0x2D, false),   // US minus position → ) on FR
        '='   => (0x2D, true),

        // ── Letters — AZERTY swaps ─────────────────────────────────────────
        // q ↔ a  and  w ↔ z
        'a'   => (Keycode::Q, false), 'A' => (Keycode::Q, true),
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
        'm'   => (0x33, false),       'M' => (0x33, true),  // FR: M on semicolon key
        'n'   => (Keycode::N, false), 'N' => (Keycode::N, true),
        'o'   => (Keycode::O, false), 'O' => (Keycode::O, true),
        'p'   => (Keycode::P, false), 'P' => (Keycode::P, true),
        'q'   => (Keycode::A, false), 'Q' => (Keycode::A, true),  // AZERTY: Q key = A
        'r'   => (Keycode::R, false), 'R' => (Keycode::R, true),
        's'   => (Keycode::S, false), 'S' => (Keycode::S, true),
        't'   => (Keycode::T, false), 'T' => (Keycode::T, true),
        'u'   => (Keycode::U, false), 'U' => (Keycode::U, true),
        'v'   => (Keycode::V, false), 'V' => (Keycode::V, true),
        'w'   => (Keycode::Z, false), 'W' => (Keycode::Z, true),  // AZERTY: W key = Z
        'x'   => (Keycode::X, false), 'X' => (Keycode::X, true),
        'y'   => (Keycode::Y, false), 'Y' => (Keycode::Y, true),
        'z'   => (Keycode::W, false), 'Z' => (Keycode::W, true),  // AZERTY: Z key = W

        // ── Symbols ────────────────────────────────────────────────────────
        '!'   => (0x38, true),   // FR: ! on shift+/ (US slash key)
        ':'   => (0x2E, true),   // FR: : on shift+. equivalent
        ';'   => (0x36, false),  // FR: ; unshifted
        '.'   => (0x36, true),   // FR: . on shift+;
        ','   => (Keycode::M, false),  // FR: , on M key
        '?'   => (Keycode::M, true),   // FR: ? shift+,
        '/'   => (0x37, true),   // FR: / shift+.
        '+'   => (0x2E, false),  // FR: + unshifted
        '*'   => (0x30, false),
        '%'   => (Keycode::U, true),   // shift+U on some FR variants

        // Accented vowels common in French
        'ê'   => (0x5B, false),  // dead circumflex sequences — best effort
        'î'   => (0x5B, false),
        'â'   => (0x5B, false),
        'ô'   => (0x5B, false),
        'û'   => (0x5B, false),
        'ë'   => (0x5B, true),
        'ï'   => (0x5B, true),
        'ü'   => (0x5B, true),
        'ù'   => (0x2F, false),  // FR: ù on [ key
        'œ'   => (0, false),     // not directly typeable on standard FR keyboard
        '«'   => (0x34, false),
        '»'   => (0x34, true),

        _     => (0, false),
    }
}
