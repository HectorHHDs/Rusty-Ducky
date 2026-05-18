//! usb/msc_sd.rs — SD card block device backend
//!
//! Dual-partition scheme (only active when SD card detected):
//!   Partition 1 (visible, 90%): FAT32, auto-formatted on first use
//!   Partition 2 (hidden,  10%): FAT32, only accessible when GP11 grounded
//!
//! All features are gated on sd_available() — if no SD card is present,
//! none of this code has any effect.

use defmt::*;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_rp::{gpio::Output, spi::Spi, peripherals::SPI0};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{
    sdcard::DummyCsPin as SdDummyCs, Block, BlockDevice, BlockIdx,
    SdCard, TimeSource, Timestamp,
};
use static_cell::StaticCell;

pub type SpiDev = ExclusiveDevice<Spi<'static, SPI0, embassy_rp::spi::Blocking>, Output<'static>, Delay>;
type Card = SdCard<SpiDev, SdDummyCs, Delay>;

static SD_AVAILABLE:  AtomicBool = AtomicBool::new(false);
static SD_BLOCK_COUNT: AtomicU32 = AtomicU32::new(0);
static VISIBLE_START:  AtomicU32 = AtomicU32::new(2048);
static VISIBLE_SIZE:   AtomicU32 = AtomicU32::new(0);
static HIDDEN_START:   AtomicU32 = AtomicU32::new(0);
static HIDDEN_SIZE:    AtomicU32 = AtomicU32::new(0);

static SD_CARD: StaticCell<Mutex<CriticalSectionRawMutex, Card>> = StaticCell::new();
static mut SD_CARD_REF: Option<&'static Mutex<CriticalSectionRawMutex, Card>> = None;

// ---------------------------------------------------------------------------
// Dummy time source for embedded-sdmmc
// ---------------------------------------------------------------------------
struct DummyTime;
impl TimeSource for DummyTime {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp { year_since_1970: 54, zero_indexed_month: 0,
                    zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
    }
}

// ---------------------------------------------------------------------------
// Init — only does anything if SPI probe succeeds
// ---------------------------------------------------------------------------

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
        compute_partition_offsets(block_count);
    }
    block_count
}

fn compute_partition_offsets(total: u32) {
    let align     = 2048u32;
    let vis_start = align;
    let vis_end   = (((total as u64 * 9 / 10) / align as u64) * align as u64) as u32;
    let hid_start = vis_end;
    let hid_end   = ((total as u64 / align as u64) * align as u64) as u32;
    VISIBLE_START.store(vis_start,           Ordering::Relaxed);
    VISIBLE_SIZE .store(vis_end - vis_start,  Ordering::Relaxed);
    HIDDEN_START .store(hid_start,            Ordering::Relaxed);
    HIDDEN_SIZE  .store(hid_end - hid_start,  Ordering::Relaxed);
}

pub fn sd_available()        -> bool { SD_AVAILABLE.load(Ordering::Relaxed) }
pub fn sd_block_count()      -> u32  { SD_BLOCK_COUNT.load(Ordering::Relaxed) }
pub fn visible_block_count() -> u32  { VISIBLE_SIZE.load(Ordering::Relaxed) }
pub fn hidden_block_count()  -> u32  { HIDDEN_SIZE.load(Ordering::Relaxed) }

// ---------------------------------------------------------------------------
// Raw block I/O
// ---------------------------------------------------------------------------

pub fn read_block_raw(lba: u32, buf: &mut [u8; 512]) -> bool {
    let Some(m) = (unsafe { SD_CARD_REF }) else { buf.fill(0); return false; };
    let Ok(card) = m.try_lock() else { buf.fill(0); return true; };
    let mut block = [Block::new()];
    match card.read(&mut block, BlockIdx(lba), "msc") {
        Ok(()) => { buf.copy_from_slice(&block[0].contents); true }
        Err(e) => { warn!("[msc_sd] read {} err: {:?}", lba, defmt::Debug2Format(&e)); false }
    }
}

