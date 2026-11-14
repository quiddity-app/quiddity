//! Messages and frames survive encoding, and frames survive arriving in
//! arbitrary pieces. Decoding arbitrary bytes is fuzzed in `fuzz/`.

use proptest::collection::{btree_set, vec};
use proptest::prelude::*;
use quiddity_core::{Graph, GraphId, Version};
use quiddity_sync::{FrameError, FrameReader, MAX_FRAME, Message, Record, write_frame};

/// A real version: from a graph that has seen edits from several peers.
fn version() -> impl Strategy<Value = Version> {
    vec(0..4usize, 0..6).prop_map(|edits| {
        let graph = Graph::blank();
        for peer_edits in edits {
            let other = Graph::blank();
            for _ in 0..peer_edits {
                other.create_block(None, None, "x").unwrap();
            }
            graph
                .import(&other.updates_since(&graph.version()).unwrap())
                .unwrap();
        }
        graph.version()
    })
}

fn message() -> impl Strategy<Value = Message> {
    prop_oneof![
        (
            proptest::option::of(any::<i64>()),
            proptest::option::of(any::<[u8; 16]>().prop_map(GraphId::from_bytes)),
            any::<String>(),
            version(),
            version()
        )
            .prop_map(|(schema, graph, device, start, version)| Message::Hello {
                schema,
                graph,
                device,
                start,
                version,
            }),
        vec(any::<u8>(), 0..64).prop_map(Message::Updates),
        vec(any::<u8>(), 0..64).prop_map(Message::Snapshot),
        (
            any::<String>(),
            any::<u64>(),
            any::<u64>(),
            btree_set(any::<String>(), 0..4),
            btree_set(any::<String>(), 0..4)
        )
            .prop_map(
                |(device, epoch, seq, peers, focus)| Message::Presence(Record {
                    device,
                    epoch,
                    seq,
                    peers,
                    focus,
                })
            ),
    ]
}

proptest! {
    #[test]
    fn messages_round_trip(message in message()) {
        prop_assert_eq!(Message::decode(&message.encode()), Ok(message));
    }

    /// A message cut short is refused (or, for updates, is shorter
    /// updates), never misread or a panic.
    #[test]
    fn truncated_messages_are_not_misread(message in message()) {
        let bytes = message.encode();
        for len in 0..bytes.len() {
            prop_assert_ne!(Message::decode(&bytes[..len]), Ok(message.clone()));
        }
    }

    #[test]
    fn frames_survive_any_split(payloads in vec(vec(any::<u8>(), 0..40), 0..6), cuts in vec(any::<prop::sample::Index>(), 0..8)) {
        let mut stream = Vec::new();
        for p in &payloads {
            write_frame(&mut stream, p);
        }
        let mut cuts: Vec<usize> = cuts.iter().map(|c| c.index(stream.len() + 1)).collect();
        cuts.push(0);
        cuts.push(stream.len());
        cuts.sort();

        let mut reader = FrameReader::new();
        let mut got = Vec::new();
        for pair in cuts.windows(2) {
            reader.push(&stream[pair[0]..pair[1]]);
            while let Some(frame) = reader.next_frame().unwrap() {
                got.push(frame);
            }
        }
        prop_assert_eq!(got, payloads);
    }
}

#[test]
fn oversized_frames_are_refused_before_buffering() {
    let header = |len: usize| u32::try_from(len).unwrap().to_le_bytes();
    let mut reader = FrameReader::new();
    reader.push(&header(MAX_FRAME + 1));
    assert_eq!(
        reader.next_frame(),
        Err(FrameError::TooLarge(MAX_FRAME + 1))
    );

    // A whole graph's history goes in one frame on first sync, so the limit
    // is generous: a frame of exactly the limit waits for its bytes, and
    // the limit is well over a hundred megabytes.
    for len in [MAX_FRAME, 200 << 20] {
        let mut reader = FrameReader::new();
        reader.push(&header(len));
        assert_eq!(reader.next_frame(), Ok(None), "{len}");
    }
}

/// Presence is read exactly: a count the bytes can't hold, or bytes left
/// over, are refused rather than believed. (A huge count costs nothing: reading
/// stops at the first item that isn't there.)
#[test]
fn presence_is_read_exactly() {
    let record = Record {
        device: "d".into(),
        epoch: 1,
        seq: 2,
        peers: ["p".to_owned()].into(),
        focus: Default::default(),
    };
    let good = Message::Presence(record).encode();
    assert!(Message::decode(&good).is_ok());

    let mut extra = good.clone();
    extra.push(0);
    assert_eq!(
        Message::decode(&extra),
        Err(quiddity_sync::DecodeError::Malformed)
    );

    // The peers' count, claiming far more than follows.
    let count_at = 1 + 8 + 8 + 4 + 1;
    let mut greedy = good;
    greedy[count_at..count_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        Message::decode(&greedy),
        Err(quiddity_sync::DecodeError::Malformed)
    );
}
