//! mgmt.rs — Serial management console

use defmt::*;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_time::{Duration, Timer};

use crate::hardware::SCRIPT_SIGNAL;

async fn drain_logs(console: &mut embassy_usb::class::cdc_acm::CdcAcmClass<'static, embassy_rp::usb::Driver<'static, embassy_rp::peripherals::USB>>) {
    if crate::console_log::is_empty() { return; }
    let msgs = crate::console_log::drain();
    for msg in &msgs {
        send(console, b"[!] ").await;
        send(console, msg.as_bytes()).await;
        send(console, b"\r\n").await;
    }
}

pub async fn mgmt_task_inner(
    mut console: CdcAcmClass<'static, Driver<'static, USB>>,
) {
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
                    0x08 | 0x7F => {
                        if len > 0 { len -= 1; send(&mut console, b"\x08 \x08").await; }
                    }
                    0x1B => { /* ignore escape */ }
                    b if b >= 0x20 && b < 0x7F => {
                        if len < 63 { line[len] = b; len += 1; send(&mut console, &[b]).await; }
                    }
                    _ => {}
                }
            }
        }
        info!("[mgmt] disconnected");
    }
}

async fn handle_cmd(console: &mut CdcAcmClass<'static, Driver<'static, USB>>, line: &str) {
    let line = line.trim();
    if line.is_empty() { return; }

    let (cmd, arg) = match line.find(' ') {
        Some(i) => (&line[..i], line[i+1..].trim()),
        None    => (line, ""),
    };

    let mut lc = [0u8; 8];
    let cb = cmd.as_bytes();
    for i in 0..cb.len().min(8) { lc[i] = cb[i].to_ascii_lowercase(); }
    let cmd_lc = core::str::from_utf8(&lc[..cb.len().min(8)]).unwrap_or("");

    match cmd_lc {
        "modes" => {
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
            let mode = crate::attackmode::get();
            // Build mode string from flags
            let mut mode_parts: heapless::String<32> = heapless::String::new();
            if mode.has_hid()      { let _ = mode_parts.push_str("HID "); }
            if mode.has_cdc()      { let _ = mode_parts.push_str("CDC "); }
            if mode.has_storage()  { let _ = mode_parts.push_str("STORAGE "); }
            if mode.has_terminal() { let _ = mode_parts.push_str("TERMINAL"); }
            let mode_str = mode_parts.as_bytes();
            send(console, b"Current mode: ").await;
            send(console, mode_str).await;
            send(console, b"\r\n").await;
            if let Some(reason) = crate::attackmode::fallback_reason() {
                send(console, b"[!] Fallback reason: ").await;
                send(console, reason.as_bytes()).await;
                send(console, b"\r\n").await;
            }
        }
        "help" => {
            send(console, b"Commands:\r\n").await;
            send(console, b"  list                   list stored payloads\r\n").await;
            send(console, b"  run <payload.dd>       queue payload to run\r\n").await;
            send(console, b"  get <payload.dd>       show payload contents\r\n").await;
            send(console, b"  put <payload.dd>       upload payload\r\n").await;
            send(console, b"  del <payload.dd>       delete payload\r\n").await;
            send(console, b"  format                 erase all payloads\r\n").await;
            send(console, b"  reboot                 reboot device\r\n").await;
            send(console, b"  modes                  show supported ATTACKMODE values\r\n").await;
            send(console, b"Valid names: payload.dd payload2.dd payload3.dd payload4.dd\r\n").await;
        }

        "list" => {
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

        "run" => {
            // Single flash read for both ATTACKMODE and syntax checks
            if !arg.is_empty() {
                let fs = crate::fs::FlashFs::new();
                // Copy payload to a local stack buffer for analysis
                static mut CHECK_BUF: [u8; 1024] = [0u8; 1024];
                static mut CHECK_LEN: usize = 0;
                if let Ok(len) = fs.read_file_async(arg).await {
                    let n = len.min(1024);
                    unsafe {
                        CHECK_BUF[..n].copy_from_slice(&crate::fs::FLASH_DATA_BUF[..n]);
                        CHECK_LEN = n;
                    }
                    if let Ok(text) = core::str::from_utf8(unsafe { &CHECK_BUF[..unsafe { CHECK_LEN }] }) {
                        let sd = crate::usb::msc_sd::sd_available();
                        let requested = crate::attackmode::parse_requested(text);
                        if requested.has_storage() && !sd {
                            send(console, b"[!] ATTACKMODE STORAGE not available - no SD card\r\n").await;
                            send(console, b"[!] Payload will run in current HID CDC mode\r\n").await;
                        } else {
                            let active = crate::attackmode::get();
                            // Only reboot if STORAGE mode changes — that requires new USB descriptors
                            let storage_changed = requested.has_storage() != active.has_storage();
                            if storage_changed {
                                send(console, b"[!] ATTACKMODE STORAGE change - rebooting to apply...\r\n").await;
                                embassy_time::Timer::after(embassy_time::Duration::from_millis(300)).await;
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
            }
            let name: &'static str = match arg {
                "payload2.dd" => "payload2.dd",
                "payload3.dd" => "payload3.dd",
                "payload4.dd" => "payload4.dd",
                "payload.dd"  => "payload.dd",
                _ => {
                    send(console, b"Usage: run <payload.dd|payload2.dd|payload3.dd|payload4.dd>\r\n").await;
                    return;
                }
            };
            match SCRIPT_SIGNAL.try_send(name) {
                Ok(())  => send(console, b"Queued\r\n").await,
                Err(_)  => send(console, b"Queue full, try again\r\n").await,
            }
        }

        "get" => {
            if arg.is_empty() {
                send(console, b"Usage: get <payload.dd>\r\n").await;
            } else {
                let fs = crate::fs::FlashFs::new();
                match fs.read_file_async(arg).await {
                    Err(_) => send(console, b"Not found\r\n").await,
                    Ok(len) => {
                        let data = unsafe { &crate::fs::FLASH_DATA_BUF[..len] };
                        let text = core::str::from_utf8(data).unwrap_or("(binary)");
                        for line in text.lines() {
                            send(console, line.as_bytes()).await;
                            send(console, b"\r\n").await;
                        }
                    }
                }
            }
        }

        "put" => {
            let name: &'static str = match arg {
                "payload2.dd" => "payload2.dd",
                "payload3.dd" => "payload3.dd",
                "payload4.dd" => "payload4.dd",
                "payload.dd"  => "payload.dd",
                _ => {
                    send(console, b"Usage: put <payload.dd|payload2.dd|payload3.dd|payload4.dd>\r\n").await;
                    return;
                }
            };
            send(console, b"Send content, type END on its own line to finish:\r\n").await;

            let mut content     = [0u8; 4096];
            let mut content_len = 0usize;
            let mut rx2         = [0u8; 64];
            let mut line2       = [0u8; 64];
            let mut len2        = 0usize;

            'upload: loop {
                let n2 = match console.read_packet(&mut rx2).await {
                    Ok(n) => n, Err(_) => break 'upload,
                };
                for i in 0..n2 {
                    match rx2[i] {
                        b'\r' | b'\n' => {
                            send(console, b"\r\n").await;
                            if len2 > 0 {
                                if let Ok(s) = core::str::from_utf8(&line2[..len2]) {
                                    if s.trim().eq_ignore_ascii_case("end") { break 'upload; }
                                    let bytes = s.as_bytes();
                                    let space = content.len() - content_len;
                                    let take  = bytes.len().min(space);
                                    content[content_len..content_len+take].copy_from_slice(&bytes[..take]);
                                    content_len += take;
                                    if content_len < content.len() {
                                        content[content_len] = b'\n';
                                        content_len += 1;
                                    }
                                }
                                len2 = 0;
                            }
                        }
                        0x08 | 0x7F => {
                            if len2 > 0 { len2 -= 1; send(console, b"\x08 \x08").await; }
                        }
                        0x1B => {}
                        b if b >= 0x20 && b < 0x7F => {
                            if len2 < 63 { line2[len2] = b; len2 += 1; send(console, &[b]).await; }
                        }
                        _ => {}
                    }
                }
            }

            send(console, b"Writing to flash...\r\n").await;
            let fs = crate::fs::FlashFs::new();
            match fs.write_file_async(name, &content[..content_len]).await {
                Ok(())  => send(console, b"Saved\r\n").await,
                Err(e)  => { send(console, b"Error: ").await; send(console, e.as_bytes()).await; send(console, b"\r\n").await; }
            }
        }

        "del" => {
            if arg.is_empty() {
                send(console, b"Usage: del <payload.dd>\r\n").await;
            } else {
                let fs = crate::fs::FlashFs::new();
                match fs.delete_file_async(arg).await {
                    Ok(())  => send(console, b"Deleted\r\n").await,
                    Err(e)  => { send(console, b"Error: ").await; send(console, e.as_bytes()).await; send(console, b"\r\n").await; }
                }
            }
        }

        "format" => {
            send(console, b"Erasing all slots...\r\n").await;
            let fs = crate::fs::FlashFs::new();
            match fs.format_all_async().await {
                Ok(())  => send(console, b"Done\r\n").await,
                Err(e)  => { send(console, b"Error: ").await; send(console, e.as_bytes()).await; send(console, b"\r\n").await; }
            }
        }

        "reboot" => {
            send(console, b"Rebooting...\r\n").await;
            Timer::after(Duration::from_millis(100)).await;
            cortex_m::peripheral::SCB::sys_reset();
        }

        _ => send(console, b"Unknown command (type help)\r\n").await,
    }
}

async fn send(console: &mut CdcAcmClass<'static, Driver<'static, USB>>, data: &[u8]) {
    for chunk in data.chunks(64) { let _ = console.write_packet(chunk).await; }
}
