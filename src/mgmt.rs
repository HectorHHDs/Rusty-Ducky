//! mgmt.rs -- Serial management console (ttyACM0)
//!
//! Commands:
//!   list                   -- list stored payloads
//!   run <name>             -- queue payload to run
//!   get <name>             -- show payload contents
//!   put <name>             -- upload payload (line by line, type END to finish)
//!   del <name>             -- delete payload
//!   format                 -- erase all payloads
//!   reboot                 -- reboot device
//!   modes                  -- show supported ATTACKMODE values and current mode
//!   keys                   -- show buffered keys from key_listener port
//!   exfil                  -- show loot.bin contents and byte count
//!   exfil clear            -- delete loot.bin
//!   help                   -- show this help

use defmt::*;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_time::{Duration, Timer};

use crate::hardware::SCRIPT_SIGNAL;

type Console = CdcAcmClass<'static, Driver<'static, USB>>;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn mgmt_task_inner(mut console: Console) {
    info!("[mgmt] ready");
    loop {
        console.wait_connection().await;
        info!("[mgmt] connected");
        drain_logs(&mut console).await;
        send(&mut console, b"ducky-rs> ").await;

        let mut rx   = [0u8; 64];
        let mut line = [0u8; 64];
        let mut len  = 0usize;

        loop {
            let n = match console.read_packet(&mut rx).await {
                Ok(n)  => n,
                Err(_) => break,
            };
            for i in 0..n {
                match rx[i] {
                    b'\r' | b'\n' => {
                        send(&mut console, b"\r\n").await;
                        if len > 0 {
                            if let Ok(cmd) = core::str::from_utf8(&line[..len]) {
                                handle_cmd(&mut console, cmd).await;
                            }
                            drain_logs(&mut console).await;
                            send(&mut console, b"ducky-rs> ").await;
                            len = 0;
                        }
                    }
                    // Backspace
                    0x08 | 0x7F => {
                        if len > 0 {
                            len -= 1;
                            send(&mut console, b"\x08 \x08").await;
                        }
                    }
                    // Ignore escape sequences
                    0x1B => {}
                    // Printable ASCII
                    b if b >= 0x20 && b < 0x7F => {
                        if len < 63 {
                            line[len] = b;
                            len += 1;
                            send(&mut console, &[b]).await;
                        }
                    }
                    _ => {}
                }
            }
        }
        info!("[mgmt] disconnected");
    }
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

async fn handle_cmd(console: &mut Console, line: &str) {
    let line = line.trim();
    if line.is_empty() { return; }

    // Split into command and argument
    let (cmd, arg) = match line.find(' ') {
        Some(i) => (&line[..i], line[i+1..].trim()),
        None    => (line, ""),
    };

    // Case-insensitive command matching
    let mut lc = [0u8; 8];
    let cb = cmd.as_bytes();
    for i in 0..cb.len().min(8) { lc[i] = cb[i].to_ascii_lowercase(); }
    let cmd_lc = core::str::from_utf8(&lc[..cb.len().min(8)]).unwrap_or("");

    match cmd_lc {
        "list"   => cmd_list(console).await,
        "run"    => cmd_run(console, arg).await,
        "get"    => cmd_get(console, arg).await,
        "put"    => cmd_put(console, arg).await,
        "del"    => cmd_del(console, arg).await,
        "format" => cmd_format(console).await,
        "reboot" => cmd_reboot(console).await,
        "modes"  => cmd_modes(console).await,
        "keys"   => cmd_keys(console).await,
        "exfil"  => cmd_exfil(console, arg).await,
        "help"   => cmd_help(console).await,
        _        => send(console, b"Unknown command (type help)\r\n").await,
    }
}

// ---------------------------------------------------------------------------
// Individual command handlers
// ---------------------------------------------------------------------------

async fn cmd_list(console: &mut Console) {
    let fs = crate::fs::FlashFs::new();
    let files = fs.list_dd_files_async().await;
    if files.is_empty() {
        send(console, b"(no files)\r\n").await;
    } else {
        for f in &files {
            send(console, f.as_bytes()).await;
            send(console, b"\r\n").await;
        }
    }
}

async fn cmd_run(console: &mut Console, arg: &str) {
    if arg.is_empty() {
        send(console, b"Usage: run <payload.dd|payload2.dd|payload3.dd>\r\n").await;
        return;
    }

    // Read a preview to check ATTACKMODE and syntax before queuing
    let fs = crate::fs::FlashFs::new();
    static mut CHECK_BUF: [u8; 1024] = [0u8; 1024];
    static mut CHECK_LEN: usize = 0;

    if let Ok(len) = fs.read_file_async(arg).await {
        let n = len.min(1024);
        unsafe {
            CHECK_BUF[..n].copy_from_slice(&crate::fs::FLASH_DATA_BUF[..n]);
            CHECK_LEN = n;
        }
        if let Ok(text) = core::str::from_utf8(unsafe { &CHECK_BUF[..CHECK_LEN] }) {
            let sd = crate::usb::msc_sd::sd_available();
            let requested = crate::attackmode::parse_requested(text);

            if requested.has_storage() && !sd {
                send(console, b"[!] ATTACKMODE STORAGE not available -- no SD card\r\n").await;
                send(console, b"[!] Payload will run in current HID CDC mode\r\n").await;
            } else {
                let active = crate::attackmode::get();
                // STORAGE changes require new USB descriptors -- need a reboot
                if requested.has_storage() != active.has_storage() {
                    send(console, b"[!] ATTACKMODE STORAGE change -- rebooting to apply...\r\n").await;
                    Timer::after(Duration::from_millis(300)).await;
                    cortex_m::peripheral::SCB::sys_reset();
                }
            }

            // Syntax check
            let errs = crate::ducky::validator::validate(text);
            if !errs.is_empty() {
                send(console, b"[!] Syntax errors:\r\n").await;
                for e in &errs {
                    send(console, b"  ").await;
                    send(console, e.msg.as_bytes()).await;
                    send(console, b"\r\n").await;
                }
            }
        }
    }

    // Map arg to a static string (required for the signal channel)
    let name: &'static str = match arg {
        "payload.dd"  => "payload.dd",
        "payload2.dd" => "payload2.dd",
        "payload3.dd" => "payload3.dd",
        _ => {
            send(console, b"Usage: run <payload.dd|payload2.dd|payload3.dd>\r\n").await;
            return;
        }
    };

    match SCRIPT_SIGNAL.try_send(name) {
        Ok(())  => send(console, b"Queued\r\n").await,
        Err(_)  => send(console, b"Queue full, try again\r\n").await,
    }
}

