//! Length-prefixed framing for messages on a byte stream.

/// The largest frame accepted. A whole graph's history goes in one message
/// on first sync, so this is generous, but it stops a misbehaving peer from
/// making us allocate without limit.
pub const MAX_FRAME: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("frame of {0} bytes is over the limit")]
    TooLarge(usize),
}

/// Appends a frame: a little-endian `u32` length, then the payload.
pub fn write_frame(out: &mut Vec<u8>, payload: &[u8]) {
    let len = u32::try_from(payload.len()).expect("frames are under 4 GiB");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
}

/// Reassembles frames from bytes arriving in arbitrary pieces.
#[derive(Debug, Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete frame, if one has arrived.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        let Some(header) = self.buf.get(..4) else {
            return Ok(None);
        };
        let len = u32::from_le_bytes(header.try_into().expect("four bytes")) as usize;
        if len > MAX_FRAME {
            return Err(FrameError::TooLarge(len));
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let frame = self.buf[4..4 + len].to_vec();
        self.buf.drain(..4 + len);
        Ok(Some(frame))
    }
}
