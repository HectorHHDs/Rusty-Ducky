//! usb/msc.rs — USB Mass Storage Class, Bulk-Only Transport
//!
//! Presents the SD card as a FAT32 removable USB drive to the host.
//! ATTACKMODE STORAGE or ATTACKMODE HID STORAGE enables this.
//!
//! Protocol: SCSI Transparent Command Set over USB BOT (Bulk-Only Transport)

use defmt::*;
use embassy_usb::driver::{Driver, EndpointIn, EndpointOut};
use core::sync::atomic::AtomicBool;

/// Set true when GP11 is grounded — exposes hidden 10% partition only
pub static HIDDEN_MODE: AtomicBool = AtomicBool::new(false);
use embassy_time::{Duration, Timer};

// ---------------------------------------------------------------------------
// SCSI / BOT constants
// ---------------------------------------------------------------------------

const CBW_SIGNATURE: u32 = 0x43425355; // "USBC"
const CSW_SIGNATURE: u32 = 0x53425355; // "USBS"
const CSW_PASS: u8 = 0x00;
const CSW_FAIL: u8 = 0x01;
const BLOCK_SIZE: u32 = 512;

const SCSI_TEST_UNIT_READY:              u8 = 0x00;
const SCSI_REQUEST_SENSE:                u8 = 0x03;
const SCSI_INQUIRY:                      u8 = 0x12;
const SCSI_MODE_SENSE_6:                 u8 = 0x1A;
const SCSI_PREVENT_ALLOW_MEDIUM_REMOVAL: u8 = 0x1E;
const SCSI_READ_CAPACITY_10:             u8 = 0x25;
const SCSI_READ_10:                      u8 = 0x28;
const SCSI_WRITE_10:                     u8 = 0x2A;

#[rustfmt::skip]
const INQUIRY_RESP: [u8; 36] = [
    0x00, 0x80, 0x04, 0x02, 0x1F, 0x00, 0x00, 0x00,
    b'D', b'U', b'C', b'K', b'Y', b' ', b' ', b' ',
    b'P', b'i', b'c', b'o', b'P', b'a', b'y', b'l', b'o', b'a', b'd', b' ', b' ', b' ', b' ', b' ',
    b'1', b'.', b'0', b' ',
];

// ---------------------------------------------------------------------------
// CBW parsing
// ---------------------------------------------------------------------------

struct Cbw {
    tag:      u32,
    data_len: u32,
    flags:    u8,
    cb:       [u8; 16],
}

impl Cbw {
    fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 31 { return None; }
        if u32::from_le_bytes([b[0],b[1],b[2],b[3]]) != CBW_SIGNATURE { return None; }
        let mut cb = [0u8; 16];
        cb.copy_from_slice(&b[15..31]);
        Some(Self {
            tag:      u32::from_le_bytes([b[4],b[5],b[6],b[7]]),
            data_len: u32::from_le_bytes([b[8],b[9],b[10],b[11]]),
            flags:    b[12],
            cb,
        })
    }
    fn is_data_in(&self) -> bool { self.flags & 0x80 != 0 }
}

fn make_csw(tag: u32, residue: u32, status: u8) -> [u8; 13] {
    let mut csw = [0u8; 13];
    csw[0..4].copy_from_slice(&CSW_SIGNATURE.to_le_bytes());
    csw[4..8].copy_from_slice(&tag.to_le_bytes());
    csw[8..12].copy_from_slice(&residue.to_le_bytes());
    csw[12] = status;
    csw
}

// ---------------------------------------------------------------------------
// MSC USB interface builder helper
// ---------------------------------------------------------------------------

pub struct MscEps<'d, D: Driver<'d>> {
    pub ep_out: D::EndpointOut,
    pub ep_in:  D::EndpointIn,
}

pub fn build_msc<'d, D: Driver<'d>>(
    builder: &mut embassy_usb::Builder<'d, D>,
) -> MscEps<'d, D> {
    let mut func  = builder.function(0x08, 0x06, 0x50); // MSC/SCSI/BOT
    let mut iface = func.interface();
    let mut alt   = iface.alt_setting(0x08, 0x06, 0x50, None);
    let ep_out = alt.endpoint_bulk_out(None, 64);
    let ep_in  = alt.endpoint_bulk_in(None, 64);
    info!("[msc] MSC interface registered");
    MscEps { ep_out, ep_in }
}

// ---------------------------------------------------------------------------
// MSC task — SCSI over BOT
// ---------------------------------------------------------------------------

