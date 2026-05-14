//! ducky/parser.rs — DuckyScript line dispatcher (Step 4: fully wired)
//!
//! Mirrors parseLine() from duckyinpython.py.
//! Every command is now wired to the real HID channel (usb::hid::send)
//! and the real CDC key channel (usb::cdc::KEY_CHANNEL).
//!
//! New: WAIT_FOR_BUTTON accepts an optional timeout in ms.
//!   WAIT_FOR_BUTTON button1           → wait indefinitely
//!   WAIT_FOR_BUTTON button1 2000      → timeout after 2000ms, continue
//!   WAIT_FOR_BUTTON button1 2000 $var → store true/false in $var

use defmt::*;
use embassy_time::{Duration, Timer};
use heapless::String;

use crate::ducky::executor::ScriptContext;
use crate::ducky::keys::{convert_line, key_name_to_code, Keycode};
use crate::ducky::expr::eval_expr;
use crate::layout;
use crate::usb::hid::{self, HidCommand, BTN_LEFT, BTN_RIGHT, BTN_MIDDLE};
use crate::usb::cdc::KEY_CHANNEL;
use crate::hardware::{wait_for_button, EXFIL_LEDS_ENABLED};

// ---------------------------------------------------------------------------
// Main dispatcher
// ---------------------------------------------------------------------------