async fn cmd_get(console: &mut Console, arg: &str) {
    if arg.is_empty() {
        send(console, b"Usage: get <payload.dd>\r\n").await;
        return;
    }
    let fs = crate::fs::FlashFs::new();
    match fs.read_file_async(arg).await {
        Err(_) => send(console, b"Not found\r\n").await,
        Ok(len) => {
            let data = unsafe { &crate::fs::FLASH_DATA_BUF[..len] };
            match core::str::from_utf8(data) {
                Ok(text) => {
                    for line in text.lines() {
                        send(console, line.as_bytes()).await;
                        send(console, b"\r\n").await;
                    }
                }
                Err(_) => send(console, b"(binary)\r\n").await,
            }
        }
    }
}

async fn cmd_put(console: &mut Console, arg: &str) {
    let name: &'static str = match arg {
        "payload.dd"  => "payload.dd",
        "payload2.dd" => "payload2.dd",
        "payload3.dd" => "payload3.dd",
        _ => {
            send(console, b"Usage: put <payload.dd|payload2.dd|payload3.dd>\r\n").await;
            return;
        }
    };

    send(console, b"Erasing slot...\r\n").await;
    let fs = crate::fs::FlashFs::new();
    let slot = match fs.stream_begin_async(name).await {
        Ok(s)  => s,
        Err(e) => {
            send(console, b"Error: ").await;
            send(console, e.as_bytes()).await;
            send(console, b"\r\n").await;
            return;
        }
    };
    send(console, b"Send content, type END on its own line to finish:\r\n").await;

    static mut LINE_BUF: [u8; 1024] = [0u8; 1024];
    static mut LINE_LEN: usize      = 0;
    unsafe { LINE_LEN = 0; }

    let mut page_buf = [0xFFu8; 256];
    let mut page_pos = 0usize;
    let mut page_idx = 0usize;
    let mut total    = 0usize;
    let mut rx       = [0u8; 64];
    let mut done     = false;

    'upload: loop {
        let n = match console.read_packet(&mut rx).await {
            Ok(n)  => n,
            Err(_) => break,
        };

        for i in 0..n {
            match rx[i] {
                b'\r' | b'\n' => {
                    send(console, b"\r\n").await;
                    let ll = unsafe { LINE_LEN };
                    if ll > 0 {
                        let line_str = unsafe { core::str::from_utf8(&LINE_BUF[..ll]).unwrap_or("") };

                        // END terminates the upload
                        if line_str.trim().eq_ignore_ascii_case("end") {
                            done = true;
                            break 'upload;
                        }

                        // Append line + newline to page buffer, flushing full pages
                        for &b in line_str.as_bytes().iter().chain(b"\n".iter()) {
                            page_buf[page_pos] = b;
                            page_pos += 1;
                            total    += 1;

                            if page_pos == 256 {
                                unsafe { crate::fs::PAGE_BUF.copy_from_slice(&page_buf); }
                                if let Err(e) = fs.stream_chunk_async(slot, page_idx).await {
                                    send(console, b"Write error: ").await;
                                    send(console, e.as_bytes()).await;
                                    send(console, b"\r\n").await;
                                    return;
                                }
                                page_buf.fill(0xFF);
                                page_pos = 0;
                                page_idx += 1;
                            }
                        }
                        unsafe { LINE_LEN = 0; }
                    }
                }
                0x08 | 0x7F => {
                    if unsafe { LINE_LEN } > 0 {
                        unsafe { LINE_LEN -= 1; }
                        send(console, b"\x08 \x08").await;
                    }
                }
                0x1B => {}
                b if b >= 0x20 && b < 0x7F => {
                    let ll = unsafe { LINE_LEN };
                    if ll < 1023 {
                        unsafe { LINE_BUF[ll] = b; LINE_LEN += 1; }
                        send(console, &[b]).await;
                    }
                }
                _ => {}
            }
        }
    }

    if done {
        // Flush any remaining partial page
        if page_pos > 0 {
            unsafe { crate::fs::PAGE_BUF.copy_from_slice(&page_buf); }
            if let Err(e) = fs.stream_chunk_async(slot, page_idx).await {
                send(console, b"Write error: ").await;
                send(console, e.as_bytes()).await;
                send(console, b"\r\n").await;
                return;
            }
        }

        send(console, b"Finalising...\r\n").await;
        match fs.stream_finish_async(slot, total).await {
            Ok(()) => {
                let mut ns: heapless::String<32> = heapless::String::new();
                let _ = ns.push_str("Saved (");
                crate::mgmt_util::push_num(&mut ns, total);
                let _ = ns.push_str(" bytes)\r\n");
                send(console, ns.as_bytes()).await;
            }
            Err(e) => {
                send(console, b"Error: ").await;
                send(console, e.as_bytes()).await;
                send(console, b"\r\n").await;
            }
        }
    }
}