pub async fn msc_task_inner<'d, D: Driver<'d>>(
    mut ep_out:    D::EndpointOut,
    mut ep_in:     D::EndpointIn,
    block_count:   u32,
) {
    info!("[msc] started ({} blocks = {}MB)", block_count, block_count / 2048);
    let mut cbw_buf  = [0u8; 64];
    let mut data_buf = [0u8; 512];

    loop {
        // Wait for CBW from host
        let n = match ep_out.read(&mut cbw_buf).await {
            Ok(n)  => n,
            Err(_) => { Timer::after(Duration::from_millis(10)).await; continue; }
        };

        let cbw = match Cbw::from_bytes(&cbw_buf[..n]) {
            Some(c) => c,
            None    => { warn!("[msc] bad CBW"); continue; }
        };

        let status = handle_scsi::<D>(&cbw, &mut ep_out, &mut ep_in, &mut data_buf, block_count).await;
        let csw = make_csw(cbw.tag, 0, status);
        let _ = ep_in.write(&csw).await;
    }
}

async fn handle_scsi<'d, D: Driver<'d>>(
    cbw:         &Cbw,
    ep_out:      &mut D::EndpointOut,
    ep_in:       &mut D::EndpointIn,
    data_buf:    &mut [u8; 512],
    block_count: u32,
) -> u8 {
    match cbw.cb[0] {
        SCSI_TEST_UNIT_READY => CSW_PASS,

        SCSI_INQUIRY => {
            let len = (cbw.data_len as usize).min(INQUIRY_RESP.len());
            let _ = ep_in.write(&INQUIRY_RESP[..len]).await;
            CSW_PASS
        }

        SCSI_READ_CAPACITY_10 => {
            let count = if HIDDEN_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                crate::usb::msc_sd::hidden_block_count()
            } else {
                crate::usb::msc_sd::visible_block_count()
            };
            let last_lba = count.saturating_sub(1);
            let mut resp = [0u8; 8];
            resp[0..4].copy_from_slice(&last_lba.to_be_bytes());
            resp[4..8].copy_from_slice(&BLOCK_SIZE.to_be_bytes());
            let _ = ep_in.write(&resp).await;
            CSW_PASS
        }

        SCSI_READ_10 => {
            let lba   = u32::from_be_bytes([cbw.cb[2],cbw.cb[3],cbw.cb[4],cbw.cb[5]]);
            let count = u16::from_be_bytes([cbw.cb[7],cbw.cb[8]]) as u32;
            for block in lba..lba+count {
                let ok = if HIDDEN_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                    crate::usb::msc_sd::read_block_hidden(block, data_buf)
                } else {
                    crate::usb::msc_sd::read_block_visible(block, data_buf)
                };
                if !ok { return CSW_FAIL; }
                for chunk in data_buf.chunks(64) {
                    if ep_in.write(chunk).await.is_err() { return CSW_FAIL; }
                }
            }
            CSW_PASS
        }

        SCSI_WRITE_10 => {
            let lba   = u32::from_be_bytes([cbw.cb[2],cbw.cb[3],cbw.cb[4],cbw.cb[5]]);
            let count = u16::from_be_bytes([cbw.cb[7],cbw.cb[8]]) as u32;
            for block in lba..lba+count {
                let mut offset = 0usize;
                while offset < 512 {
                    let n = (512 - offset).min(64);
                    match ep_out.read(&mut data_buf[offset..offset+n]).await {
                        Ok(r)  => offset += r,
                        Err(_) => return CSW_FAIL,
                    }
                }
                let ok = if HIDDEN_MODE.load(core::sync::atomic::Ordering::Relaxed) {
                    crate::usb::msc_sd::write_block_hidden(block, data_buf)
                } else {
                    crate::usb::msc_sd::write_block_visible(block, data_buf)
                };
                if !ok { return CSW_FAIL; }
            }
            CSW_PASS
        }

        SCSI_REQUEST_SENSE => {
            let sense = [
                0x70,0x00,0x00,0x00,0x00,0x00,0x00,0x0A,
                0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
            ];
            let len = (cbw.data_len as usize).min(sense.len());
            let _ = ep_in.write(&sense[..len]).await;
            CSW_PASS
        }

        SCSI_MODE_SENSE_6 => {
            let _ = ep_in.write(&[0x03,0x00,0x00,0x00]).await;
            CSW_PASS
        }

        SCSI_PREVENT_ALLOW_MEDIUM_REMOVAL => CSW_PASS,

        other => {
            warn!("[msc] unhandled SCSI 0x{:02X}", other);
            if cbw.data_len > 0 {
                if cbw.is_data_in() { let _ = ep_in.write(&[]).await; }
                else { let mut d = [0u8;64]; let _ = ep_out.read(&mut d).await; }
            }
            CSW_FAIL
        }
    }
}