pub async fn parse_line(line: &str, ctx: &mut ScriptContext) {
    let stripped = line.trim();
    if stripped.is_empty() { return; }

    let upper = to_upper::<64>(stripped);
    let u = upper.as_str();

    // ---- REM / ATTACKMODE ---------------------------------------------------
    if u.starts_with("REM") { return; }
    if u.starts_with("ATTACKMODE") { return; }

    // ---- TYPE_LOOT — read entire loot.bin and type it out --------------------
    // One flash read per 64KB chunk, type directly from FLASH_DATA_BUF
    if u == "TYPE_LOOT" {
        static mut TYPE_CHUNK: [u8; 256] = [0u8; 256];
        let fs = crate::fs::FlashFs::new();
        let mut file_offset = 0usize;
        loop {
            // Read next 64KB chunk from flash
            match fs.read_file_offset_async("loot.bin", file_offset, crate::fs::DATA_BUF_SIZE).await {
                Err(_) | Ok(0) => break,
                Ok(n) => {
                    // Type this chunk 256 bytes at a time from FLASH_DATA_BUF
                    // No more flash ops until this chunk is fully typed
                    let mut pos = 0usize;
                    while pos < n {
                        let take = (n - pos).min(256);
                        unsafe { TYPE_CHUNK[..take].copy_from_slice(&crate::fs::FLASH_DATA_BUF[pos..pos+take]); }
                        if let Ok(text) = core::str::from_utf8(unsafe { &TYPE_CHUNK[..take] }) {
                            send_string(text, ctx).await;
                        }
                        pos += take;
                    }
                    file_offset += n;
                    if n < crate::fs::DATA_BUF_SIZE { break; } // last chunk
                }
            }
        }
        return;
    } // handled before USB starts

    // ---- DELAY --------------------------------------------------------------
    if u.starts_with("DELAY ") {
        let ms = eval_expr(stripped[6..].trim(), ctx).as_int().max(0) as u64;
        jittered_delay(ms, ctx).await;
        return;
    }

    // ---- DEFAULT_DELAY / DEFAULTDELAY ---------------------------------------
    if u.starts_with("DEFAULT_DELAY ") {
        ctx.default_delay = eval_expr(stripped[14..].trim(), ctx).as_int().max(0) as u64;
        return;
    }
    if u.starts_with("DEFAULTDELAY ") {
        ctx.default_delay = eval_expr(stripped[13..].trim(), ctx).as_int().max(0) as u64;
        return;
    }

    // ---- STRINGLN -----------------------------------------------------------
    if u.starts_with("STRINGLN ") {
        let raw = stripped[9..].trim();
        if raw == "$loot.bin" {
            let fs = crate::fs::FlashFs::new();
            if let Ok(len) = fs.read_file_async("loot.bin").await {
                let n = len.min(65536);
                static mut LOOT_TYPE_BUF2: [u8; 4096] = [0u8; 4096];
                let n2 = n.min(4096);
                unsafe { LOOT_TYPE_BUF2[..n2].copy_from_slice(&crate::fs::FLASH_DATA_BUF[..n2]); }
                if let Ok(text) = core::str::from_utf8(unsafe { &LOOT_TYPE_BUF2[..n2] }) {
                    send_string(text, ctx).await;
                }
            }
        } else {
            let text = expand_vars(raw, ctx);
            send_string(text.as_str(), ctx).await;
        }
        hid::send(HidCommand::KeyPress(Keycode::ENTER)).await;
        hid::send(HidCommand::KeyRelease(Keycode::ENTER)).await;
        return;
    }

    // ---- STRING_DELAY -------------------------------------------------------
    if u.starts_with("STRING_DELAY ") {
        let v = eval_expr(stripped[13..].trim(), ctx).as_int();
        ctx.string_char_delay_ms = if v > 0 { Some(v as u64) } else { None };
        return;
    }

    // ---- STRING -------------------------------------------------------------
    if u.starts_with("STRING ") {
        let raw = stripped[7..].trim();
        // Special: STRING $loot.bin — read full loot.bin and type it
        if raw == "$loot.bin" {
            let fs = crate::fs::FlashFs::new();
            if let Ok(len) = fs.read_file_async("loot.bin").await {
                // Copy into SCRIPT_BUF (safe — not in use during STRING execution)
                let n = len.min(65536);
                static mut LOOT_TYPE_BUF: [u8; 4096] = [0u8; 4096];
                let n2 = n.min(4096);
                unsafe { LOOT_TYPE_BUF[..n2].copy_from_slice(&crate::fs::FLASH_DATA_BUF[..n2]); }
                if let Ok(text) = core::str::from_utf8(unsafe { &LOOT_TYPE_BUF[..n2] }) {
                    send_string(text, ctx).await;
                }
            }
            return;
        }
        let text = expand_vars(raw, ctx);
        send_string(text.as_str(), ctx).await;
        return;
    }

    // ---- HOLD ---------------------------------------------------------------
    if u.starts_with("HOLD ") {
        let key = stripped[5..].trim();
        if let Some(code) = key_name_to_code(to_upper::<32>(key).as_str()) {
            hid::send(HidCommand::KeyPress(code)).await;
            ctx.held_keys.push(code).ok();
            info!("[HOLD] {}", key);
        } else {
            warn!("[HOLD] Unknown key: {}", key);
        }
        return;
    }

    // ---- RELEASE ------------------------------------------------------------
    if u.starts_with("RELEASE ") {
        let key = stripped[8..].trim();
        if to_upper::<8>(key).as_str() == "ALL" {
            hid::send(HidCommand::KeyReleaseAll).await;
            ctx.held_keys.clear();
            info!("[RELEASE] ALL");
        } else if let Some(code) = key_name_to_code(to_upper::<32>(key).as_str()) {
            hid::send(HidCommand::KeyRelease(code)).await;
            ctx.held_keys.retain(|&k| k != code);
        } else {
            warn!("[RELEASE] Unknown key: {}", key);
        }
        return;
    }

    // ---- DUCKY_LANG ---------------------------------------------------------
    if u.starts_with("DUCKY_LANG") {
        let lang = stripped[10..].trim();
        if lang.is_empty() {
            info!("[DUCKY_LANG] active: {}", ctx.active_layout.as_str());
        } else {
            layout::switch_layout(lang, ctx);
        }
        return;
    }

    // ---- MOUSE --------------------------------------------------------------
    if u.starts_with("MOUSE ") {
        parse_mouse_command(&stripped[6..]).await;
        return;
    }

    // ---- CC (Consumer Control / media keys) ---------------------------------
    if u.starts_with("CC ") {
        if let Some(usage) = cc_name_to_usage(stripped[3..].trim()) {
            hid::send(HidCommand::ConsumerSend(usage)).await;
        }
        return;
    }

    // ---- PRINT --------------------------------------------------------------
    if u.starts_with("PRINT ") {
        let msg = expand_vars(&stripped[6..], ctx);
        info!("[SCRIPT]: {}", msg.as_str());
        // TODO Step 5: write to CDC console port
        return;
    }

    // ---- IMPORT -------------------------------------------------------------
    if u.starts_with("IMPORT ") {
        // IMPORT is logged but not executed recursively (would cause infinite future size).
        // To use IMPORT, combine scripts manually or use FUNCTION instead.
        warn!("[IMPORT] {} — use FUNCTION for reusable blocks in ducky-rs", stripped[7..].trim());
        return;
    }

    // ---- LED ----------------------------------------------------------------
    if u.trim() == "LED" {
        ctx.led_state = !ctx.led_state;
        EXFIL_LEDS_ENABLED.signal(ctx.led_state);
        info!("[LED] toggled → {}", ctx.led_state);
        return;
    }

    // ---- SAVE_HOST_KEYBOARD_STATE -------------------------------------------
    if u.trim() == "SAVE_HOST_KEYBOARD_STATE" {
        let leds = hid::get_led_state().await;
        ctx.saved_num_lock    = (leds & 0x01) != 0;
        ctx.saved_caps_lock   = (leds & 0x02) != 0;
        ctx.saved_scroll_lock = (leds & 0x04) != 0;
        info!("[SAVE_HOST_KBD_STATE] caps={} num={} scroll={}",
            ctx.saved_caps_lock, ctx.saved_num_lock, ctx.saved_scroll_lock);
        return;
    }

    // ---- RESTORE_HOST_KEYBOARD_STATE ----------------------------------------
    if u.trim() == "RESTORE_HOST_KEYBOARD_STATE" {
        let leds = hid::get_led_state().await;
        if ((leds & 0x02) != 0) != ctx.saved_caps_lock {
            press_release(Keycode::CAPS_LOCK).await;
        }
        if ((leds & 0x01) != 0) != ctx.saved_num_lock {
            press_release(Keycode::KEYPAD_NUMLOCK).await;
        }
        if ((leds & 0x04) != 0) != ctx.saved_scroll_lock {
            press_release(Keycode::SCROLL_LOCK).await;
        }
        return;
    }

    // ---- WAIT_FOR_BUTTON ----------------------------------------------------
    //
    // Syntax:
    //   WAIT_FOR_BUTTON button1
    //   WAIT_FOR_BUTTON button1 <timeout_ms>
    //   WAIT_FOR_BUTTON button1 <timeout_ms> $var
    //
    // timeout_ms  = 0 or omitted → wait indefinitely (original behaviour)
    // $var        = optional variable to store TRUE (pressed) / FALSE (timeout)
    //
    // The elapsed press duration is also available — it's stored as
    // $_BUTTON_ELAPSED_MS in the script context after each call.
    if u.starts_with("WAIT_FOR_BUTTON") {
        let rest: heapless::Vec<&str, 4> = stripped.splitn(4, ' ').collect();
        // rest[0] = "WAIT_FOR_BUTTON", rest[1] = "button1",
        // rest[2] = optional timeout_ms, rest[3] = optional $var

        let timeout_ms: Option<u64> = rest.get(2)
            .and_then(|s| {
                let v = eval_expr(s, ctx).as_int();
                if v > 0 { Some(v as u64) } else { None }
            });

        let save_var: Option<&str> = rest.get(3).copied().filter(|s| s.starts_with('$'));

        info!("[WAIT_FOR_BUTTON] waiting (timeout={:?}ms)...", timeout_ms);

        let (pressed, elapsed_ms) = wait_for_button(timeout_ms).await;

        // Store the elapsed time as a built-in variable
        ctx.set_var_int("$_BUTTON_ELAPSED_MS", elapsed_ms as i32);

        // Store pressed state in user variable if requested
        if let Some(var) = save_var {
            ctx.set_var_bool(var, pressed);
        }

        info!("[WAIT_FOR_BUTTON] pressed={} elapsed={}ms", pressed, elapsed_ms);
        return;
    }

    // ---- WAIT_FOR_KEY -------------------------------------------------------
    //
    // Syntax:
    //   WAIT_FOR_KEY                          → wait forever, discard key
    //   WAIT_FOR_KEY $var                     → wait forever, store in $var
    //   WAIT_FOR_KEY $var 5000                → wait up to 5000ms, store in $var
    //   WAIT_FOR_KEY $var 5000 $timed_out     → also store true/false in $timed_out
    if u.starts_with("WAIT_FOR_KEY") {
        let rest = stripped["WAIT_FOR_KEY".len()..].trim();
        let mut parts = rest.splitn(3, ' ');
        let save_var    = parts.next().filter(|s| s.starts_with('$'));
        let timeout_ms  = parts.next().and_then(|s| s.parse::<u64>().ok());
        let timeout_var = parts.next().filter(|s| s.starts_with('$'));

        info!("[WAIT_FOR_KEY] waiting (timeout={:?}ms)...", timeout_ms);

        let key_str = if let Some(ms) = timeout_ms {
            use embassy_futures::select::{select, Either};
            use embassy_time::{Duration, Timer};
            match select(KEY_CHANNEL.receive(), Timer::after(Duration::from_millis(ms))).await {
                Either::First(k) => {
                    if let Some(var) = timeout_var { ctx.set_var_bool(var, false); }
                    Some(k)
                }
                Either::Second(_) => {
                    info!("[WAIT_FOR_KEY] timed out after {}ms", ms);
                    if let Some(var) = timeout_var { ctx.set_var_bool(var, true); }
                    None
                }
            }
        } else {
            Some(KEY_CHANNEL.receive().await)
        };

        if let Some(k) = key_str {
            info!("[WAIT_FOR_KEY] received: {}", k.as_str());
            if let Some(var) = save_var { ctx.set_var_str(var, k.as_str()); }
        } else if let Some(var) = save_var {
            ctx.set_var_str(var, "");
        }
        return;
    }

    // ---- Bare key combo (e.g. "GUI r", "CTRL ALT DELETE") ------------------
    run_script_line(stripped).await;
}