pub fn write_block_raw(lba: u32, buf: &[u8; 512]) -> bool {
    let Some(m) = (unsafe { SD_CARD_REF }) else { return false; };
    let Ok(card) = m.try_lock() else { return false; };
    let mut block = [Block::new()];
    block[0].contents.copy_from_slice(buf);
    match card.write(&block, BlockIdx(lba)) {
        Ok(()) => true,
        Err(e) => { warn!("[msc_sd] write {} err: {:?}", lba, defmt::Debug2Format(&e)); false }
    }
}

// ---------------------------------------------------------------------------
// Partition-relative I/O
// ---------------------------------------------------------------------------

pub fn read_block_visible(lba: u32, buf: &mut [u8; 512]) -> bool {
    let (s, n) = (VISIBLE_START.load(Ordering::Relaxed), VISIBLE_SIZE.load(Ordering::Relaxed));
    if n == 0 || lba >= n { buf.fill(0); return false; }
    read_block_raw(s + lba, buf)
}
pub fn write_block_visible(lba: u32, buf: &[u8; 512]) -> bool {
    let (s, n) = (VISIBLE_START.load(Ordering::Relaxed), VISIBLE_SIZE.load(Ordering::Relaxed));
    if n == 0 || lba >= n { return false; }
    write_block_raw(s + lba, buf)
}
pub fn read_block_hidden(lba: u32, buf: &mut [u8; 512]) -> bool {
    let (s, n) = (HIDDEN_START.load(Ordering::Relaxed), HIDDEN_SIZE.load(Ordering::Relaxed));
    if n == 0 || lba >= n { buf.fill(0); return false; }
    read_block_raw(s + lba, buf)
}
pub fn write_block_hidden(lba: u32, buf: &[u8; 512]) -> bool {
    let (s, n) = (HIDDEN_START.load(Ordering::Relaxed), HIDDEN_SIZE.load(Ordering::Relaxed));
    if n == 0 || lba >= n { return false; }
    write_block_raw(s + lba, buf)
}

// ---------------------------------------------------------------------------
// FAT32 formatter
// ---------------------------------------------------------------------------

fn make_partition_entry(lba_start: u32, lba_count: u32, bootable: bool) -> [u8; 16] {
    let mut e = [0u8; 16];
    e[0] = if bootable { 0x80 } else { 0x00 };
    e[1] = 0xFE; e[2] = 0xFF; e[3] = 0xFF;
    e[4] = 0x0C; // FAT32 with LBA
    e[5] = 0xFE; e[6] = 0xFF; e[7] = 0xFF;
    e[8..12].copy_from_slice(&lba_start.to_le_bytes());
    e[12..16].copy_from_slice(&lba_count.to_le_bytes());
    e
}

