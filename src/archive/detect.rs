use std::io::{self, Read};

use super::backend::ArchiveSource;
use super::codec::{StreamCompression, decoder};
use crate::error::{ArcthisError, Result};
use crate::model::ArchiveFormat;

const ZIP_LOCAL_FILE: &[u8; 4] = b"PK\x03\x04";
const ZIP_EMPTY: &[u8; 4] = b"PK\x05\x06";
const ZIP_SPANNED: &[u8; 4] = b"PK\x07\x08";
const SEVEN_Z: &[u8; 6] = b"7z\xBC\xAF\x27\x1C";
const RAR4: &[u8; 7] = b"Rar!\x1A\x07\x00";
const RAR5: &[u8; 8] = b"Rar!\x1A\x07\x01\x00";
const XZ: &[u8; 6] = b"\xFD7zXZ\0";
const ZSTD: &[u8; 4] = b"\x28\xB5\x2F\xFD";

pub fn detect(source: &ArchiveSource) -> Result<ArchiveFormat> {
    let mut file = source.reader()?;
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

    if read >= SEVEN_Z.len() && &prefix[..SEVEN_Z.len()] == SEVEN_Z {
        return Ok(ArchiveFormat::SevenZip);
    }

    if (read >= RAR4.len() && &prefix[..RAR4.len()] == RAR4)
        || (read >= RAR5.len() && &prefix[..RAR5.len()] == RAR5)
    {
        return Ok(ArchiveFormat::Rar);
    }

    if read >= 2 && prefix[..2] == [0x1f, 0x8b] {
        return detect_compressed(source, StreamCompression::Gzip);
    }

    if read >= 3 && &prefix[..3] == b"BZh" {
        return detect_compressed(source, StreamCompression::Bzip2);
    }

    if read >= XZ.len() && &prefix[..XZ.len()] == XZ {
        return detect_compressed(source, StreamCompression::Xz);
    }

    if read >= ZSTD.len() && &prefix[..ZSTD.len()] == ZSTD {
        return detect_compressed(source, StreamCompression::Zstd);
    }

    if read == 512 && is_tar_header(&prefix) {
        return Ok(ArchiveFormat::Tar);
    }

    Err(ArcthisError::UnsupportedFormat {
        path: source.name().to_path_buf(),
    })
}

fn detect_compressed(
    source: &ArchiveSource,
    compression: StreamCompression,
) -> Result<ArchiveFormat> {
    let mut reader = decoder(source.reader()?, compression)?;
    let mut header = [0_u8; 512];
    let decompressed =
        read_prefix(&mut reader, &mut header).map_err(|error| ArcthisError::InvalidArchive {
            message: format!("invalid compressed stream: {error}"),
        })?;
    let is_tar = decompressed == header.len() && is_tar_header(&header);
    Ok(match (compression, is_tar) {
        (StreamCompression::Gzip, true) => ArchiveFormat::TarGzip,
        (StreamCompression::Bzip2, true) => ArchiveFormat::TarBzip2,
        (StreamCompression::Xz, true) => ArchiveFormat::TarXz,
        (StreamCompression::Zstd, true) => ArchiveFormat::TarZstd,
        (StreamCompression::Gzip, false) => ArchiveFormat::Gzip,
        (StreamCompression::Bzip2, false) => ArchiveFormat::Bzip2,
        (StreamCompression::Xz, false) => ArchiveFormat::Xz,
        (StreamCompression::Zstd, false) => ArchiveFormat::Zstd,
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
