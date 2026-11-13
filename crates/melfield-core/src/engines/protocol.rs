//! Worker wire protocol v1 (docs/protocol.md): 4-byte big-endian length +
//! UTF-8 JSON frame; a header with `"bin": n` is followed by one binary frame.

use std::io::{Read, Write};

use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME: usize = 256 * 1024 * 1024;

pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(payload.len()).map_err(|_| std::io::Error::other("frame too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

pub fn write_json(w: &mut impl Write, value: &Value) -> std::io::Result<()> {
    write_frame(w, value.to_string().as_bytes())
}

pub fn read_frame(r: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_be_bytes(len) as usize;
    if n > MAX_FRAME {
        return Err(std::io::Error::other(format!(
            "frame of {n} bytes exceeds limit"
        )));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// A decoded message: JSON header plus optional binary payload.
#[derive(Debug, Clone)]
pub struct Message {
    pub header: Value,
    pub binary: Option<Vec<u8>>,
}

pub fn read_message(r: &mut impl Read) -> std::io::Result<Message> {
    let header: Value = serde_json::from_slice(&read_frame(r)?).map_err(std::io::Error::other)?;
    let binary = match header.get("bin").and_then(Value::as_u64) {
        Some(_) => Some(read_frame(r)?),
        None => None,
    };
    Ok(Message { header, binary })
}

pub fn f32le_to_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_json_and_binary() {
        let mut buf = Vec::new();
        write_json(&mut buf, &json!({"id": 1, "event": "chunk", "bin": 8})).unwrap();
        let samples: Vec<u8> = [0.5f32, -0.25]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        write_frame(&mut buf, &samples).unwrap();
        let mut r = std::io::Cursor::new(buf);
        let m = read_message(&mut r).unwrap();
        assert_eq!(m.header["event"], "chunk");
        assert_eq!(f32le_to_vec(m.binary.as_deref().unwrap()), vec![0.5, -0.25]);
    }

    #[test]
    fn rejects_oversized_frames() {
        let mut r = std::io::Cursor::new(u32::MAX.to_be_bytes().to_vec());
        assert!(read_frame(&mut r).is_err());
    }
}