async fn cmd_del(console: &mut Console, arg: &str) {
    if arg.is_empty() {
        send(console, b"Usage: del <payload.dd>\r\n").await;
        return;
    }
    let fs = crate::fs::FlashFs::new();
    match fs.delete_file_async(arg).await {
        Ok(())  => send(console, b"Deleted\r\n").await,
        Err(e)  => {
            send(console, b"Error: ").await;
            send(console, e.as_bytes()).await;
            send(console, b"\r\n").await;
        }
    }
}

async fn cmd_format(console: &mut Console) {
    send(console, b"Erasing all slots...\r\n").await;
    let fs = crate::fs::FlashFs::new();
    match fs.format_all_async().await {
        Ok(())  => send(console, b"Done\r\n").await,
        Err(e)  => {
            send(console, b"Error: ").await;
            send(console, e.as_bytes()).await;
            send(console, b"\r\n").await;
        }
    }
}

async fn cmd_reboot(console: &mut Console) {
    send(console, b"Rebooting...\r\n").await;
    Timer::after(Duration::from_millis(100)).await;
    cortex_m::peripheral::SCB::sys_reset();
}

async fn cmd_modes(console: &mut Console) {
    send(console, b"Supported ATTACKMODE values:\r\n").await;
    send(console, b"  ATTACKMODE HID              keyboard + mouse\r\n").await;
    send(console, b"  ATTACKMODE HID CDC          keyboard + mouse + serial (default)\r\n").await;

    let sd = crate::usb::msc_sd::sd_available();
    if sd {
        send(console, b"  ATTACKMODE STORAGE          USB mass storage (SD detected)\r\n").await;
        send(console, b"  ATTACKMODE HID STORAGE      keyboard + mouse + storage\r\n").await;
        send(console, b"  ATTACKMODE HID CDC STORAGE  all interfaces\r\n").await;
    } else {
        send(console, b"  ATTACKMODE STORAGE          [NOT AVAILABLE - no SD card]\r\n").await;
        send(console, b"  ATTACKMODE HID STORAGE      [NOT AVAILABLE - no SD card]\r\n").await;
        send(console, b"  ATTACKMODE HID CDC STORAGE  [NOT AVAILABLE - no SD card]\r\n").await;
    }

    // Show current mode
    let mode = crate::attackmode::get();
    let mut mode_str: heapless::String<32> = heapless::String::new();
    if mode.has_hid()      { let _ = mode_str.push_str("HID ");      }
    if mode.has_cdc()      { let _ = mode_str.push_str("CDC ");      }
    if mode.has_storage()  { let _ = mode_str.push_str("STORAGE ");  }
    if mode.has_terminal() { let _ = mode_str.push_str("TERMINAL");  }
    send(console, b"Current mode: ").await;
    send(console, mode_str.as_bytes()).await;
    send(console, b"\r\n").await;

    if let Some(reason) = crate::attackmode::fallback_reason() {
        send(console, b"[!] Fallback reason: ").await;
        send(console, reason.as_bytes()).await;
        send(console, b"\r\n").await;
    }
}

