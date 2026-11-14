use std::collections::BTreeSet;

use quiddity_core::{GraphId, Version};

use crate::Record;

/// One protocol message. Messages are framed by the transport, so each is
/// encoded without its own length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// The first message each side sends.
    Hello {
        /// The sender's graph schema version, if it has a graph yet.
        schema: Option<i64>,
        /// Which graph the sender holds, if it knows: a blank replica has
        /// yet to learn, and a graph from before graphs had IDs has none.
        /// Either is accepted by any peer, and two that differ are refused.
        graph: Option<GraphId>,
        /// The sending device's ID, as it appears in the graph's device
        /// list.
        device: String,
        /// Where the sender's history starts: it can only bring a peer up
        /// to date that has reached this.
        start: Version,
        version: Version,
    },
    /// Graph updates, as made by `Graph::updates_since`.
    Updates(Vec<u8>),
    /// A whole graph, for a blank peer that can't be brought up to date
    /// with updates because the sender's history has been compacted.
    Snapshot(Vec<u8>),
    /// A device's presence record, for a peer that has yet to be told of it
    /// (see [`crate::Presence`]).
    Presence(Record),
}

const HELLO: u8 = 1;
const UPDATES: u8 = 2;
const SNAPSHOT: u8 = 3;
const PRESENCE: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("empty message")]
    Empty,
    /// A message type this build doesn't know. Peers ignore these, so newer
    /// versions can add messages.
    #[error("unknown message type {0}")]
    UnknownType(u8),
    #[error("malformed message")]
    Malformed,
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Message::Hello {
                schema,
                graph,
                device,
                start,
                version,
            } => {
                let mut out = vec![HELLO];
                match schema {
                    Some(s) => {
                        out.push(1);
                        out.extend_from_slice(&s.to_le_bytes());
                    }
                    None => out.push(0),
                }
                match graph {
                    Some(id) => {
                        out.push(1);
                        out.extend_from_slice(&id.to_bytes());
                    }
                    None => out.push(0),
                }
                put(&mut out, device.as_bytes());
                put(&mut out, &start.encode());
                out.extend(version.encode());
                out
            }
            Message::Updates(bytes) => tagged(UPDATES, bytes),
            Message::Snapshot(bytes) => tagged(SNAPSHOT, bytes),
            Message::Presence(record) => {
                let mut out = vec![PRESENCE];
                out.extend_from_slice(&record.epoch.to_le_bytes());
                out.extend_from_slice(&record.seq.to_le_bytes());
                put(&mut out, record.device.as_bytes());
                put_set(&mut out, &record.peers);
                put_set(&mut out, &record.focus);
                out
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let (&tag, rest) = bytes.split_first().ok_or(DecodeError::Empty)?;
        match tag {
            HELLO => {
                let (schema, rest) = match rest.split_first() {
                    Some((0, rest)) => (None, rest),
                    Some((1, rest)) if rest.len() >= 8 => {
                        let (s, rest) = rest.split_at(8);
                        (
                            Some(i64::from_le_bytes(s.try_into().expect("eight bytes"))),
                            rest,
                        )
                    }
                    _ => return Err(DecodeError::Malformed),
                };
                let (graph, rest) = match rest.split_first() {
                    Some((0, rest)) => (None, rest),
                    Some((1, rest)) if rest.len() >= 16 => {
                        let (id, rest) = rest.split_at(16);
                        let id = id.try_into().expect("sixteen bytes");
                        (Some(GraphId::from_bytes(id)), rest)
                    }
                    _ => return Err(DecodeError::Malformed),
                };
                let (device, rest) = take(rest)?;
                let device =
                    String::from_utf8(device.to_vec()).map_err(|_| DecodeError::Malformed)?;
                let (start, rest) = take(rest)?;
                let start = Version::decode(start).map_err(|_| DecodeError::Malformed)?;
                let version = Version::decode(rest).map_err(|_| DecodeError::Malformed)?;
                Ok(Message::Hello {
                    schema,
                    graph,
                    device,
                    start,
                    version,
                })
            }
            UPDATES => Ok(Message::Updates(rest.to_vec())),
            SNAPSHOT => Ok(Message::Snapshot(rest.to_vec())),
            PRESENCE => {
                let (epoch, rest) = take_u64(rest)?;
                let (seq, rest) = take_u64(rest)?;
                let (device, rest) = take_string(rest)?;
                let (peers, rest) = take_set(rest)?;
                let (focus, rest) = take_set(rest)?;
                if !rest.is_empty() {
                    return Err(DecodeError::Malformed);
                }
                Ok(Message::Presence(Record {
                    device,
                    epoch,
                    seq,
                    peers,
                    focus,
                }))
            }
            other => Err(DecodeError::UnknownType(other)),
        }
    }
}

fn tagged(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.push(tag);
    out.extend_from_slice(bytes);
    out
}

/// Appends a field with a little-endian `u32` length.
fn put(out: &mut Vec<u8>, field: &[u8]) {
    let len = u32::try_from(field.len()).expect("fields are under 4 GiB");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(field);
}

/// Reads a field written by [`put`].
fn take(bytes: &[u8]) -> Result<(&[u8], &[u8]), DecodeError> {
    let (len, rest) = bytes
        .split_first_chunk::<4>()
        .ok_or(DecodeError::Malformed)?;
    let len = u32::from_le_bytes(*len) as usize;
    let field = rest.get(..len).ok_or(DecodeError::Malformed)?;
    Ok((field, &rest[len..]))
}

fn take_u64(bytes: &[u8]) -> Result<(u64, &[u8]), DecodeError> {
    let (n, rest) = bytes
        .split_first_chunk::<8>()
        .ok_or(DecodeError::Malformed)?;
    Ok((u64::from_le_bytes(*n), rest))
}

fn take_string(bytes: &[u8]) -> Result<(String, &[u8]), DecodeError> {
    let (field, rest) = take(bytes)?;
    let field = String::from_utf8(field.to_vec()).map_err(|_| DecodeError::Malformed)?;
    Ok((field, rest))
}

/// Appends a set of strings: a little-endian `u32` count, then each as by
/// [`put`].
fn put_set(out: &mut Vec<u8>, set: &BTreeSet<String>) {
    let len = u32::try_from(set.len()).expect("sets are under 4 billion");
    out.extend_from_slice(&len.to_le_bytes());
    for item in set {
        put(out, item.as_bytes());
    }
}

/// Reads a set written by [`put_set`].
fn take_set(bytes: &[u8]) -> Result<(BTreeSet<String>, &[u8]), DecodeError> {
    let (len, mut rest) = bytes
        .split_first_chunk::<4>()
        .ok_or(DecodeError::Malformed)?;
    let len = u32::from_le_bytes(*len) as usize;
    let mut set = BTreeSet::new();
    for _ in 0..len {
        let (item, after) = take_string(rest)?;
        set.insert(item);
        rest = after;
    }
    Ok((set, rest))
}
