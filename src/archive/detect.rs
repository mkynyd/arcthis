use std::fs::File;
use std::io::{self, Read};

use flate2::read::GzDecoder;

use super::ArchiveLocator;
use crate::error::{ArcthisError, Result};
use crate::model::ArchiveFormat;

const ZIP_LOCAL_FILE: &[u8; 4] = b"PK\x03\x04";
const ZIP_EMPTY: &[u8; 4] = b"PK\x05\x06";
const ZIP_SPANNED: &[u8; 4] = b"PK\x07\x08";

pub fn detect(locator: &ArchiveLocator) -> Result<ArchiveFormat> {
    let mut file =
        File::open(locator.path()).map_err(|error| ArcthisError::io("opening archive", error))?;
    let mut prefix = [0_u8; 512];
    let read = read_prefix(&mut file, &mut prefix)
        .map_err(|error| ArcthisError::io("reading archive signature", error))?;

    if read >= 4
        && (&prefix[..4] == ZIP_LOCAL_FILE
            || &prefix[..4] == ZIP_EMPTY
            || &prefix[..4] == ZIP_SPANNED)
    {
        return Ok(ArchiveFormat::Zip);
    }

    if read >= 2 && prefix[..2] == [0x1f, 0x8b] {
        let file = File::open(locator.path())
            .map_err(|error| ArcthisError::io("reopening gzip archive", error))?;
        let mut decoder = GzDecoder::new(file);
        let mut header = [0_u8; 512];
        let decompressed = read_prefix(&mut decoder, &mut header).map_err(|error| {
            ArcthisError::InvalidArchive {
                message: format!("invalid gzip stream: {error}"),
            }
        })?;
        if decompressed == 512 && is_tar_header(&header) {
            return Ok(ArchiveFormat::TarGzip);
        }
        return Err(ArcthisError::UnsupportedFormat {
            path: locator.path().to_path_buf(),
        });
    }

    if read == 512 && is_tar_header(&prefix) {
        return Ok(ArchiveFormat::Tar);
    }

    Err(ArcthisError::UnsupportedFormat {
        path: locator.path().to_path_buf(),
    })
}

fn read_prefix(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut total = 0;
    while total < buffer.len() {
        match reader.read(&mut buffer[total..])? {
            0 => break,
            count => total += count,
        }
    }
    Ok(total)
}

fn is_tar_header(header: &[u8; 512]) -> bool {
    if header.iter().all(|byte| *byte == 0) {
        return true;
    }

    let stored = parse_octal(&header[148..156]);
    let Some(stored) = stored else {
        return false;
    };
    let computed: u64 = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum();
    stored == computed
}

fn parse_octal(field: &[u8]) -> Option<u64> {
    let text = field
        .iter()
        .copied()
        .skip_while(|byte| *byte == b' ' || *byte == 0)
        .take_while(|byte| (b'0'..=b'7').contains(byte))
        .collect::<Vec<_>>();
    if text.is_empty() {
        return Some(0);
    }
    std::str::from_utf8(&text)
        .ok()
        .and_then(|value| u64::from_str_radix(value, 8).ok())
}

#[cfg(test)]
mod tests {
    use super::is_tar_header;

    #[test]
    fn zero_block_is_a_valid_empty_tar_header() {
        assert!(is_tar_header(&[0_u8; 512]));
    }

    #[test]
    fn arbitrary_data_is_not_a_tar_header() {
        assert!(!is_tar_header(&[b'x'; 512]));
    }
}