// ---------------------------------------------------------------------------
// String typing
// ---------------------------------------------------------------------------
//
// Mirrors sendString() + layout.write(char) from duckyinpython.py.
// Each character is dispatched as a TypeChar command to the HID task.

async fn send_string(text: &str, ctx: &ScriptContext) {
    for ch in text.chars() {
        let (keycode, need_shift) = layout::char_to_keycode(ch, ctx.active_layout.as_str());
        if keycode == 0 {
            warn!("[STRING] unmapped char U+{:04X}", ch as u32);
            continue;
        }
        hid::send(HidCommand::TypeChar { keycode, shift: need_shift }).await;

        // Per-character delay
        let delay_ms = ctx.string_char_delay_ms.unwrap_or_else(|| {
            if ch.is_ascii_uppercase() || "!@#$%^&*()_+-\"|{}|~".contains(ch) { 80 } else { 30 }
        });
        Timer::after(Duration::from_millis(delay_ms)).await;
    }
}

// ---------------------------------------------------------------------------
// Key combo dispatcher
// ---------------------------------------------------------------------------
//
// Mirrors runScriptLine() + convertLine() from duckyinpython.py.

async fn run_script_line(line: &str) {
    let codes = convert_line(line);
    if codes.is_empty() { return; }
    hid::send(HidCommand::KeyCombo(codes)).await;
}

