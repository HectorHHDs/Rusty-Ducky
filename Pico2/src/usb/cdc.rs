//! usb/cdc.rs — CDC ACM serial (Step 2)
//!
//! Mirrors usb_cdc.enable(console=True, data=True) from boot.py.
//! Provides the WAIT_FOR_KEY receive loop (mirrors waitForKey() in duckyinpython.py).
//!
//! key_listener.py protocol:
//!   Host sends newline-terminated key strings over the CDC data port.
//!   e.g. "a\n", "SHIFT+I\n", "CTRL+C\n", "ENTER\n"
//!   Pico reads, strips \n, pushes to KEY_CHANNEL for executor to receive.

use defmt::*;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
};
use heapless::String;

// ---------------------------------------------------------------------------
// Key string channel (WAIT_FOR_KEY)
// ---------------------------------------------------------------------------
// key_listener.py sends newline-terminated key strings.
// We buffer up to 8 of them so a fast typist doesn't lose keypresses.

pub type KeyString = String<32>;
pub static KEY_CHANNEL: Channel<CriticalSectionRawMutex, KeyString, 8> = Channel::new();

// ---------------------------------------------------------------------------
// CDC inner task (runs as a future inside usb_task)
// ---------------------------------------------------------------------------

pub async fn cdc_task_inner(
    mut class: CdcAcmClass<'static, Driver<'static, USB>>,
) {
    info!("cdc_task_inner started");

    let mut buf  = [0u8; 64];
    let mut line: KeyString = String::new();

    loop {
        // Wait for key_listener to connect on the data port
        class.wait_connection().await;
        info!("[CDC] key_listener connected");

        loop {
            match class.read_packet(&mut buf).await {
                Ok(n) => {
                    for &byte in &buf[..n] {
                        match byte {
                            b'\n' => {
                                // Complete key string
                                if !line.is_empty() {
                                    info!("[WAIT_FOR_KEY] received: {}", line.as_str());
                                    // Non-blocking send; if channel full, discard oldest
                                    if KEY_CHANNEL.try_send(line.clone()).is_err() {
                                        // Channel full — drain one and retry
                                        let _ = KEY_CHANNEL.try_receive();
                                        let _ = KEY_CHANNEL.try_send(line.clone());
                                    }
                                    line.clear();
                                }
                            }
                            b'\r' => { /* ignore carriage return */ }
                            c => {
                                // Append (silently truncate if line is too long)
                                let _ = line.push(c as char);
                            }
                        }
                    }
                }
                Err(_) => {
                    info!("[CDC] key_listener disconnected");
                    line.clear();
                    break;
                }
            }
        }
    }
}
