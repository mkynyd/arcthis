//! Preserve central-directory instances without replacing the ZIP codec.
//!
//! The pinned ZIP reader keys its directory by name. A duplicate instance is
//! opened through a seekable view with its original data prefix and exactly one
//! original central header. ZIP/ZIP64 parsing, passwords, decoding and CRC stay
//! in the ZIP crate; no compressed data is rewritten or materialized.
use std::io::{self, Read, Seek, SeekFrom};

use super::ReadSeek;
use crate::{ArcthisError, Result};

pub(super) fn records(reader: &mut dyn ReadSeek, start: u64) -> Result<Vec<Vec<u8>>> {
    let length = reader.seek(SeekFrom::End(0)).map_err(io_error)?;
    reader.seek(SeekFrom::Start(start)).map_err(io_error)?;
    let mut result = Vec::new();
    let mut retained = 0_usize;
    loop {
        let mut signature = [0_u8; 4];
        reader.read_exact(&mut signature).map_err(io_error)?;
        if signature != [0x50, 0x4b, 0x01, 0x02] {
            break;
        }
        let mut header = vec![0_u8; 46];
        header[..4].copy_from_slice(&signature);
        reader.read_exact(&mut header[4..]).map_err(io_error)?;
        let extra = [28, 30, 32]
            .into_iter()
            .map(|offset| usize::from(u16::from_le_bytes([header[offset], header[offset + 1]])))
            .sum::<usize>();
        retained = retained.checked_add(46 + extra).ok_or_else(budget_error)?;
        if retained > 256 * 1024 * 1024 {
            return Err(budget_error());
        }
        let position = reader.stream_position().map_err(io_error)?;
        if u64::try_from(extra).unwrap_or(u64::MAX) > length.saturating_sub(position) {
            return Err(ArcthisError::InvalidArchive {
                message: "truncated ZIP central directory".to_owned(),
            });
        }
        header.resize(46 + extra, 0);
        reader.read_exact(&mut header[46..]).map_err(io_error)?;
        result.push(header);
    }
    Ok(result)
}

pub(super) struct EntryView<'a> {
    reader: &'a mut dyn ReadSeek,
    prefix: u64,
    tail: Vec<u8>,
    position: u64,
}
impl<'a> EntryView<'a> {
    pub(super) fn new(
        reader: &'a mut dyn ReadSeek,
        prefix: u64,
        base: u64,
        record: &[u8],
    ) -> Result<Self> {
        let offset = prefix
            .checked_sub(base)
            .ok_or_else(|| ArcthisError::InvalidArchive {
                message: "invalid ZIP archive offset".to_owned(),
            })?;
        let mut tail = record.to_vec();
        if offset > u64::from(u32::MAX) {
            let zip64_offset = offset + u64::try_from(record.len()).unwrap_or(u64::MAX);
            tail.extend_from_slice(&[0x50, 0x4b, 0x06, 0x06]);
            tail.extend_from_slice(&44_u64.to_le_bytes());
            tail.extend_from_slice(&45_u16.to_le_bytes());
            tail.extend_from_slice(&45_u16.to_le_bytes());
            tail.extend_from_slice(&0_u32.to_le_bytes());
            tail.extend_from_slice(&0_u32.to_le_bytes());
            tail.extend_from_slice(&1_u64.to_le_bytes());
            tail.extend_from_slice(&1_u64.to_le_bytes());
            tail.extend_from_slice(
                &u64::try_from(record.len())
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            tail.extend_from_slice(&offset.to_le_bytes());
            tail.extend_from_slice(&[0x50, 0x4b, 0x06, 0x07]);
            tail.extend_from_slice(&0_u32.to_le_bytes());
            tail.extend_from_slice(&zip64_offset.to_le_bytes());
            tail.extend_from_slice(&1_u32.to_le_bytes());
        }
        tail.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        tail.extend_from_slice(&0_u32.to_le_bytes());
        tail.extend_from_slice(&1_u16.to_le_bytes());
        tail.extend_from_slice(&1_u16.to_le_bytes());
        tail.extend_from_slice(
            &u32::try_from(record.len())
                .map_err(|_| budget_error())?
                .to_le_bytes(),
        );
        tail.extend_from_slice(&u32::try_from(offset).unwrap_or(u32::MAX).to_le_bytes());
        tail.extend_from_slice(&0_u16.to_le_bytes());
        Ok(Self {
            reader,
            prefix,
            tail,
            position: 0,
        })
    }
    fn len(&self) -> u64 {
        self.prefix
            .saturating_add(u64::try_from(self.tail.len()).unwrap_or(u64::MAX))
    }
}
impl Read for EntryView<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = if self.position < self.prefix {
            let remaining = usize::try_from(self.prefix - self.position)
                .unwrap_or(usize::MAX)
                .min(buffer.len());
            self.reader.seek(SeekFrom::Start(self.position))?;
            self.reader.read(&mut buffer[..remaining])?
        } else {
            let offset = usize::try_from(self.position - self.prefix).unwrap_or(usize::MAX);
            let bytes = self.tail.get(offset..).unwrap_or_default();
            let count = buffer.len().min(bytes.len());
            buffer[..count].copy_from_slice(&bytes[..count]);
            count
        };
        self.position += u64::try_from(count).unwrap_or(u64::MAX);
        Ok(count)
    }
}
impl Seek for EntryView<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let next = match from {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.len()) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        self.position = u64::try_from(next)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid ZIP view seek"))?;
        Ok(self.position)
    }
}
fn io_error(error: io::Error) -> ArcthisError {
    ArcthisError::io("reading ZIP central directory", error)
}
fn budget_error() -> ArcthisError {
    ArcthisError::ResourceLimit {
        message: "ZIP central directory exceeds 256 MiB metadata budget".to_owned(),
    }
}