/// Write a FAT32 filesystem to a partition.
/// `part_start` = absolute LBA of partition start, `part_size` = sectors
fn format_fat32(part_start: u32, part_size: u32, label: &[u8; 11]) {
    let spc: u32 = if part_size < 524288 { 8 } else { 64 }; // sectors per cluster
    let reserved: u32 = 32;
    let num_fats: u32 = 2;

    // FAT size: ceil((total_clusters + 2) / 128)  [128 = 512/4 entries per sector]
    let data_secs   = part_size - reserved;
    let total_clust = data_secs / spc;
    let fat_size    = (total_clust + 2 + 127) / 128;

    // --- VBR ---
    let mut vbr = [0u8; 512];
    vbr[0] = 0xEB; vbr[1] = 0x58; vbr[2] = 0x90;
    vbr[3..11].copy_from_slice(b"MSWIN4.1");
    vbr[11..13].copy_from_slice(&512u16.to_le_bytes());   // bytes/sector
    vbr[13] = spc as u8;                                   // sectors/cluster
    vbr[14..16].copy_from_slice(&(reserved as u16).to_le_bytes());
    vbr[16] = num_fats as u8;
    vbr[17..19].copy_from_slice(&0u16.to_le_bytes());      // root entry count = 0
    vbr[19..21].copy_from_slice(&0u16.to_le_bytes());      // total sectors 16 = 0
    vbr[21] = 0xF8;                                        // media
    vbr[22..24].copy_from_slice(&0u16.to_le_bytes());      // FAT size 16 = 0
    vbr[24..26].copy_from_slice(&63u16.to_le_bytes());     // sectors/track
    vbr[26..28].copy_from_slice(&255u16.to_le_bytes());    // heads
    vbr[28..32].copy_from_slice(&part_start.to_le_bytes()); // hidden sectors
    vbr[32..36].copy_from_slice(&part_size.to_le_bytes());  // total sectors 32
    vbr[36..40].copy_from_slice(&fat_size.to_le_bytes());   // FAT size 32
    vbr[40..42].copy_from_slice(&0u16.to_le_bytes());       // ext flags
    vbr[42..44].copy_from_slice(&0u16.to_le_bytes());       // FS version 0.0
    vbr[44..48].copy_from_slice(&2u32.to_le_bytes());       // root cluster = 2
    vbr[48..50].copy_from_slice(&1u16.to_le_bytes());       // FSInfo sector = 1
    vbr[50..52].copy_from_slice(&6u16.to_le_bytes());       // backup boot sector = 6
    vbr[64] = 0x80;                                         // drive number
    vbr[66] = 0x29;                                         // ext boot sig
    vbr[67..71].copy_from_slice(&0x12345678u32.to_le_bytes()); // volume serial
    vbr[71..82].copy_from_slice(label);
    vbr[82..90].copy_from_slice(b"FAT32   ");
    vbr[510] = 0x55; vbr[511] = 0xAA;
    write_block_raw(part_start, &vbr);

    // --- FSInfo (sector 1) ---
    let mut fsi = [0u8; 512];
    fsi[0..4].copy_from_slice(&0x41615252u32.to_le_bytes());
    fsi[484..488].copy_from_slice(&0x61417272u32.to_le_bytes());
    fsi[488..492].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // free count unknown
    fsi[492..496].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // next free unknown
    fsi[508..512].copy_from_slice(&0xAA550000u32.to_le_bytes());
    write_block_raw(part_start + 1, &fsi);

    // Backup VBR at sector 6
    write_block_raw(part_start + 6, &vbr);

    // --- FAT tables ---
    let fat1 = part_start + reserved;
    let fat2 = fat1 + fat_size;
    let zero = [0u8; 512];

    // First sector of each FAT
    let mut fat0 = [0u8; 512];
    fat0[0..4].copy_from_slice(&0x0FFFFFF8u32.to_le_bytes()); // entry 0: media
    fat0[4..8].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes()); // entry 1: EOC
    fat0[8..12].copy_from_slice(&0x0FFFFFFFu32.to_le_bytes()); // entry 2: root EOC
    write_block_raw(fat1, &fat0);
    write_block_raw(fat2, &fat0);
    for i in 1..fat_size {
        write_block_raw(fat1 + i, &zero);
        write_block_raw(fat2 + i, &zero);
    }

    // --- Root directory cluster (cluster 2) ---
    let data_start = fat2 + fat_size;
    let root_lba   = data_start; // cluster 2 = first data cluster

    // Volume label dir entry
    let mut root = [0u8; 512];
    root[0..11].copy_from_slice(label);
    root[11] = 0x08; // ATTR_VOLUME_ID
    write_block_raw(root_lba, &root);
    for i in 1..spc {
        write_block_raw(root_lba + i, &zero);
    }

    info!("[msc_sd] FAT32 formatted (fat_size={} clust={})", fat_size, total_clust);
}

fn is_fat32(part_start: u32) -> bool {
    let mut buf = [0u8; 512];
    if !read_block_raw(part_start, &mut buf) { return false; }
    buf[510] == 0x55 && buf[511] == 0xAA && &buf[82..90] == b"FAT32   "
}

