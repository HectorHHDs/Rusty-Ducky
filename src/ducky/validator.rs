//! ducky/validator.rs — Pre-execution syntax check
//!
//! Validates a DuckyScript payload and returns a list of errors with line numbers.

use heapless::{String, Vec};

pub struct SyntaxError {
    pub line: usize,
    pub msg:  String<64>,
}

// Known valid command prefixes
const VALID_COMMANDS: &[&str] = &[
    "REM", "ATTACKMODE", "DELAY", "DEFAULT_DELAY", "DEFAULTDELAY",
    "STRING", "STRINGLN", "STRINGLN_HOLD", "HOLD",
    "GUI", "WIN", "CTRL", "ALT", "SHIFT", "APP", "MENU",
    "UP", "DOWN", "LEFT", "RIGHT", "HOME", "END", "INSERT", "DELETE",
    "PAGEUP", "PAGEDOWN", "PGUP", "PGDN",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8",
    "F9", "F10", "F11", "F12",
    "SPACE", "TAB", "ENTER", "ESCAPE", "ESC", "BACKSPACE", "CAPSLOCK",
    "NUMLOCK", "SCROLLLOCK", "PRINTSCREEN", "PAUSE", "BREAK",
    "MOUSE", "LED",
    "WAIT_FOR_KEY", "WAIT_FOR_BUTTON",
    "STRING_DELAY",
    "VAR", "DEFINE", "IF", "ELSE", "END_IF", "WHILE", "END_WHILE",
    "FOR", "END_FOR", "FUNCTION", "END_FUNCTION", "RETURN",
    "REPEAT", "RANDOMIZE", "JITTER",
    "RESTART_PAYLOAD", "STOP_PAYLOAD",
    "SAVE_HOST_KEYBOARD_LOCK_STATE", "RESTORE_HOST_KEYBOARD_LOCK_STATE",
    "RELEASE", "HOLD",
    "EXFIL_MODE_ENABLE", "EXFIL_MODE_DISABLE", "TYPE_LOOT",
    "IMPORT",
    // Single-key commands (no args)
    "WINDOWS",
];

pub fn validate(text: &str) -> Vec<SyntaxError, 8> {
    let mut errors: Vec<SyntaxError, 8> = Vec::new();

    for (line_num, line) in text.lines().enumerate() {
        let stripped = line.trim();
        if stripped.is_empty() { continue; }

        // Get first word
        let first = stripped.split_whitespace().next().unwrap_or("");
        let first_upper = to_upper_16(first);

        // Skip REM, ATTACKMODE, bare variable assignments, and known commands
        if first_upper.as_str() == "REM"
            || first_upper.as_str() == "ATTACKMODE"
            || stripped.starts_with('$')
            || first_upper.as_str().starts_with('$') {
            continue;
        }

        // Check if it's a known command
        let known = VALID_COMMANDS.iter().any(|&c| {
            c.len() == first_upper.len() &&
            c.as_bytes().iter().zip(first_upper.as_bytes()).all(|(a,b)| a == b)
        });

        if !known {
            if errors.is_full() { break; }
            let mut msg: String<64> = String::new();
            let _ = msg.push_str("line ");
            push_num(&mut msg, line_num + 1);
            let _ = msg.push_str(": unknown command '");
            let _ = msg.push_str(&first_upper.as_str()[..first_upper.len().min(20)]);
            let _ = msg.push('\'');
            let _ = errors.push(SyntaxError { line: line_num + 1, msg });
        }

        // Check STRING/STRINGLN have an argument
        if (first_upper.as_str() == "STRING" || first_upper.as_str() == "STRINGLN")
            && stripped[first.len()..].trim().is_empty()
        {
            if errors.is_full() { break; }
            let mut msg: String<64> = String::new();
            let _ = msg.push_str("line ");
            push_num(&mut msg, line_num + 1);
            let _ = msg.push_str(": ");
            let _ = msg.push_str(first_upper.as_str());
            let _ = msg.push_str(" requires an argument");
            let _ = errors.push(SyntaxError { line: line_num + 1, msg });
        }

        // Check DELAY has a numeric argument
        if first_upper.as_str() == "DELAY" {
            let rest = stripped[first.len()..].trim();
            if rest.is_empty() || (!rest.starts_with('$') && rest.parse::<u64>().is_err()) {
                if errors.is_full() { break; }
                let mut msg: String<64> = String::new();
                let _ = msg.push_str("line ");
                push_num(&mut msg, line_num + 1);
                let _ = msg.push_str(": DELAY requires a numeric argument");
                let _ = errors.push(SyntaxError { line: line_num + 1, msg });
            }
        }
    }

    errors
}

fn to_upper_16(s: &str) -> String<16> {
    let mut out: String<16> = String::new();
    for c in s.chars().take(16) { let _ = out.push(c.to_ascii_uppercase()); }
    out
}

fn push_num(s: &mut String<64>, n: usize) {
    // Simple number to string
    if n == 0 { let _ = s.push('0'); return; }
    let mut buf = [0u8; 10];
    let mut i = 10usize;
    let mut v = n;
    while v > 0 { i -= 1; buf[i] = b'0' + (v % 10) as u8; v /= 10; }
    for &b in &buf[i..] { let _ = s.push(b as char); }
}
