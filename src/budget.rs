//! Allocation-free JSON byte accounting before result materialization.

use std::io::{self, Write};

use serde::Serialize;

use crate::error::{ArcthisError, Result};

pub(crate) fn json_bytes(value: &impl Serialize, limit: u64) -> Result<u64> {
    measure(value, limit, false)
}

#[cfg(feature = "mcp")]
pub(crate) fn mcp_payload_bytes(value: &impl Serialize, limit: u64) -> Result<u64> {
    measure(value, limit, true)
}

fn measure(value: &impl Serialize, limit: u64, mcp: bool) -> Result<u64> {
    let mut counter = JsonCounter {
        bytes: 0,
        limit,
        mcp,
        exceeded: false,
    };
    if let Err(error) = serde_json::to_writer(&mut counter, value) {
        return Err(if counter.exceeded {
            ArcthisError::ResourceLimit {
                message: format!("serialized result exceeds the {limit} byte budget"),
            }
        } else {
            ArcthisError::io("measuring JSON result", io::Error::other(error))
        });
    }
    Ok(counter.bytes)
}

struct JsonCounter {
    bytes: u64,
    limit: u64,
    mcp: bool,
    exceeded: bool,
}

impl Write for JsonCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let size = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        // MCP carries both structuredContent and a JSON string in content[].text.
        let size = if self.mcp {
            let escapes = buffer
                .iter()
                .filter(|byte| matches!(**byte, b'"' | b'\\'))
                .count();
            size.saturating_mul(2)
                .saturating_add(u64::try_from(escapes).unwrap_or(u64::MAX))
        } else {
            size
        };
        if size > self.limit.saturating_sub(self.bytes) {
            self.exceeded = true;
            return Err(io::Error::other("JSON byte budget exceeded"));
        }
        self.bytes += size;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