/// Ensure SD has our 2-partition MBR and both partitions are FAT32-formatted.
/// Only runs if SD card is present. Safe to call on every boot — checks before writing.
/// Returns true if any work was done.
pub fn ensure_partitioned_and_formatted() -> bool {
    if !sd_available() { return false; }
    let _total    = SD_BLOCK_COUNT.load(Ordering::Relaxed);
    let vis_start = VISIBLE_START.load(Ordering::Relaxed);
    let vis_size  = VISIBLE_SIZE .load(Ordering::Relaxed);
    let hid_start = HIDDEN_START .load(Ordering::Relaxed);
    let hid_size  = HIDDEN_SIZE  .load(Ordering::Relaxed);
    let mut did_work = false;

    // Check MBR — both partitions must be type 0x0C (FAT32 LBA)
    let mut mbr = [0u8; 512];
    read_block_raw(0, &mut mbr);
    let has_mbr = mbr[510] == 0x55 && mbr[511] == 0xAA
               && mbr[450] == 0x0C  // partition 1 type
               && mbr[466] == 0x0C; // partition 2 type

    if !has_mbr {
        info!("[msc_sd] Writing MBR...");
        let p1 = make_partition_entry(vis_start, vis_size, true);
        let p2 = make_partition_entry(hid_start, hid_size, false);
        let mut new_mbr = [0u8; 512];
        new_mbr[446..462].copy_from_slice(&p1);
        new_mbr[462..478].copy_from_slice(&p2);
        new_mbr[510] = 0x55;
        new_mbr[511] = 0xAA;
        write_block_raw(0, &new_mbr);
        did_work = true;
    }

    // Only format if partition is not already a valid FAT32 filesystem.
    // is_fat32() reads the VBR and checks the FAT32 signature — if it's
    // already formatted this returns true and we skip the format entirely.
    if !is_fat32(vis_start) {
        info!("[msc_sd] Visible partition not FAT32 — formatting...");
        format_fat32(vis_start, vis_size, b"PICO DUCKY ");
        did_work = true;
    } else {
        info!("[msc_sd] Visible partition already FAT32 — skipping format");
    }

    if !is_fat32(hid_start) {
        info!("[msc_sd] Hidden partition not FAT32 — formatting...");
        format_fat32(hid_start, hid_size, b"LOOT       ");
        did_work = true;
    } else {
        info!("[msc_sd] Hidden partition already FAT32 — skipping format");
    }

    if did_work {
        crate::console_log::push("SD: partitioned & formatted");
    }
    did_work
}

// ---------------------------------------------------------------------------
// SD payload reading — reads a .dd file from the visible FAT32 partition
// ---------------------------------------------------------------------------

// We implement a minimal FAT32 reader directly using read_block_raw,
// because VolumeManager requires ownership of the BlockDevice and we
// can't hand ownership away from the static mutex.
//
// Algorithm:
//   1. Read VBR to get FAT layout params
//   2. Walk the root directory to find the filename
//   3. Follow the FAT chain to read all clusters

struct Fat32Info {
    part_start: u32,
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    reserved_sectors: u32,
    fat_start: u32,      // absolute LBA of FAT1
    data_start: u32,     // absolute LBA of data region
}

fn read_fat32_info(part_start: u32) -> Option<Fat32Info> {
    let mut buf = [0u8; 512];
    if !read_block_raw(part_start, &mut buf) { return None; }
    if buf[510] != 0x55 || buf[511] != 0xAA { return None; }
    if &buf[82..90] != b"FAT32   " { return None; }

    let bps  = u16::from_le_bytes([buf[11], buf[12]]) as u32;
    let spc  = buf[13] as u32;
    let rsv  = u16::from_le_bytes([buf[14], buf[15]]) as u32;
    let nfat = buf[16] as u32;
    let fat_sz32 = u32::from_le_bytes([buf[36], buf[37], buf[38], buf[39]]);

    let fat_start  = part_start + rsv;
    let data_start = fat_start + nfat * fat_sz32;

    Some(Fat32Info { part_start, bytes_per_sector: bps, sectors_per_cluster: spc,
                     reserved_sectors: rsv, fat_start, data_start })
}

