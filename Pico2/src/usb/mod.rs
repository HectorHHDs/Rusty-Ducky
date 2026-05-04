//! usb/mod.rs — USB device task

pub mod hid;
pub mod cdc;
pub mod msc;
pub mod msc_sd;

use defmt::*;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::{Builder, Config};
use embassy_usb::class::hid::{HidReaderWriter, HidWriter, State as HidState, HidSubclass, HidBootProtocol};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcState};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use static_cell::StaticCell;

use crate::detection::Features;
use hid::{KEYBOARD_REPORT_DESC, MOUSE_REPORT_DESC};

static USB_READY: Signal<CriticalSectionRawMutex, bool> = Signal::new();

pub async fn wait_ready() {
    USB_READY.wait().await;
}

static CONFIG_DESC:    StaticCell<[u8; 256]>          = StaticCell::new();
static BOS_DESC:       StaticCell<[u8; 256]>          = StaticCell::new();
static MSOS_DESC:      StaticCell<[u8; 256]>          = StaticCell::new();
static CONTROL_BUF:    StaticCell<[u8; 64]>           = StaticCell::new();
static KBD_STATE:      StaticCell<HidState<'static>>  = StaticCell::new();
static MOUSE_STATE:    StaticCell<HidState<'static>>  = StaticCell::new();
static CDC_MGMT_STATE: StaticCell<CdcState<'static>>  = StaticCell::new();
static CDC_DATA_STATE: StaticCell<CdcState<'static>>  = StaticCell::new();

pub struct UsbState;
impl UsbState { pub fn new() -> Self { Self } }

#[embassy_executor::task]
pub async fn usb_task(
    driver:     Driver<'static, USB>,
    _state:     &'static mut UsbState,
    features:   Features,
    no_storage: bool,
) {
    info!("usb_task: started");

    let mut config           = Config::new(0x239A, 0x80F4);
    config.manufacturer      = Some("Raspberry Pi");
    config.product           = Some("Pico Ducky");
    config.serial_number     = Some("DUCKY01");
    config.max_power         = 100;
    config.max_packet_size_0 = 64;
    config.device_class      = 0xEF;
    config.device_sub_class  = 0x02;
    config.device_protocol   = 0x01;

    let config_desc    = CONFIG_DESC.init([0u8; 256]);
    let bos_desc       = BOS_DESC.init([0u8; 256]);
    let msos_desc      = MSOS_DESC.init([0u8; 256]);
    let control_buf    = CONTROL_BUF.init([0u8; 64]);
    let kbd_state      = KBD_STATE.init(HidState::new());
    let mouse_state    = MOUSE_STATE.init(HidState::new());
    let cdc_mgmt_state = CDC_MGMT_STATE.init(CdcState::new());
    let cdc_data_state = CDC_DATA_STATE.init(CdcState::new());

    let mut builder = Builder::new(
        driver, config,
        config_desc, bos_desc, msos_desc, control_buf,
    );

    let cdc_mgmt = CdcAcmClass::new(&mut builder, cdc_mgmt_state, 64);
    let cdc_data = CdcAcmClass::new(&mut builder, cdc_data_state, 64);

    let kbd = HidReaderWriter::<_, 1, 8>::new(&mut builder, kbd_state,
        embassy_usb::class::hid::Config {
            report_descriptor: KEYBOARD_REPORT_DESC,
            request_handler: None, poll_ms: 1, max_packet_size: 8,
            hid_subclass: HidSubclass::Boot,
            hid_boot_protocol: HidBootProtocol::Keyboard,
        });

    let mouse = HidWriter::<_, 4>::new(&mut builder, mouse_state,
        embassy_usb::class::hid::Config {
            report_descriptor: MOUSE_REPORT_DESC,
            request_handler: None, poll_ms: 1, max_packet_size: 4,
            hid_subclass: HidSubclass::No,
            hid_boot_protocol: HidBootProtocol::Mouse,
        });

    let mut usb = builder.build();
    info!("USB built");
    USB_READY.signal(true);

    let (kbd_reader, kbd_writer) = kbd.split();

    // Run all futures on this task's stack but yield frequently
    // to prevent starving other tasks
    embassy_futures::join::join4(
        usb.run(),
        hid::hid_task_inner(kbd_reader, kbd_writer, mouse),
        cdc::cdc_task_inner(cdc_data),
        crate::mgmt::mgmt_task_inner(cdc_mgmt),
    ).await;
}