async fn cmd_keys(console: &mut Console) {
    let mut count = 0u8;
    while let Ok(key) = crate::usb::cdc::KEY_CHANNEL.try_receive() {
        send(console, b"  key: ").await;
        send(console, key.as_bytes()).await;
        send(console, b"\r\n").await;
        count += 1;
    }
    if count == 0 {
        send(console, b"No keys buffered (is key_listener.py running on ttyACM1?)\r\n").await;
    }
}

async fn cmd_exfil(console: &mut Console, arg: &str) {
    if arg == "clear" {
        let fs = crate::fs::FlashFs::new();
        match fs.delete_file_async("loot.bin").await {
            Ok(())  => send(console, b"loot.bin deleted\r\n").await,
            Err(_)  => send(console, b"loot.bin not found\r\n").await,
        }
        return;
    }

    let fs = crate::fs::FlashFs::new();
    match fs.read_file_async("loot.bin").await {
        Err(_) => {
            send(console, b"loot.bin empty or not found\r\n").await;
            send(console, b"Exfil requires $_EXFIL_MODE_ENABLED = TRUE in payload\r\n").await;
            send(console, b"and exfil_send.py running on target machine\r\n").await;
        }
        Ok(len) => {
            // Print byte count header
            let mut header: heapless::String<32> = heapless::String::new();
            let _ = header.push_str("loot.bin: ");
            crate::mgmt_util::push_num(&mut header, len);
            let _ = header.push_str(" bytes\r\n");
            send(console, header.as_bytes()).await;

            let data = unsafe { &crate::fs::FLASH_DATA_BUF[..len] };
            let is_text = data.iter().all(|&b| matches!(b, 0x20..=0x7E | b'\n' | b'\r'));

            if is_text {
                // Print as plain text
                for line in core::str::from_utf8(data).unwrap_or("").lines() {
                    send(console, line.as_bytes()).await;
                    send(console, b"\r\n").await;
                }
            } else {
                // Hex dump with address offsets
                for (i, chunk) in data.chunks(16).enumerate() {
                    let mut row: heapless::String<80> = heapless::String::new();
                    crate::mgmt_util::fmt_hex32(&mut row, (i * 16) as u32);
                    let _ = row.push_str(": ");
                    for &b in chunk {
                        crate::mgmt_util::fmt_hex8(&mut row, b);
                        let _ = row.push(' ');
                    }
                    // ASCII column
                    let _ = row.push(' ');
                    for &b in chunk {
                        let _ = row.push(if b >= 0x20 && b < 0x7F { b as char } else { '.' });
                    }
                    let _ = row.push_str("\r\n");
                    send(console, row.as_bytes()).await;
                }
            }
        }
    }
}

async fn cmd_help(console: &mut Console) {
    send(console, b"Commands:\r\n").await;
    send(console, b"  list                   list stored payloads\r\n").await;
    send(console, b"  run <payload.dd>       queue payload to run\r\n").await;
    send(console, b"  get <payload.dd>       show payload contents\r\n").await;
    send(console, b"  put <payload.dd>       upload payload\r\n").await;
    send(console, b"  del <payload.dd>       delete payload\r\n").await;
    send(console, b"  format                 erase all payloads\r\n").await;
    send(console, b"  reboot                 reboot device\r\n").await;
    send(console, b"  modes                  show supported ATTACKMODE values\r\n").await;
    send(console, b"  keys                   show buffered keys from key_listener\r\n").await;
    send(console, b"  exfil                  show loot.bin contents\r\n").await;
    send(console, b"  exfil clear            delete loot.bin\r\n").await;
    send(console, b"  help                   show this help\r\n").await;
    send(console, b"Valid payload names: payload.dd  payload2.dd  payload3.dd\r\n").await;
}

// ---------------------------------------------------------------------------
// Console log drain -- shown before each prompt
// ---------------------------------------------------------------------------

async fn drain_logs(console: &mut Console) {
    if crate::console_log::is_empty() { return; }
    for msg in &crate::console_log::drain() {
        send(console, b"[!] ").await;
        send(console, msg.as_bytes()).await;
        send(console, b"\r\n").await;
    }
}

// ---------------------------------------------------------------------------
// Low-level send helper -- splits into 64-byte USB packets
// ---------------------------------------------------------------------------

async fn send(console: &mut Console, data: &[u8]) {
    for chunk in data.chunks(64) {
        let _ = console.write_packet(chunk).await;
    }
}
