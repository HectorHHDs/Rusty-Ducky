//! detection.rs — SD card probe + Features flags

use defmt::*;
use embedded_sdmmc::sdcard::DummyCsPin as SdDummyCs;
use embedded_sdmmc::{SdCard, Block, BlockDevice};
use embedded_hal::spi::SpiDevice;
use embassy_time::Delay;

#[derive(Clone, Copy, defmt::Format)]
pub struct Features {
    pub sd_available:   bool,
    pub sd_block_count: u32,
    pub usb_msc:        bool,
    pub usb_hid:        bool,
    pub usb_cdc:        bool,
}

impl Features {
    pub const fn none() -> Self {
        Self { sd_available: false, sd_block_count: 0,
               usb_msc: false, usb_hid: true, usb_cdc: true }
    }
}

/// Quick probe — just checks if SD is present and gets block count.
/// Does NOT hold onto the SPI device.
pub fn probe_sd<SPI: SpiDevice>(spi: SPI) -> Features {
    info!("[detection] probing SD...");
    let card = SdCard::new(spi, SdDummyCs, Delay);
    match card.num_bytes() {
        Ok(n) => {
            let blocks = (n / 512) as u32;
            info!("[detection] SD found: {} MB ({} blocks)", n / 1_000_000, blocks);
            Features { sd_available: true, sd_block_count: blocks,
                       usb_msc: true, usb_hid: true, usb_cdc: true }
        }
        Err(e) => {
            warn!("[detection] SD not found: {:?}", defmt::Debug2Format(&e));
            Features::none()
        }
    }
}
