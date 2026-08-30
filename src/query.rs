use std::io::{self, Write};

use globset::{Glob, GlobMatcher};
use serde::Serialize;
use sha2::{Digest, Sha256, Sha512};

use crate::archive::Archive;
use crate::error::{ArcthisError, Result};
use crate::model::{ArchiveEntry, EntryKind};

const BINARY_PROBE_BYTES: usize = 8 * 1024;
const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct FindResult {
    pub glob: String,
    pub matched: u64,
    pub entries: Vec<ArchiveEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    Sha256,
    Sha512,
}

impl HashAlgorithm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct HashResult {
    pub entry: String,
    pub algorithm: HashAlgorithm,
    pub digest: String,
    pub bytes_hashed: u64,
}

#[derive(Debug, Clone)]
pub struct GrepOptions {
    pub glob: Option<String>,
    pub max_entry_size: u64,
    pub max_matches: u64,
    pub scan_binary: bool,
}

impl Default for GrepOptions {
    fn default() -> Self {
        Self {
            glob: None,
            max_entry_size: 16 * 1024 * 1024,
            max_matches: 10_000,
            scan_binary: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct GrepMatch {
    pub path: String,
    pub line_number: u64,
    pub text: String,
    pub line_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
pub struct GrepResult {
    pub pattern: String,
    pub glob: Option<String>,
    pub files_scanned: u64,
    pub binary_files_skipped: u64,
    pub oversized_files_skipped: u64,
    pub bytes_scanned: u64,
    pub matches_truncated: bool,
    pub matches: Vec<GrepMatch>,
}

pub fn find(archive: &Archive, pattern: &str) -> Result<FindResult> {
    let matcher = build_matcher(pattern)?;
    let entries = archive
        .entries()?
        .into_iter()
        .filter(|entry| matcher.is_match(&entry.path))
        .collect::<Vec<_>>();
    Ok(FindResult {
        glob: pattern.to_owned(),
        matched: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        entries,
    })
}

pub fn hash(archive: &Archive, entry: &str, algorithm: HashAlgorithm) -> Result<HashResult> {
    let mut writer = HashWriter::new(algorithm);
    archive.copy_entry_to(entry, &mut writer)?;
    let bytes_hashed = writer.bytes;
    Ok(HashResult {
        entry: entry.to_owned(),
        algorithm,
        digest: writer.finish(),
        bytes_hashed,
    })
}

pub fn grep(archive: &Archive, pattern: &str, options: &GrepOptions) -> Result<GrepResult> {
    let matcher = options.glob.as_deref().map(build_matcher).transpose()?;
    let candidates = archive.entries()?.into_iter().filter(|entry| {
        entry.kind == EntryKind::File
            && matcher
                .as_ref()
                .is_none_or(|matcher| matcher.is_match(&entry.path))
    });
    let mut result = GrepResult {
        pattern: pattern.to_owned(),
        glob: options.glob.clone(),
        files_scanned: 0,
        binary_files_skipped: 0,
        oversized_files_skipped: 0,
        bytes_scanned: 0,
        matches_truncated: false,
        matches: Vec::new(),
    };
    for entry in candidates {
        if entry.size > options.max_entry_size {
            result.oversized_files_skipped += 1;
            continue;
        }
        let remaining = options
            .max_matches
            .saturating_sub(u64::try_from(result.matches.len()).unwrap_or(u64::MAX));
        if remaining == 0 {
            result.matches_truncated = true;
            break;
        }
        let mut scanner = GrepWriter::new(&entry.path, pattern, remaining, options.scan_binary);
        archive.copy_entry_to(&entry.path, &mut scanner)?;
        let scan = scanner.finish();
        result.files_scanned += 1;
        result.bytes_scanned = result.bytes_scanned.saturating_add(scan.bytes);
        if scan.binary_skipped {
            result.binary_files_skipped += 1;
        } else {
            result.matches.extend(scan.matches);
        }
        if scan.truncated {
            result.matches_truncated = true;
            break;
        }
    }
    Ok(result)
}

fn build_matcher(pattern: &str) -> Result<GlobMatcher> {
    Glob::new(pattern)
        .map(|glob| glob.compile_matcher())
        .map_err(|error| ArcthisError::UnsupportedOperation {
            message: format!("invalid glob `{pattern}`: {error}"),
        })
}

enum HashState {
    Sha256(Sha256),
    Sha512(Sha512),
}

struct HashWriter {
    state: HashState,
    bytes: u64,
}

impl HashWriter {
    fn new(algorithm: HashAlgorithm) -> Self {
        let state = match algorithm {
            HashAlgorithm::Sha256 => HashState::Sha256(Sha256::new()),
            HashAlgorithm::Sha512 => HashState::Sha512(Sha512::new()),
        };
        Self { state, bytes: 0 }
    }

    fn finish(self) -> String {
        match self.state {
            HashState::Sha256(state) => hex(&state.finalize()),
            HashState::Sha512(state) => hex(&state.finalize()),
        }
    }
}

impl Write for HashWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match &mut self.state {
            HashState::Sha256(state) => state.update(buffer),
            HashState::Sha512(state) => state.update(buffer),
        }
        self.bytes = self.bytes.saturating_add(buffer.len() as u64);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct GrepScan {
    bytes: u64,
    binary_skipped: bool,
    truncated: bool,
    matches: Vec<GrepMatch>,
}

#[allow(clippy::struct_excessive_bools)] // Independent streaming scan facts are clearer than an artificial state enum.
struct GrepWriter<'a> {
    path: &'a str,
    pattern: &'a [u8],
    scan_binary: bool,
    binary: bool,
    probe_complete: bool,
    pending: Vec<u8>,
    line: Vec<u8>,
    line_number: u64,
    line_truncated: bool,
    max_matches: u64,
    truncated: bool,
    matches: Vec<GrepMatch>,
    bytes: u64,
}

impl<'a> GrepWriter<'a> {
    fn new(path: &'a str, pattern: &'a str, max_matches: u64, scan_binary: bool) -> Self {
        Self {
            path,
            pattern: pattern.as_bytes(),
            scan_binary,
            binary: false,
            probe_complete: false,
            pending: Vec::new(),
            line: Vec::new(),
            line_number: 1,
            line_truncated: false,
            max_matches,
            truncated: false,
            matches: Vec::new(),
            bytes: 0,
        }
    }

    fn consume(&mut self, bytes: &[u8]) {
        if self.binary && !self.scan_binary {
            return;
        }
        for byte in bytes {
            if *byte == b'\n' {
                self.finish_line();
            } else if self.line.len() < MAX_LINE_BYTES {
                self.line.push(*byte);
            } else {
                self.line_truncated = true;
            }
        }
    }

    fn finish_line(&mut self) {
        if !self.truncated && contains_bytes(&self.line, self.pattern) {
            self.matches.push(GrepMatch {
                path: self.path.to_owned(),
                line_number: self.line_number,
                text: String::from_utf8_lossy(&self.line).into_owned(),
                line_truncated: self.line_truncated,
            });
            if u64::try_from(self.matches.len()).unwrap_or(u64::MAX) >= self.max_matches {
                self.truncated = true;
            }
        }
        self.line.clear();
        self.line_truncated = false;
        self.line_number = self.line_number.saturating_add(1);
    }

    fn finish(mut self) -> GrepScan {
        if !self.probe_complete {
            self.binary = self.pending.contains(&0);
            let pending = std::mem::take(&mut self.pending);
            self.consume(&pending);
        }
        if !self.line.is_empty() && (!self.binary || self.scan_binary) {
            self.finish_line();
        }
        GrepScan {
            bytes: self.bytes,
            binary_skipped: self.binary && !self.scan_binary,
            truncated: self.truncated,
            matches: self.matches,
        }
    }
}

impl Write for GrepWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buffer.len() as u64);
        if self.probe_complete {
            self.consume(buffer);
            return Ok(buffer.len());
        }
        self.pending.extend_from_slice(buffer);
        if self.pending.len() >= BINARY_PROBE_BYTES {
            self.binary = self.pending[..BINARY_PROBE_BYTES].contains(&0);
            self.probe_complete = true;
            let pending = std::mem::take(&mut self.pending);
            self.consume(&pending);
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[usize::from(byte >> 4)]));
        result.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    result
}
