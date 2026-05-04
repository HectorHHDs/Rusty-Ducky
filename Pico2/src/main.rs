#![no_std]
#![no_main]

use embassy_rp::block::ImageDef;
#[link_section = ".start_block"]
#[used]
static IMAGE_DEF: ImageDef = ImageDef::secure_exe();
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts,
    gpio::{Input, Level, Output, Pull},
    spi::{Config as SpiConfig, Spi},
    peripherals::USB,
    usb::{Driver, InterruptHandler},
};
use embassy_time::{Duration, Timer, Delay};
use embedded_hal_bus::spi::ExclusiveDevice;
use embassy_usb::{Builder, Config, UsbDevice};
use embassy_usb::class::hid::{
    HidReaderWriter, HidWriter, State as HidState, HidSubclass, HidBootProtocol,
};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcState};
use static_cell::StaticCell;

mod attackmode;
mod console_log;
mod detection;
mod hardware;
mod fs;
mod usb;
mod ducky;
mod layout;
mod mgmt;
mod exfil;

use usb::hid::{KEYBOARD_REPORT_DESC, MOUSE_REPORT_DESC};
use hardware::SCRIPT_SIGNAL;
use attackmode::AttackMode;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    DMA_IRQ_0   => embassy_rp::dma::InterruptHandler<embassy_rp::peripherals::DMA_CH0>;
});

type MyDriver = Driver<'static, USB>;

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

#[embassy_executor::task]
async fn usb_runner(mut usb: UsbDevice<'static, MyDriver>) -> ! {
    usb.run().await
}

#[embassy_executor::task]
async fn hid_runner(
    reader: embassy_usb::class::hid::HidReader<'static, MyDriver, 1>,
    writer: embassy_usb::class::hid::HidWriter<'static, MyDriver, 8>,
    mouse:  embassy_usb::class::hid::HidWriter<'static, MyDriver, 4>,
) {
    usb::hid::hid_task_inner(reader, writer, mouse).await;
}

#[embassy_executor::task]
async fn cdc_data_runner(class: CdcAcmClass<'static, MyDriver>) {
    usb::cdc::cdc_task_inner(class).await;
}

#[embassy_executor::task]
async fn mgmt_runner(class: CdcAcmClass<'static, MyDriver>) {
    mgmt::mgmt_task_inner(class).await;
}

#[embassy_executor::task]
async fn msc_runner(
    ep_out: <MyDriver as embassy_usb::driver::Driver<'static>>::EndpointOut,
    ep_in:  <MyDriver as embassy_usb::driver::Driver<'static>>::EndpointIn,
) {
    usb::msc::msc_task_inner::<MyDriver>(ep_out, ep_in, usb::msc_sd::sd_block_count()).await;
}

