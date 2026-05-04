//! usb/msc_sd.rs — SD card block device backend for MSC

use defmt::*;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_rp::{gpio::Output, spi::Spi, peripherals::SPI0};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{sdcard::DummyCsPin as SdDummyCs, Block, BlockDevice, BlockIdx, SdCard};
use static_cell::StaticCell;

pub type SpiDev = ExclusiveDevice<Spi<'static, SPI0, embassy_rp::spi::Blocking>, Output<'static>, Delay>;
type Card = SdCard<SpiDev, SdDummyCs, Delay>;

static SD_AVAILABLE:   AtomicBool = AtomicBool::new(false);
static SD_BLOCK_COUNT: AtomicU32  = AtomicU32::new(0);
static SD_CARD: StaticCell<Mutex<CriticalSectionRawMutex, Card>> = StaticCell::new();
static mut SD_CARD_REF: Option<&'static Mutex<CriticalSectionRawMutex, Card>> = None;

/// Initialize with SPI device. Returns block count (0 if no card).
pub fn init(spi: SpiDev) -> u32 {
    let card = SdCard::new(spi, SdDummyCs, Delay);
    let block_count = match card.num_bytes() {
        Ok(n) => {
            let blocks = (n / 512) as u32;
            info!("[msc_sd] SD found: {} blocks ({} MB)", blocks, blocks / 2048);
            blocks
        }
        Err(e) => {
            warn!("[msc_sd] SD probe failed: {:?}", defmt::Debug2Format(&e));
            0
        }
    };
    let mutex = SD_CARD.init(Mutex::new(card));
    unsafe { SD_CARD_REF = Some(mutex); }
    if block_count > 0 {
        SD_BLOCK_COUNT.store(block_count, Ordering::Relaxed);
        SD_AVAILABLE.store(true, Ordering::Relaxed);
    }
    block_count
}

pub fn sd_available()   -> bool { SD_AVAILABLE.load(Ordering::Relaxed) }
pub fn sd_block_count() -> u32  { SD_BLOCK_COUNT.load(Ordering::Relaxed) }

pub fn read_block(lba: u32, buf: &mut [u8; 512]) -> bool {
    let Some(card_mutex) = (unsafe { SD_CARD_REF }) else { buf.fill(0); return false; };
    let Ok(card) = card_mutex.try_lock() else {
        buf.fill(0); return true;
    };
    let mut block = [Block::new()];
    match card.read(&mut block, BlockIdx(lba), "msc") {
        Ok(()) => { buf.copy_from_slice(&block[0].contents); true }
        Err(e) => { warn!("[msc_sd] read {} err: {:?}", lba, defmt::Debug2Format(&e)); false }
    }
}

pub fn write_block(lba: u32, buf: &[u8; 512]) -> bool {
    let Some(card_mutex) = (unsafe { SD_CARD_REF }) else { return false; };
    let Ok(card) = card_mutex.try_lock() else { return false; };
    let mut block = [Block::new()];
    block[0].contents.copy_from_slice(buf);
    match card.write(&block, BlockIdx(lba)) {
        Ok(()) => true,
        Err(e) => { warn!("[msc_sd] write {} err: {:?}", lba, defmt::Debug2Format(&e)); false }
    }
}