fn cluster_to_lba(info: &Fat32Info, cluster: u32) -> u32 {
    info.data_start + (cluster - 2) * info.sectors_per_cluster
}

fn next_cluster(info: &Fat32Info, cluster: u32) -> u32 {
    // Each FAT32 entry is 4 bytes; 128 entries per 512-byte sector
    let fat_sector = info.fat_start + cluster / 128;
    let fat_offset = (cluster % 128) as usize * 4;
    let mut buf = [0u8; 512];
    if !read_block_raw(fat_sector, &mut buf) { return 0x0FFFFFFF; }
    u32::from_le_bytes([buf[fat_offset], buf[fat_offset+1],
                        buf[fat_offset+2], buf[fat_offset+3]]) & 0x0FFFFFFF
}

/// Read a file from the visible SD partition into `out`.
/// Returns bytes read, or None if not found.
pub fn read_payload_from_sd(filename: &str, out: &mut [u8]) -> Option<usize> {
    if !sd_available() { return None; }

    let part_start = VISIBLE_START.load(Ordering::Relaxed);
    let info = read_fat32_info(part_start)?;

    // Root directory starts at cluster 2
    let mut dir_cluster = 2u32;
    let mut found_cluster = 0u32;
    let mut found_size    = 0u32;
    let fname_upper = to_upper_83(filename)?; // convert to 8.3 uppercase

    'dir_search: loop {
        if dir_cluster >= 0x0FFFFFF8 { break; }
        let lba = cluster_to_lba(&info, dir_cluster);
        for sec in 0..info.sectors_per_cluster {
            let mut buf = [0u8; 512];
            if !read_block_raw(lba + sec, &mut buf) { break; }
            let mut i = 0;
            while i + 32 <= 512 {
                let entry = &buf[i..i+32];
                if entry[0] == 0x00 { break 'dir_search; } // end of dir
                if entry[0] == 0xE5 { i += 32; continue; } // deleted
                if entry[11] == 0x0F { i += 32; continue; } // LFN entry
                // Compare 8.3 name (11 bytes at offset 0)
                if &entry[0..11] == &fname_upper {
                    found_cluster = ((entry[20] as u32) << 8 | entry[21] as u32) << 16
                                  | (entry[26] as u32) << 8 | entry[27] as u32;
                    found_size    = u32::from_le_bytes([entry[28], entry[29], entry[30], entry[31]]);
                    break 'dir_search;
                }
                i += 32;
            }
        }
        dir_cluster = next_cluster(&info, dir_cluster);
    }

    if found_cluster < 2 { return None; }

    // Read file data following FAT chain
    let mut total   = 0usize;
    let mut cluster = found_cluster;
    let bytes_left  = found_size as usize;

    while cluster < 0x0FFFFFF8 && total < bytes_left && total < out.len() {
        let lba = cluster_to_lba(&info, cluster);
        for sec in 0..info.sectors_per_cluster {
            if total >= bytes_left || total >= out.len() { break; }
            let mut buf = [0u8; 512];
            if !read_block_raw(lba + sec, &mut buf) { break; }
            let take = (bytes_left - total).min(out.len() - total).min(512);
            out[total..total+take].copy_from_slice(&buf[..take]);
            total += take;
        }
        cluster = next_cluster(&info, cluster);
    }

    if total > 0 { Some(total) } else { None }
}

/// Convert a filename like "payload.dd" to FAT32 8.3 format (11 bytes, space-padded, uppercase)
fn to_upper_83(name: &str) -> Option<[u8; 11]> {
    let mut out = [b' '; 11];
    let (base, ext) = if let Some(dot) = name.rfind('.') {
        (&name[..dot], &name[dot+1..])
    } else {
        (name, "")
    };
    if base.len() > 8 || ext.len() > 3 { return None; }
    for (i, c) in base.bytes().enumerate() {
        out[i] = c.to_ascii_uppercase();
    }
    for (i, c) in ext.bytes().enumerate() {
        out[8+i] = c.to_ascii_uppercase();
    }
    Some(out)
}