#[embassy_executor::task]
async fn payload_task(boot_payload: &'static str) {
    Timer::after(Duration::from_millis(3000)).await;
    ducky::run_payload(boot_payload).await;
    loop {
        let p = SCRIPT_SIGNAL.receive().await;
        ducky::run_payload(p).await;
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    info!("ducky-rs booting");

    spawner.spawn(hardware::led_task(Output::new(p.PIN_25, Level::Low)).expect("led"));

    // Spawn flash task first — needed to read payload for ATTACKMODE
    {
        use embassy_rp::flash::{Async, Flash};
        let flash = Flash::<_, Async, {2 * 1024 * 1024}>::new(p.FLASH, p.DMA_CH0, Irqs);
        spawner.spawn(fs::flash_task(flash).expect("flash"));
    }
    ducky::init_flash_fs();

    // SD probe — needed for STORAGE mode capability check
    // SD probe — msc_sd::init takes ownership and does real block R/W
    let features = {
        let spi1 = Spi::new_blocking(p.SPI0, p.PIN_18, p.PIN_19, p.PIN_16, SpiConfig::default());
        let cs1  = Output::new(p.PIN_17, Level::High);
        match ExclusiveDevice::new(spi1, cs1, Delay) {
            Ok(dev) => {
                // msc_sd::init probes and keeps the SPI device for block R/W
                let block_count = usb::msc_sd::init(dev);
                if block_count > 0 {
                    detection::Features {
                        sd_available: true, sd_block_count: block_count,
                        usb_msc: true, usb_hid: true, usb_cdc: true,
                    }
                } else {
                    detection::Features::none()
                }
            }
            Err(_) => detection::Features::none(),
        }
    };

    // Boot mode + payload selector pins
    let prog_mode = Input::new(p.PIN_0, Pull::Up).is_low();
    let p1 = Input::new(p.PIN_4,  Pull::Up);
    let p2 = Input::new(p.PIN_5,  Pull::Up);
    let p3 = Input::new(p.PIN_10, Pull::Up);
    let p4 = Input::new(p.PIN_11, Pull::Up);
    let boot_payload: &'static str = match (p1.is_low(), p2.is_low(), p3.is_low(), p4.is_low()) {
        (true,_,_,_) => "payload.dd",
        (_,true,_,_) => "payload2.dd",
        (_,_,true,_) => "payload3.dd",
        (_,_,_,true) => "payload4.dd",
        _            => "payload.dd",
    };
    spawner.spawn(hardware::button_task(
        Input::new(p.PIN_22, Pull::Up), p1, p2, p3, p4,
    ).expect("btn"));

    // Read boot payload and parse ATTACKMODE before USB starts
    // We need to wait a moment for flash_task to be ready
    Timer::after(Duration::from_millis(50)).await;
    let mode = {
        let fs = fs::FlashFs::new();
        match fs.read_file_async(boot_payload).await {
            Ok(len) => {
                info!("[main] read {} bytes from {}", len, boot_payload);
                let data = unsafe { &fs::FLASH_DATA_BUF[..len] };
                if let Ok(text) = core::str::from_utf8(data) {
                    // Store requested mode separately to detect fallback
                    let parsed = attackmode::parse_from_payload(text, features.sd_available);
                    info!("[main] parsed attack mode: {:?}", parsed);
                    parsed
                } else {
                    warn!("[main] payload not valid UTF-8");
                    AttackMode::DEFAULT
                }
            }
            Err(_) => {
                // No payload on flash yet — normal on first boot
                AttackMode::DEFAULT
            }
        }
    };
    // If prog_mode pin is active, force HID CDC (safe mode for management)
    // prog_mode forces terminal on regardless of payload
    let mode = if prog_mode {
        info!("[main] prog_mode active — forcing TERMINAL on");
        crate::console_log::push("prog_mode active — terminal forced on, payload will not run");
        mode.with_terminal()
    } else {
        mode
    };
    attackmode::set(mode);
    info!("Attack mode: {:?}", mode);

    // Build USB device based on attack mode
    let driver = Driver::new(p.USB, Irqs);
    let mut config           = Config::new(0x239A, 0x80F4);
    config.manufacturer      = Some("Raspberry Pi");
    config.product           = Some("Pico Ducky");
    config.serial_number     = Some("DUCKY01");
    config.max_power         = 100;
    config.max_packet_size_0 = 64;
    config.device_class      = 0xEF;
    config.device_sub_class  = 0x02;
    config.device_protocol   = 0x01;

    let mut builder = {
        static CONFIG_DESC: StaticCell<[u8; 512]> = StaticCell::new();
        static BOS_DESC:    StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]>  = StaticCell::new();
        Builder::new(driver, config,
            CONFIG_DESC.init([0; 512]),
            BOS_DESC.init([0; 256]),
            &mut [],
            CONTROL_BUF.init([0; 64]),
        )
    };

    // CDC management console — only if TERMINAL mode
    let cdc_mgmt = if mode.has_terminal() {
        static S: StaticCell<CdcState> = StaticCell::new();
        Some(CdcAcmClass::new(&mut builder, S.init(CdcState::new()), 64))
    } else { None };

    // CDC data port (key_listener) — only if HID+CDC mode
    let cdc_data = if mode.has_cdc() {
        static S: StaticCell<CdcState> = StaticCell::new();
        Some(CdcAcmClass::new(&mut builder, S.init(CdcState::new()), 64))
    } else { None };

    // HID keyboard + mouse
    let (kbd, mouse) = if mode.has_hid() {
        let kbd = {
            static S: StaticCell<HidState> = StaticCell::new();
            HidReaderWriter::<_, 1, 8>::new(&mut builder, S.init(HidState::new()),
                embassy_usb::class::hid::Config {
                    report_descriptor: KEYBOARD_REPORT_DESC, request_handler: None,
                    poll_ms: 1, max_packet_size: 8,
                    hid_subclass: HidSubclass::Boot,
                    hid_boot_protocol: HidBootProtocol::Keyboard,
                })
        };
        let mouse = {
            static S: StaticCell<HidState> = StaticCell::new();
            HidWriter::<_, 4>::new(&mut builder, S.init(HidState::new()),
                embassy_usb::class::hid::Config {
                    report_descriptor: MOUSE_REPORT_DESC, request_handler: None,
                    poll_ms: 1, max_packet_size: 4,
                    hid_subclass: HidSubclass::No,
                    hid_boot_protocol: HidBootProtocol::Mouse,
                })
        };
        (Some(kbd), Some(mouse))
    } else { (None, None) };

    // MSC endpoints — only if STORAGE mode
    let msc_eps = if mode.has_storage() {
        Some(usb::msc::build_msc(&mut builder))
    } else { None };

    let usb = builder.build();
    spawner.spawn(usb_runner(usb).expect("usb"));

    // Spawn class runners based on mode
    if let (Some(k), Some(m)) = (kbd, mouse) {
        let (kr, kw) = k.split();
        spawner.spawn(hid_runner(kr, kw, m).expect("hid"));
    }
    if let Some(cdc) = cdc_data {
        spawner.spawn(cdc_data_runner(cdc).expect("cdc_data"));
    }
    if let Some(mgmt) = cdc_mgmt {
        spawner.spawn(mgmt_runner(mgmt).expect("mgmt"));
    }

    if let Some(eps) = msc_eps {
        spawner.spawn(msc_runner(eps.ep_out, eps.ep_in).expect("msc"));
    }

    spawner.spawn(exfil::exfil_task().expect("exfil"));

    if !prog_mode {
        spawner.spawn(payload_task(boot_payload).expect("payload"));
    }

    info!("all tasks spawned (mode={:?})", mode);
    loop { Timer::after(Duration::from_secs(60)).await; }
}
