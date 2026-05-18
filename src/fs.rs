//! fs.rs — Flash payload storage via dedicated flash task
//!
//! Flash layout (2MB total):
//!   0x00000000 .. 0x000FFFFF  firmware (1MB)
//!   0x00100000 .. 0x0013FFFF  payload.dd  slot (256KB)
//!   0x00140000 .. 0x0017FFFF  payload2.dd slot (256KB)
//!   0x00180000 .. 0x001BFFFF  payload3.dd slot (256KB)
//!   0x001C0000 .. 0x001FFFFF  payload4.dd slot (256KB)
//!
//! Each slot:
//!   [0..4]   magic 0xDDDDDDDD
//!   [4..8]   length u32 LE
//!   [8..]    payload bytes
//!
//! Flash ops run in flash_task so USB stays responsive.

use defmt::*;
use heapless::{String, Vec};
use embassy_rp::flash::{Async, ERASE_SIZE, Flash};
use embassy_rp::peripherals::FLASH;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal};
use embassy_time::Delay;
use embedded_hal::spi::SpiDevice;
use embedded_sdmmc::{SdCard, VolumeManager, VolumeIdx, Mode, TimeSource, Timestamp};

pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
pub const DATA_BUF_SIZE:     usize = 32768;  // 32KB — saves 32KB RAM vs 64KB
const FLASH_SIZE:  usize = 2 * 1024 * 1024;
const MAGIC:       u32   = 0xDDDD_DDDD;
const SLOT_SIZE: usize = 256 * 1024;
const SLOT_OFFSET: [u32; 4] = [0x00100000, 0x00140000, 0x00180000, 0x001C0000];
const NUM_SLOTS:   usize = 4;

pub const PAYLOAD_NAMES: [&str; 4] = [
    "payload.dd", "payload2.dd", "payload3.dd", "loot.bin",
];

fn slot_index(path: &str) -> Option<usize> {
    PAYLOAD_NAMES.iter().position(|&n| n.eq_ignore_ascii_case(path))
}

/// Public version for use by exfil.rs
pub fn slot_index_pub(path: &str) -> Option<usize> {
    slot_index(path)
}

// ---------------------------------------------------------------------------
// Flash task IPC
// ---------------------------------------------------------------------------

pub enum FlashReq {
    Exists       { slot: usize },
    Read         { slot: usize },
    Write        { slot: usize, len: usize },
    Delete       { slot: usize },
    List,
    FormatAll,
    ReadOffset   { slot: usize, offset: usize, len: usize },
    StreamBegin  { slot: usize },           // erase slot, prepare for streaming
    StreamChunk  { offset: usize, len: usize }, // write PAGE_BUF[..len] at offset
    StreamFinish { slot: usize, total: usize }, // write length header
}

