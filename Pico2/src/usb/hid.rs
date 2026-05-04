//! usb/hid.rs — HID keyboard + mouse reports and inner task

use defmt::*;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::hid::{HidReader, HidWriter};
use embassy_time::{Duration, Timer};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
    mutex::Mutex,
};
use heapless::Vec;

// ---------------------------------------------------------------------------
// HID report descriptors
// ---------------------------------------------------------------------------

#[rustfmt::skip]
pub const KEYBOARD_REPORT_DESC: &[u8] = &[
    0x05,0x01, 0x09,0x06, 0xA1,0x01,
    0x05,0x07, 0x19,0xE0, 0x29,0xE7,
    0x15,0x00, 0x25,0x01, 0x75,0x01, 0x95,0x08, 0x81,0x02,
    0x95,0x01, 0x75,0x08, 0x81,0x01,
    0x95,0x05, 0x75,0x01, 0x05,0x08, 0x19,0x01, 0x29,0x05, 0x91,0x02,
    0x95,0x01, 0x75,0x03, 0x91,0x01,
    0x95,0x06, 0x75,0x08, 0x15,0x00, 0x26,0xFF,0x00,
    0x05,0x07, 0x19,0x00, 0x29,0xFF, 0x81,0x00,
    0xC0,
];

#[rustfmt::skip]
pub const MOUSE_REPORT_DESC: &[u8] = &[
    0x05,0x01, 0x09,0x02, 0xA1,0x01,
    0x09,0x01, 0xA1,0x00,
    0x05,0x09, 0x19,0x01, 0x29,0x03,
    0x15,0x00, 0x25,0x01, 0x95,0x03, 0x75,0x01, 0x81,0x02,
    0x95,0x01, 0x75,0x05, 0x81,0x01,
    0x05,0x01, 0x09,0x30, 0x09,0x31,
    0x15,0x81, 0x25,0x7F, 0x75,0x08, 0x95,0x02, 0x81,0x06,
    0x09,0x38, 0x15,0x81, 0x25,0x7F, 0x75,0x08, 0x95,0x01, 0x81,0x06,
    0xC0, 0xC0,
];

// ---------------------------------------------------------------------------
// Report structs
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct KeyboardReport {
    pub modifier:  u8,
    pub reserved:  u8,
    pub keycodes: [u8; 6],
}

impl KeyboardReport {
    pub fn empty() -> Self { Self::default() }

    pub fn press(&mut self, kc: u8) {
        if kc >= 0xE0 {
            self.modifier |= 1 << (kc - 0xE0);
        } else {
            for slot in self.keycodes.iter_mut() {
                if *slot == 0 { *slot = kc; return; }
            }
            warn!("key buffer full, dropping 0x{:02X}", kc);
        }
    }

    pub fn release(&mut self, kc: u8) {
        if kc >= 0xE0 {
            self.modifier &= !(1 << (kc - 0xE0));
        } else {
            for slot in self.keycodes.iter_mut() {
                if *slot == kc { *slot = 0; return; }
            }
        }
    }

    pub fn release_all(&mut self) {
        self.modifier = 0;
        self.reserved = 0;
        self.keycodes = [0u8; 6];
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0] = self.modifier;
        b[1] = self.reserved;
        b[2..8].copy_from_slice(&self.keycodes);
        b
    }
}

#[derive(Clone, Default)]
pub struct MouseReport {
    pub buttons: u8,
    pub x:       i8,
    pub y:       i8,
    pub wheel:   i8,
}

impl MouseReport {
    pub fn empty() -> Self { Self::default() }
    pub fn to_bytes(&self) -> [u8; 4] {
        [self.buttons, self.x as u8, self.y as u8, self.wheel as u8]
    }
}

// ---------------------------------------------------------------------------
// HID command channel
// ---------------------------------------------------------------------------

// Note: no #[derive(defmt::Format)] on HidCommand because heapless::Vec
// does not implement Format in the version pulled in by embassy-usb.
pub enum HidCommand {
    KeyPress(u8),
    KeyRelease(u8),
    KeyReleaseAll,
    KeyCombo(Vec<u8, 8>),
    TypeChar { keycode: u8, shift: bool },
    MouseMove { x: i8, y: i8 },
    MousePress(u8),
    MouseRelease(u8),
    MouseClick(u8),
    MouseWheel(i8),
    ConsumerSend(u16),
}

pub static HID_CHANNEL: Channel<CriticalSectionRawMutex, HidCommand, 4> = Channel::new();

pub async fn send(cmd: HidCommand) {
    HID_CHANNEL.send(cmd).await;
}

// ---------------------------------------------------------------------------
// LED state
// ---------------------------------------------------------------------------

pub static LED_STATE: Mutex<CriticalSectionRawMutex, u8> = Mutex::new(0);

pub async fn get_led_state() -> u8 {
    *LED_STATE.lock().await
}

