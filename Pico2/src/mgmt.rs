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
        "exfil" => {
            if arg == "clear" {
                let fs = crate::fs::FlashFs::new();
                match fs.delete_file_async("loot.bin").await {
                    Ok(())  => send(console, b"loot.bin deleted\r\n").await,
                    Err(_)  => send(console, b"loot.bin not found\r\n").await,
                }
            } else {
                let fs = crate::fs::FlashFs::new();
                match fs.read_file_async("loot.bin").await {
                    Err(_) => {
                        send(console, b"loot.bin empty or not found\r\n").await;
                        send(console, b"Exfil requires $_EXFIL_MODE_ENABLED = TRUE in payload\r\n").await;
                        send(console, b"and exfil_send.py running on target machine\r\n").await;
                    }
                    Ok(len) => {
                        let data = unsafe { &crate::fs::FLASH_DATA_BUF[..len] };
                        // Show as text if printable, otherwise hex
                        let is_text = data.iter().all(|&b| b >= 0x20 && b <= 0x7E || b == b'\n' || b == b'\r');
                        let mut size_str: heapless::String<32> = heapless::String::new();
                        let _ = size_str.push_str("loot.bin: ");
                        crate::mgmt_util::push_num(&mut size_str, len);
                        let _ = size_str.push_str(" bytes\r\n");
                        send(console, size_str.as_bytes()).await;
                        if is_text {
                            for line in core::str::from_utf8(data).unwrap_or("").lines() {
                                send(console, line.as_bytes()).await;
                                send(console, b"\r\n").await;
                            }
                        } else {
                            // Hex dump
                            for (i, chunk) in data.chunks(16).enumerate() {
                                let mut line: heapless::String<80> = heapless::String::new();
                                crate::mgmt_util::fmt_hex32(&mut line, (i * 16) as u32);
                                let _ = line.push_str(": ");
                                for &b in chunk {
                                    crate::mgmt_util::fmt_hex8(&mut line, b);
                                    let _ = line.push(' ');
                                }
                                let _ = line.push_str("\r\n");
                                send(console, line.as_bytes()).await;
                            }
                        }
                    }
                }
            }
        }
        "keys" => {
            // Drain and show all buffered keys from key_listener.py
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
            send(console, b"  keys                   show buffered keys from key_listener\r\n").await;
            send(console, b"  exfil                  show loot.bin contents and byte count\r\n").await;
            send(console, b"  exfil clear            delete loot.bin\r\n").await;
            send(console, b"Valid names: payload.dd payload2.dd payload3.dd\r\n").await;
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
                "payload.dd"  => "payload.dd",
                _ => {
                    send(console, b"Usage: put <payload.dd|payload2.dd|payload3.dd|payload4.dd>\r\n").await;
                    return;
                }
            };

            // Erase slot immediately
            send(console, b"Erasing slot...\r\n").await;
            let fs = crate::fs::FlashFs::new();
            let slot = match fs.stream_begin_async(name).await {
                Ok(s)  => s,
                Err(e) => { send(console, b"Error: ").await; send(console, e.as_bytes()).await; send(console, b"\r\n").await; return; }
            };
            send(console, b"Send content, type END on its own line to finish:\r\n").await;

            // Stream line by line into flash pages
            // PAGE accumulates bytes; when full (256 bytes) it flushes to flash
            static mut LINE_BUF: [u8; 1024] = [0u8; 1024];
            static mut LINE_LEN: usize      = 0;
            unsafe { LINE_LEN = 0; }

            let mut page_buf  = [0xFFu8; 256];
            let mut page_pos  = 0usize;   // position within current page
            let mut page_idx  = 0usize;   // which page we're on
            let mut total     = 0usize;   // total bytes written
            let mut rx2       = [0u8; 64];
            let mut done      = false;

            'upload: loop {
                let n2 = match console.read_packet(&mut rx2).await {
                    Ok(n) => n, Err(_) => break 'upload,
                };

                for i in 0..n2 {
                    match rx2[i] {
                        b'\r' | b'\n' => {
                            send(console, b"\r\n").await;
                            let ll = unsafe { LINE_LEN };
                            if ll > 0 {
                                let line = unsafe { core::str::from_utf8(&LINE_BUF[..ll]).unwrap_or("") };
                                if line.trim().eq_ignore_ascii_case("end") {
                                    done = true;
                                    break 'upload;
                                }
                                // Append line + newline to page buffer
                                let bytes = unsafe { &LINE_BUF[..ll] };
                                for &b in bytes.iter().chain(b"\n".iter()) {
                                    page_buf[page_pos] = b;
                                    page_pos += 1;
                                    total += 1;
                                    // Flush page when full
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
                // Flush remaining partial page
                if page_pos > 0 {
                    unsafe { crate::fs::PAGE_BUF.copy_from_slice(&page_buf); }
                    if let Err(e) = fs.stream_chunk_async(slot, page_idx).await {
                        send(console, b"Write error: ").await;
                        send(console, e.as_bytes()).await;
                        send(console, b"\r\n").await;
                        return;
                    }
                }
                // Write header with total length
                send(console, b"Finalising...\r\n").await;
                match fs.stream_finish_async(slot, total).await {
                    Ok(())  => {
                        send(console, b"Saved (").await;
                        let mut ns: heapless::String<32> = heapless::String::new();
                        crate::mgmt_util::push_num(&mut ns, total);
                        send(console, ns.as_bytes()).await;
                        send(console, b" bytes)\r\n").await;
                    }
                    Err(e) => { send(console, b"Error: ").await; send(console, e.as_bytes()).await; send(console, b"\r\n").await; }
                }
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