async fn press_release(keycode: u8) {
    hid::send(HidCommand::KeyPress(keycode)).await;
    hid::send(HidCommand::KeyRelease(keycode)).await;
}

// ---------------------------------------------------------------------------
// MOUSE command parser
// ---------------------------------------------------------------------------
//
// Mirrors parseMouseCommand() from duckyinpython.py.

async fn parse_mouse_command(cmd: &str) {
    let upper = to_upper::<64>(cmd);
    let u = upper.as_str();

    if u.starts_with("CLICK LEFT")        { hid::send(HidCommand::MouseClick(BTN_LEFT)).await; }
    else if u.starts_with("CLICK RIGHT")  { hid::send(HidCommand::MouseClick(BTN_RIGHT)).await; }
    else if u.starts_with("CLICK MIDDLE") { hid::send(HidCommand::MouseClick(BTN_MIDDLE)).await; }
    else if u.starts_with("PRESS LEFT")   { hid::send(HidCommand::MousePress(BTN_LEFT)).await; }
    else if u.starts_with("PRESS RIGHT")  { hid::send(HidCommand::MousePress(BTN_RIGHT)).await; }
    else if u.starts_with("RELEASE LEFT") { hid::send(HidCommand::MouseRelease(BTN_LEFT)).await; }
    else if u.starts_with("RELEASE RIGHT"){ hid::send(HidCommand::MouseRelease(BTN_RIGHT)).await; }
    else if u.starts_with("MOVE ") {
        let parts: heapless::Vec<&str, 2> = cmd[5..].splitn(2, ',').collect();
        if parts.len() == 2 {
            let x: i8 = parts[0].trim().parse().unwrap_or(0);
            let y: i8 = parts[1].trim().parse().unwrap_or(0);
            hid::send(HidCommand::MouseMove { x, y }).await;
        }
    }
    else if u.starts_with("WHEEL ") {
        let w: i8 = cmd[6..].trim().parse().unwrap_or(0);
        hid::send(HidCommand::MouseWheel(w)).await;
    }
    else { warn!("[MOUSE] Unknown command: {}", cmd); }
}