pub const BTN_LEFT:   u8 = 0x01;
pub const BTN_RIGHT:  u8 = 0x02;
pub const BTN_MIDDLE: u8 = 0x04;

// ---------------------------------------------------------------------------
// HID inner task
// ---------------------------------------------------------------------------

pub async fn hid_task_inner(
    mut kbd_reader:   HidReader<'static, Driver<'static, USB>, 1>,
    mut kbd_writer:   HidWriter<'static, Driver<'static, USB>, 8>,
    mut mouse_writer: HidWriter<'static, Driver<'static, USB>, 4>,
) {
    info!("hid_task_inner started");

    let mut kbd   = KeyboardReport::empty();
    let mut mouse = MouseReport::empty();

    let led_fut = async {
        let mut buf = [0u8; 1];
        loop {
            match kbd_reader.read(&mut buf).await {
                Ok(_) => {
                    let leds = buf[0] & 0x07;
                    *LED_STATE.lock().await = leds;
                }
                Err(_) => { Timer::after(Duration::from_millis(10)).await; }
            }
        }
    };

    let cmd_fut = async {
        loop {
            match HID_CHANNEL.receive().await {
                HidCommand::KeyPress(kc) => {
                    kbd.press(kc);
                    write_kbd(&mut kbd_writer, &kbd).await;
                }
                HidCommand::KeyRelease(kc) => {
                    kbd.release(kc);
                    write_kbd(&mut kbd_writer, &kbd).await;
                }
                HidCommand::KeyReleaseAll => {
                    kbd.release_all();
                    write_kbd(&mut kbd_writer, &kbd).await;
                }
                HidCommand::KeyCombo(keys) => {
                    kbd.release_all();
                    // Press modifiers first
                    for &kc in keys.iter() { if kc >= 0xE0 { kbd.press(kc); } }
                    write_kbd(&mut kbd_writer, &kbd).await;
                    Timer::after(Duration::from_millis(50)).await;
                    // Then press regular keys
                    for &kc in keys.iter() { if kc < 0xE0 { kbd.press(kc); } }
                    write_kbd(&mut kbd_writer, &kbd).await;
                    Timer::after(Duration::from_millis(100)).await;
                    kbd.release_all();
                    write_kbd(&mut kbd_writer, &kbd).await;
                    Timer::after(Duration::from_millis(50)).await;
                }
                HidCommand::TypeChar { keycode, shift } => {
                    if shift { kbd.modifier |= 0x02; }
                    kbd.press(keycode);
                    write_kbd(&mut kbd_writer, &kbd).await;
                    Timer::after(Duration::from_millis(5)).await;
                    kbd.release(keycode);
                    if shift { kbd.modifier &= !0x02; }
                    write_kbd(&mut kbd_writer, &kbd).await;
                }
                HidCommand::MouseMove { x, y } => {
                    mouse.x = x; mouse.y = y;
                    write_mouse(&mut mouse_writer, &mouse).await;
                    mouse.x = 0; mouse.y = 0;
                }
                HidCommand::MousePress(btn) => {
                    mouse.buttons |= btn;
                    write_mouse(&mut mouse_writer, &mouse).await;
                }
                HidCommand::MouseRelease(btn) => {
                    mouse.buttons &= !btn;
                    write_mouse(&mut mouse_writer, &mouse).await;
                }
                HidCommand::MouseClick(btn) => {
                    mouse.buttons |= btn;
                    write_mouse(&mut mouse_writer, &mouse).await;
                    Timer::after(Duration::from_millis(10)).await;
                    mouse.buttons &= !btn;
                    write_mouse(&mut mouse_writer, &mouse).await;
                }
                HidCommand::MouseWheel(w) => {
                    mouse.wheel = w;
                    write_mouse(&mut mouse_writer, &mouse).await;
                    mouse.wheel = 0;
                }
                HidCommand::ConsumerSend(usage) => {
                    let down = {
                        let mut b = [0u8; 8];
                        let le = usage.to_le_bytes();
                        b[0] = le[0]; b[1] = le[1];
                        b
                    };
                    let _ = kbd_writer.write(&down).await;
                    Timer::after(Duration::from_millis(5)).await;
                    let _ = kbd_writer.write(&[0u8; 8]).await;
                }
            }
        }
    };

    embassy_futures::join::join(led_fut, cmd_fut).await;
}

async fn write_kbd(w: &mut HidWriter<'static, Driver<'static, USB>, 8>, r: &KeyboardReport) {
    if let Err(e) = w.write(&r.to_bytes()).await {
        warn!("kbd write: {}", defmt::Debug2Format(&e));
    }
}

async fn write_mouse(w: &mut HidWriter<'static, Driver<'static, USB>, 4>, r: &MouseReport) {
    if let Err(e) = w.write(&r.to_bytes()).await {
        warn!("mouse write: {}", defmt::Debug2Format(&e));
    }
}
