//! Original-name bytes and strict opt-in decoding. CP437 mapping follows the
//! MIT-licensed ZIP dependency's standard IBM table (see `THIRD_PARTY_LICENSES`).
use crate::{ArcthisError, EntryPathEncoding, Result};

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    text
}
pub(crate) fn unhex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return invalid_hex();
    }
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = char::from(pair[0]).to_digit(16);
            let low = char::from(pair[1]).to_digit(16);
            match (high, low) {
                (Some(high), Some(low)) => {
                    u8::try_from(high * 16 + low).map_err(|_| ArcthisError::InvalidArchive {
                        message: "invalid raw path bytes".to_owned(),
                    })
                }
                _ => invalid_hex(),
            }
        })
        .collect()
}
fn invalid_hex<T>() -> Result<T> {
    Err(ArcthisError::InvalidArchive {
        message: "invalid raw path bytes".to_owned(),
    })
}

pub(crate) fn original_bytes(path: &str, encoding: EntryPathEncoding) -> Result<Vec<u8>> {
    if encoding != EntryPathEncoding::EscapedBytes {
        return Ok(path.as_bytes().to_vec());
    }
    let mut result = Vec::new();
    let mut bytes = path.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let a = bytes.next().ok_or_else(|| ArcthisError::InvalidArchive {
                message: "invalid escaped path".to_owned(),
            })?;
            let b = bytes.next().ok_or_else(|| ArcthisError::InvalidArchive {
                message: "invalid escaped path".to_owned(),
            })?;
            result.extend(unhex(std::str::from_utf8(&[a, b]).map_err(|_| {
                ArcthisError::InvalidArchive {
                    message: "invalid escaped path".to_owned(),
                }
            })?)?);
        } else {
            result.push(byte);
        }
    }
    Ok(result)
}
pub(crate) fn decode(bytes: &[u8], encoding: &str) -> Result<(String, EntryPathEncoding)> {
    let (text, kind) = match encoding {
        "cp437" => (
            bytes
                .iter()
                .map(|byte| {
                    if *byte < 128 {
                        char::from(*byte)
                    } else {
                        CP437[usize::from(*byte) - 128]
                    }
                })
                .collect(),
            EntryPathEncoding::Cp437,
        ),
        "utf8" | "utf-8" => (
            std::str::from_utf8(bytes)
                .map_err(|_| ArcthisError::InvalidArgument {
                    message: "entry name is not valid UTF-8".to_owned(),
                })?
                .to_owned(),
            EntryPathEncoding::Utf8,
        ),
        "gbk" | "shift-jis" | "shift_jis" => {
            let (codec, kind) = if encoding == "gbk" {
                (encoding_rs::GBK, EntryPathEncoding::Gbk)
            } else {
                (encoding_rs::SHIFT_JIS, EntryPathEncoding::ShiftJis)
            };
            let (text, errors) = codec.decode_without_bom_handling(bytes);
            if errors {
                return Err(ArcthisError::InvalidArgument {
                    message: format!("entry name cannot be decoded as {encoding}"),
                });
            }
            (text.into_owned(), kind)
        }
        _ => {
            return Err(ArcthisError::InvalidArgument {
                message: format!("unsupported filename encoding: {encoding}"),
            });
        }
    };
    Ok((text, kind))
}
const CP437: [char; 128] = [
    '\u{00c7}', '\u{00fc}', '\u{00e9}', '\u{00e2}', '\u{00e4}', '\u{00e0}', '\u{00e5}', '\u{00e7}',
    '\u{00ea}', '\u{00eb}', '\u{00e8}', '\u{00ef}', '\u{00ee}', '\u{00ec}', '\u{00c4}', '\u{00c5}',
    '\u{00c9}', '\u{00e6}', '\u{00c6}', '\u{00f4}', '\u{00f6}', '\u{00f2}', '\u{00fb}', '\u{00f9}',
    '\u{00ff}', '\u{00d6}', '\u{00dc}', '\u{00a2}', '\u{00a3}', '\u{00a5}', '\u{20a7}', '\u{0192}',
    '\u{00e1}', '\u{00ed}', '\u{00f3}', '\u{00fa}', '\u{00f1}', '\u{00d1}', '\u{00aa}', '\u{00ba}',
    '\u{00bf}', '\u{2310}', '\u{00ac}', '\u{00bd}', '\u{00bc}', '\u{00a1}', '\u{00ab}', '\u{00bb}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255d}', '\u{255c}', '\u{255b}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252c}', '\u{251c}', '\u{2500}', '\u{253c}', '\u{255e}', '\u{255f}',
    '\u{255a}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256c}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256b}',
    '\u{256a}', '\u{2518}', '\u{250c}', '\u{2588}', '\u{2584}', '\u{258c}', '\u{2590}', '\u{2580}',
    '\u{03b1}', '\u{00df}', '\u{0393}', '\u{03c0}', '\u{03a3}', '\u{03c3}', '\u{00b5}', '\u{03c4}',
    '\u{03a6}', '\u{0398}', '\u{03a9}', '\u{03b4}', '\u{221e}', '\u{03c6}', '\u{03b5}', '\u{2229}',
    '\u{2261}', '\u{00b1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00f7}', '\u{2248}',
    '\u{00b0}', '\u{2219}', '\u{00b7}', '\u{221a}', '\u{207f}', '\u{00b2}', '\u{25a0}', '\u{00a0}',
];