// ---------------------------------------------------------------------------
// Consumer Control name → USB usage ID
// ---------------------------------------------------------------------------
//
// Mirrors the CC command handler, using actual HID Usage IDs from the
// USB HID Usage Tables spec (Consumer Page 0x0C).

fn cc_name_to_usage(name: &str) -> Option<u16> {
    match to_upper::<32>(name).as_str() {
        "MUTE"                  => Some(0x00E2),
        "VOLUME_INCREMENT"      => Some(0x00E9),
        "VOLUME_DECREMENT"      => Some(0x00EA),
        "PLAY_PAUSE"            => Some(0x00CD),
        "SCAN_NEXT_TRACK"       => Some(0x00B5),
        "SCAN_PREVIOUS_TRACK"   => Some(0x00B6),
        "STOP"                  => Some(0x00B7),
        "EJECT"                 => Some(0x00B8),
        "BRIGHTNESS_INCREMENT"  => Some(0x006F),
        "BRIGHTNESS_DECREMENT"  => Some(0x0070),
        "FAST_FORWARD"          => Some(0x00B3),
        "REWIND"                => Some(0x00B4),
        other => { warn!("[CC] Unknown: {}", other); None }
    }
}

// ---------------------------------------------------------------------------
// Jitter delay (shared with executor.rs via pub)
// ---------------------------------------------------------------------------
//
// Mirrors _jitteredDelay() from duckyinpython.py.

pub async fn jittered_delay(ms: u64, ctx: &ScriptContext) {
    let mut total = ms;
    if ctx.jitter_enabled {
        let extra = crate::ducky::expr::rand_range(0, ctx.jitter_max_delay as i32) as u64;
        total += extra;
    }
    if total > 0 {
        Timer::after(Duration::from_millis(total)).await;
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn expand_vars(text: &str, ctx: &ScriptContext) -> String<128> {
    crate::ducky::expr::substitute_vars(text, ctx)
}

fn to_upper<const N: usize>(s: &str) -> String<N> {
    let mut out: String<N> = String::new();
    for c in s.chars().take(N) { let _ = out.push(c.to_ascii_uppercase()); }
    out
}
