use std::io::Read;

use bzip2::read::MultiBzDecoder;
use flate2::read::MultiGzDecoder;
use lzma_rust2::XzReader;

use crate::error::{ArcthisError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamCompression {
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

pub(crate) fn decoder(
    reader: Box<dyn Read>,
    compression: StreamCompression,
) -> Result<Box<dyn Read>> {
    match compression {
        StreamCompression::Gzip => Ok(Box::new(MultiGzDecoder::new(reader))),
        StreamCompression::Bzip2 => Ok(Box::new(MultiBzDecoder::new(reader))),
        StreamCompression::Xz => Ok(Box::new(XzReader::new(reader, true))),
        StreamCompression::Zstd => zstd::stream::read::Decoder::new(reader)
            .map(|reader| Box::new(reader) as Box<dyn Read>)
            .map_err(|error| ArcthisError::InvalidArchive {
                message: format!("invalid Zstandard stream: {error}"),
            }),
    }
}