pub enum FlashResp {
    Exists(bool),
    Read(Result<usize, &'static str>),
    ReadOffset(Result<usize, &'static str>),  // returns actual bytes read
    Write(Result<(), &'static str>),
    Delete(Result<(), &'static str>),
    List(u8),
    FormatAll(Result<(), &'static str>),
    StreamBegin(Result<(), &'static str>),
    StreamChunk(Result<(), &'static str>),
    StreamFinish(Result<(), &'static str>),
}

pub static mut FLASH_DATA_BUF: [u8; DATA_BUF_SIZE] = [0u8; DATA_BUF_SIZE];
// 256-byte page buffer for streaming writes
pub static mut PAGE_BUF: [u8; 256] = [0xFFu8; 256];

pub static FLASH_REQ:  Channel<CriticalSectionRawMutex, FlashReq, 1> = Channel::new();
pub static FLASH_RESP: Signal<CriticalSectionRawMutex, FlashResp>     = Signal::new();

// ---------------------------------------------------------------------------
// Flash task
// ---------------------------------------------------------------------------

#[embassy_executor::task]
pub async fn flash_task(mut flash: Flash<'static, FLASH, Async, FLASH_SIZE>) {
    info!("[flash] task started");
    loop {
        let req = FLASH_REQ.receive().await;
        let resp = match req {
            FlashReq::Exists { slot } => FlashResp::Exists(slot_exists(&mut flash, slot)),
            FlashReq::Read         { slot }                  => FlashResp::Read(slot_read(&mut flash, slot)),
            FlashReq::ReadOffset   { slot, offset, len }     => FlashResp::ReadOffset(slot_read_offset(&mut flash, slot, offset, len)),
            FlashReq::Write  { slot, len } => FlashResp::Write(slot_write(&mut flash, slot, len)),
            FlashReq::Delete { slot } => FlashResp::Delete(slot_delete(&mut flash, slot)),
            FlashReq::List            => FlashResp::List(slot_list(&mut flash)),
            FlashReq::FormatAll       => FlashResp::FormatAll(slot_format_all(&mut flash)),
            FlashReq::StreamBegin { slot } => FlashResp::StreamBegin(stream_begin(&mut flash, slot)),
            FlashReq::StreamChunk { offset, len } => FlashResp::StreamChunk(stream_chunk(&mut flash, offset, len)),
            FlashReq::StreamFinish { slot, total } => FlashResp::StreamFinish(stream_finish(&mut flash, slot, total)),
        };
        FLASH_RESP.signal(resp);
    }
}

fn slot_exists(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, slot: usize) -> bool {
    let mut hdr = [0u8; 8];
    if flash.blocking_read(SLOT_OFFSET[slot], &mut hdr).is_err() { return false; }
    let magic = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    let len   = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
    magic == MAGIC && len > 0 && len as usize <= DATA_BUF_SIZE
}

fn slot_read(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, slot: usize)
    -> Result<usize, &'static str>
{
    let mut hdr = [0u8; 8];
    flash.blocking_read(SLOT_OFFSET[slot], &mut hdr).map_err(|_| "read error")?;
    let magic = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    let len   = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
    if magic != MAGIC || len == 0 || len > DATA_BUF_SIZE { return Err("file not found"); }
    flash.blocking_read(SLOT_OFFSET[slot] + 8, unsafe { &mut FLASH_DATA_BUF[..len] })
        .map_err(|_| "read error")?;
    Ok(len)
}

fn slot_read_offset(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, slot: usize, offset: usize, len: usize)
    -> Result<usize, &'static str>
{
    let mut hdr = [0u8; 8];
    flash.blocking_read(SLOT_OFFSET[slot], &mut hdr).map_err(|_| "read error")?;
    let magic   = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
    let total   = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
    if magic != MAGIC || total == 0 { return Err("file not found"); }
    if offset >= total { return Ok(0); }
    let n = len.min(total - offset).min(DATA_BUF_SIZE);
    flash.blocking_read(SLOT_OFFSET[slot] + 8 + offset as u32, unsafe { &mut FLASH_DATA_BUF[..n] })
        .map_err(|_| "read error")?;
    Ok(n)
}

fn slot_write(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, slot: usize, len: usize)
    -> Result<(), &'static str>
{
    if len > DATA_BUF_SIZE { 
        defmt::error!("[flash] slot_write: len {} > DATA_BUF_SIZE {}", len, DATA_BUF_SIZE);
        return Err("too large"); 
    }
    if slot >= NUM_SLOTS {
        defmt::error!("[flash] slot_write: slot {} out of range", slot);
        return Err("slot out of range");
    }
    defmt::info!("[flash] slot_write: slot={} len={} base=0x{:08x}", slot, len, SLOT_OFFSET[slot]);
    let base = SLOT_OFFSET[slot];
    let num_sectors = SLOT_SIZE / ERASE_SIZE;
    for s in 0..num_sectors {
        let addr = base + (s * ERASE_SIZE) as u32;
        flash.blocking_erase(addr, addr + ERASE_SIZE as u32).map_err(|_| "erase error")?;
    }
    let mut hdr = [0xFFu8; 8];
    hdr[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    hdr[4..8].copy_from_slice(&(len as u32).to_le_bytes());
    flash.blocking_write(base, &hdr).map_err(|_| "write header")?;
    let mut off = base + 8;
    let mut pos = 0usize;
    let mut page = [0xFFu8; 256];
    while pos < len {
        let n = (len - pos).min(256);
        page.fill(0xFF);
        page[..n].copy_from_slice(unsafe { &FLASH_DATA_BUF[pos..pos+n] });
        flash.blocking_write(off, &page).map_err(|_| "write error")?;
        off += 256;
        pos += n;
    }
    info!("[flash] wrote {} bytes to slot {}", len, slot);
    Ok(())
}

fn slot_delete(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, slot: usize)
    -> Result<(), &'static str>
{
    flash.blocking_erase(SLOT_OFFSET[slot], SLOT_OFFSET[slot] + ERASE_SIZE as u32)
        .map_err(|_| "erase error")?;
    Ok(())
}

fn slot_list(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>) -> u8 {
    let mut mask = 0u8;
    for i in 0..NUM_SLOTS {
        if slot_exists(flash, i) { mask |= 1 << i; }
    }
    mask
}

fn stream_begin(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, slot: usize)
    -> Result<(), &'static str>
{
    let base = SLOT_OFFSET[slot];
    let num_sectors = SLOT_SIZE / ERASE_SIZE;
    for s in 0..num_sectors {
        let addr = base + (s * ERASE_SIZE) as u32;
        flash.blocking_erase(addr, addr + ERASE_SIZE as u32).map_err(|_| "erase error")?;
    }
    info!("[flash] stream_begin slot {}", slot);
    Ok(())
}

fn stream_chunk(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, offset: usize, _len: usize)
    -> Result<(), &'static str>
{
    // offset is byte offset from start of data (after 256-byte header page)
    // We write a full 256-byte page — PAGE_BUF must be filled before calling
    // offset encodes: upper 16 bits = slot index, lower 16 bits = page index
    let slot = (offset >> 16) & 0xF;
    let page = offset & 0xFFFF;
    let addr = SLOT_OFFSET[slot] + 8 + (page * 256) as u32; // data starts at offset 8
    flash.blocking_write(addr, unsafe { &PAGE_BUF[..256] }).map_err(|_| "write error")?;
    Ok(())
}

fn stream_finish(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>, slot: usize, total: usize)
    -> Result<(), &'static str>
{
    // Write 8-byte header at slot base (must be 4-byte aligned, write as 8 bytes)
    let mut hdr = [0xFFu8; 8];
    hdr[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    hdr[4..8].copy_from_slice(&(total as u32).to_le_bytes());
    flash.blocking_write(SLOT_OFFSET[slot], &hdr).map_err(|_| "write header")?;
    info!("[flash] stream_finish slot {} total {} bytes", slot, total);
    Ok(())
}

fn slot_format_all(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>)
    -> Result<(), &'static str>
{
    for i in 0..NUM_SLOTS {
        flash.blocking_erase(SLOT_OFFSET[i], SLOT_OFFSET[i] + ERASE_SIZE as u32)
            .map_err(|_| "erase error")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// FlashFs — async proxy to flash_task
// ---------------------------------------------------------------------------

pub struct FlashFs;

impl FlashFs {
    pub fn new() -> Self { Self }

    async fn req(&self, req: FlashReq) -> FlashResp {
        FLASH_REQ.send(req).await;
        FLASH_RESP.wait().await
    }

    pub async fn file_exists_async(&self, path: &str) -> bool {
        let Some(i) = slot_index(path) else { return false; };
        matches!(self.req(FlashReq::Exists { slot: i }).await, FlashResp::Exists(true))
    }

    pub async fn read_file_async(&self, path: &str) -> Result<usize, &'static str> {
        let i = slot_index(path).ok_or("unknown filename")?;
        match self.req(FlashReq::Read { slot: i }).await {
            FlashResp::Read(r) => r,
            _ => Err("protocol error"),
        }
    }

    /// Read len bytes from offset into FLASH_DATA_BUF. Returns bytes actually read.
    pub async fn read_file_offset_async(&self, path: &str, offset: usize, len: usize) -> Result<usize, &'static str> {
        let i = slot_index(path).ok_or("unknown filename")?;
        match self.req(FlashReq::ReadOffset { slot: i, offset, len }).await {
            FlashResp::ReadOffset(r) => r,
            _ => Err("protocol error"),
        }
    }

    pub async fn write_file_async(&self, path: &str, data: &[u8]) -> Result<(), &'static str> {
        let i = slot_index(path).ok_or("unknown filename — use payload.dd .. payload4.dd")?;
        if data.len() > DATA_BUF_SIZE { return Err("payload too large (max 64KB)"); }
        unsafe { FLASH_DATA_BUF[..data.len()].copy_from_slice(data); }
        match self.req(FlashReq::Write { slot: i, len: data.len() }).await {
            FlashResp::Write(r) => r,
            _ => Err("protocol error"),
        }
    }

    pub async fn delete_file_async(&self, path: &str) -> Result<(), &'static str> {
        let i = slot_index(path).ok_or("file not found")?;
        match self.req(FlashReq::Delete { slot: i }).await {
            FlashResp::Delete(r) => r,
            _ => Err("protocol error"),
        }
    }

    pub async fn list_dd_files_async(&self) -> Vec<String<32>, 3> {
        let mut out: Vec<String<32>, 3> = Vec::new();
        match self.req(FlashReq::List).await {
            FlashResp::List(mask) => {
                for i in 0..NUM_SLOTS {
                    // Don't show loot.bin in payload list
                    if PAYLOAD_NAMES[i] == "loot.bin" { continue; }
                    if mask & (1 << i) != 0 {
                        let mut s: String<32> = String::new();
                        let _ = s.push_str(PAYLOAD_NAMES[i]);
                        let _ = out.push(s);
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Write FLASH_DATA_BUF[..len] directly to a slot — no extra copy needed
    pub async fn write_raw_async(&self, slot: usize, len: usize) -> Result<(), &'static str> {
        if len > DATA_BUF_SIZE { return Err("too large"); }
        match self.req(FlashReq::Write { slot, len }).await {
            FlashResp::Write(r) => r,
            _ => Err("protocol error"),
        }
    }

    pub async fn format_all_async(&self) -> Result<(), &'static str> {
        match self.req(FlashReq::FormatAll).await {
            FlashResp::FormatAll(r) => r,
            _ => Err("protocol error"),
        }
    }

    /// Begin a streaming write — erases the slot.
    pub async fn stream_begin_async(&self, path: &str) -> Result<usize, &'static str> {
        let i = slot_index(path).ok_or("unknown filename")?;
        match self.req(FlashReq::StreamBegin { slot: i }).await {
            FlashResp::StreamBegin(r) => r.map(|_| i),
            _ => Err("protocol error"),
        }
    }

    /// Write a 256-byte page. Copy data into PAGE_BUF first, pad with 0xFF.
    /// page_index is the 0-based page number within the data area.
    pub async fn stream_chunk_async(&self, slot: usize, page_index: usize) -> Result<(), &'static str> {
        let offset = (slot << 16) | page_index;
        match self.req(FlashReq::StreamChunk { offset, len: 256 }).await {
            FlashResp::StreamChunk(r) => r,
            _ => Err("protocol error"),
        }
    }

    /// Finalize — writes the header with actual byte count.
    pub async fn stream_finish_async(&self, slot: usize, total: usize) -> Result<(), &'static str> {
        match self.req(FlashReq::StreamFinish { slot, total }).await {
            FlashResp::StreamFinish(r) => r,
            _ => Err("protocol error"),
        }
    }
}

// ---------------------------------------------------------------------------
// DummyTime / DummyCsPin / SdFs
// ---------------------------------------------------------------------------

pub struct DummyTime;
impl TimeSource for DummyTime {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp { year_since_1970: 54, zero_indexed_month: 0,
                    zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
    }
}

pub struct DummyCsPin;
impl embedded_hal::digital::ErrorType for DummyCsPin {
    type Error = core::convert::Infallible;
}
impl embedded_hal::digital::OutputPin for DummyCsPin {
    fn set_high(&mut self) -> Result<(), Self::Error> { Ok(()) }
    fn set_low(&mut self)  -> Result<(), Self::Error> { Ok(()) }
}

pub struct SdFs<SPI: SpiDevice> {
    mgr: VolumeManager<SdCard<SPI, DummyCsPin, Delay>, DummyTime, 4, 4, 1>,
}

impl<SPI: SpiDevice> SdFs<SPI> {
    pub fn new(spi: SPI) -> Self {
        Self { mgr: VolumeManager::new(SdCard::new(spi, DummyCsPin, Delay), DummyTime) }
    }
    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8, DATA_BUF_SIZE>, &'static str> {
        let vol  = self.mgr.open_raw_volume(VolumeIdx(0)).map_err(|_| "open_volume")?;
        let dir  = self.mgr.open_root_dir(vol).map_err(|_| "open_root_dir")?;
        let file = self.mgr.open_file_in_dir(dir, path, Mode::ReadOnly).map_err(|_| "file not found")?;
        let mut buf: Vec<u8, DATA_BUF_SIZE> = Vec::new();
        let mut tmp = [0u8; 256];
        loop {
            match self.mgr.read(file, &mut tmp) {
                Ok(0)  => break,
                Ok(n)  => { if buf.extend_from_slice(&tmp[..n]).is_err() { break; } }
                Err(_) => break,
            }
        }
        Ok(buf)
    }
}
