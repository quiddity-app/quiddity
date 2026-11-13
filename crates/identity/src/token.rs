//! How an identifier is written down: a prefix that says what it is, the
//! payload in base 62, and a checksum, in the shape of GitHub's own tokens
//! (`ghp_…`) so that a secret committed by mistake can be found by pattern,
//! and a typo is caught before the value is used.
//!
//! `qdk_` + 43 characters + 6 characters, for a 32-byte payload. The
//! checksum is the CRC-32 of the payload bytes, written in base 62 at a fixed
//! width. Every width is fixed, so a token has exactly one spelling.

use crate::Error;

const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The characters in the checksum: 62^6 is more than 2^32.
const CHECK_DIGITS: usize = 6;

/// The kinds of identifier, with the prefix each is written under. One
/// secret and three public: only the secret's prefix is meant to be matched
/// by a scanner, so a public ID is never mistaken for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A device key: secret.
    DeviceKey,
    /// A device's public ID.
    DeviceId,
    GraphId,
    NodeId,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::DeviceKey, Kind::DeviceId, Kind::GraphId, Kind::NodeId];

    /// The prefix, without its underscore.
    pub const fn prefix(self) -> &'static str {
        match self {
            Kind::DeviceKey => "qdk",
            Kind::DeviceId => "qdi",
            Kind::GraphId => "qgi",
            Kind::NodeId => "qni",
        }
    }

    /// What the kind is called in an error message.
    pub const fn name(self) -> &'static str {
        match self {
            Kind::DeviceKey => "device key",
            Kind::DeviceId => "device ID",
            Kind::GraphId => "graph ID",
            Kind::NodeId => "node ID",
        }
    }

    /// How many bytes the payload has.
    pub const fn payload_len(self) -> usize {
        match self {
            Kind::DeviceKey | Kind::DeviceId => 32,
            Kind::GraphId => 16,
            Kind::NodeId => 8,
        }
    }

    /// How many characters the payload takes: the fewest whose range covers
    /// every value.
    const fn digits(self) -> usize {
        match self {
            Kind::DeviceKey | Kind::DeviceId => 43,
            Kind::GraphId => 22,
            Kind::NodeId => 11,
        }
    }

    /// The kind whose prefix `text` begins with.
    pub fn of(text: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| {
            text.strip_prefix(kind.prefix())
                .is_some_and(|rest| rest.starts_with('_'))
        })
    }
}

/// CRC-32 (IEEE, as in zlib and GitHub's tokens) of `bytes`.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// The checksum of a payload, as the bytes it is written from.
pub(crate) fn checksum(payload: &[u8]) -> [u8; 4] {
    crc32(payload).to_be_bytes()
}

/// Writes `bytes` as a big-endian number in base 62, in exactly `digits`
/// characters.
fn to_base62(bytes: &[u8], digits: usize) -> String {
    let mut number = bytes.to_vec();
    let mut out = vec![ALPHABET[0]; digits];
    for slot in out.iter_mut().rev() {
        // One step of long division by 62.
        let mut remainder = 0u32;
        for byte in &mut number {
            let current = remainder * 256 + u32::from(*byte);
            *byte = (current / 62) as u8;
            remainder = current % 62;
        }
        *slot = ALPHABET[remainder as usize];
    }
    debug_assert!(number.iter().all(|&b| b == 0), "too few digits");
    String::from_utf8(out).expect("the alphabet is ASCII")
}

/// Reads `text` as a number in base 62 into `len` bytes. `None` if it has a
/// character outside the alphabet, or a value too large for `len` bytes.
fn from_base62(text: &str, len: usize) -> Option<Vec<u8>> {
    let mut number = vec![0u8; len];
    for c in text.bytes() {
        let digit = ALPHABET.iter().position(|&a| a == c)? as u32;
        // number = number * 62 + digit
        let mut carry = digit;
        for byte in number.iter_mut().rev() {
            let current = u32::from(*byte) * 62 + carry;
            *byte = (current & 0xFF) as u8;
            carry = current >> 8;
        }
        if carry != 0 {
            return None;
        }
    }
    Some(number)
}

/// Writes a payload as a token of `kind`.
pub(crate) fn encode(kind: Kind, payload: &[u8]) -> String {
    assert_eq!(
        payload.len(),
        kind.payload_len(),
        "the payload is the wrong size"
    );
    format!(
        "{}_{}{}",
        kind.prefix(),
        to_base62(payload, kind.digits()),
        to_base62(&checksum(payload), CHECK_DIGITS)
    )
}

/// Reads a token that must be of `kind`.
///
/// The text is checked in the order a person would want told: whether it is
/// one of ours at all, whether it is the right kind, and only then whether it
/// is intact.
pub(crate) fn decode(kind: Kind, text: &str) -> Result<Vec<u8>, Error> {
    let Some(found) = Kind::of(text) else {
        return Err(Error::NotAToken { expected: kind });
    };
    if found != kind {
        return Err(Error::WrongKind {
            expected: kind,
            found,
        });
    }
    let body = &text[kind.prefix().len() + 1..];
    if body.len() != kind.digits() + CHECK_DIGITS || !body.is_ascii() {
        return Err(Error::BadLength { kind });
    }
    let (payload, check) = body.split_at(kind.digits());
    let payload = from_base62(payload, kind.payload_len()).ok_or(Error::Malformed { kind })?;
    let check = from_base62(check, 4).ok_or(Error::Malformed { kind })?;
    if check != checksum(&payload) {
        return Err(Error::BadChecksum { kind });
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn crc32_is_the_standard_one() {
        // The check value in the CRC catalogue for CRC-32/ISO-HDLC.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn the_digit_counts_cover_every_payload_and_no_more() {
        for kind in Kind::ALL {
            let bits = kind.payload_len() as u32 * 8;
            // 62^digits >= 2^bits > 62^(digits - 1), by logarithm to be exact
            // enough for sizes this small.
            let log62 = f64::from(bits) / 62f64.log2();
            assert_eq!(kind.digits(), log62.ceil() as usize, "{kind:?}");
            assert_eq!(
                to_base62(&vec![0xFF; kind.payload_len()], kind.digits()).len(),
                kind.digits()
            );
        }
    }

    #[test]
    fn extremes_round_trip() {
        for kind in Kind::ALL {
            for byte in [0x00, 0x01, 0x7F, 0xFF] {
                let payload = vec![byte; kind.payload_len()];
                let text = encode(kind, &payload);
                assert_eq!(decode(kind, &text).unwrap(), payload, "{text}");
            }
        }
    }

    /// Written down once, from an independent implementation (Python's
    /// `zlib.crc32` and long division), so that a change to the text form of
    /// anything already stored is a failure here and not a surprise later.
    #[test]
    fn the_text_form_is_pinned() {
        assert_eq!(encode(Kind::NodeId, &[0; 8]), "qni_000000000001qpX5d");
        assert_eq!(
            encode(Kind::DeviceKey, &[0xFF; 32]),
            "qdk_yhjskwdA6OZ1AL1YmHWZWm8LLG7HjnuCA2j5rOw8Xp14g0jLX"
        );
    }

    /// Every single character of a token changed, to every other character:
    /// the checksum notices, or the value is out of range. A CRC could let a
    /// few through, 2^-32 of them; none of these are such.
    #[test]
    fn no_single_changed_character_reads() {
        for kind in Kind::ALL {
            let text = encode(kind, &vec![0xA5; kind.payload_len()]);
            let start = kind.prefix().len() + 1;
            for at in start..text.len() {
                for &c in ALPHABET {
                    let mut changed = text.clone().into_bytes();
                    if changed[at] == c {
                        continue;
                    }
                    changed[at] = c;
                    let changed = String::from_utf8(changed).unwrap();
                    assert!(decode(kind, &changed).is_err(), "{changed}");
                }
            }
        }
    }

    #[test]
    fn swapping_neighbours_is_noticed() {
        for kind in Kind::ALL {
            let text = encode(kind, &(0..kind.payload_len() as u8).collect::<Vec<_>>());
            let start = kind.prefix().len() + 1;
            for at in start..text.len() - 1 {
                let mut swapped = text.clone().into_bytes();
                if swapped[at] == swapped[at + 1] {
                    continue;
                }
                swapped.swap(at, at + 1);
                let swapped = String::from_utf8(swapped).unwrap();
                assert!(decode(kind, &swapped).is_err(), "{swapped}");
            }
        }
    }

    #[test]
    fn the_wrong_kind_is_named() {
        let text = encode(Kind::GraphId, &[7; 16]);
        assert_eq!(
            decode(Kind::DeviceKey, &text),
            Err(Error::WrongKind {
                expected: Kind::DeviceKey,
                found: Kind::GraphId
            })
        );
    }

    #[test]
    fn other_text_is_not_a_token() {
        for text in [
            "", "qdk", "qdk-abc", "qdkabc", "ghp_abc", "Qdk_abc", " qdk_abc",
        ] {
            assert_eq!(
                decode(Kind::DeviceKey, text),
                Err(Error::NotAToken {
                    expected: Kind::DeviceKey
                }),
                "{text:?}"
            );
        }
    }

    #[test]
    fn the_wrong_length_is_refused_before_it_is_read() {
        let text = encode(Kind::NodeId, &[1; 8]);
        for cut in 0..text.len() - 4 {
            assert!(decode(Kind::NodeId, &text[..text.len() - cut - 1]).is_err());
        }
        assert_eq!(
            decode(Kind::NodeId, &format!("{text}0")),
            Err(Error::BadLength { kind: Kind::NodeId })
        );
    }

    #[test]
    fn a_value_too_big_for_the_payload_is_refused() {
        // 11 digits of 'z' is 62^11 - 1, which is more than 2^64.
        assert_eq!(
            decode(Kind::NodeId, "qni_zzzzzzzzzzz000000"),
            Err(Error::Malformed { kind: Kind::NodeId })
        );
    }

    #[test]
    fn multibyte_text_is_refused_not_split() {
        // The right number of bytes, with a character that is not one.
        let text = format!("qni_{}0", "é".repeat(8));
        assert!(decode(Kind::NodeId, &text).is_err());
    }

    proptest! {
        #[test]
        fn what_is_written_reads_back(payload in proptest::collection::vec(any::<u8>(), 32)) {
            for kind in Kind::ALL {
                let payload = &payload[..kind.payload_len()];
                let text = encode(kind, payload);
                prop_assert_eq!(Kind::of(&text), Some(kind));
                prop_assert_eq!(decode(kind, &text).unwrap(), payload);
            }
        }

        /// Whatever a person might paste, it either reads as what was
        /// written or is refused: it never panics, and a successful read
        /// writes back as the same text.
        #[test]
        fn arbitrary_text_never_panics_and_reads_only_what_it_spells(text in "\\PC{0,80}") {
            for kind in Kind::ALL {
                if let Ok(payload) = decode(kind, &text) {
                    prop_assert_eq!(encode(kind, &payload), text.clone());
                }
            }
        }

        /// Text shaped like a token, with the right prefix and length and any
        /// characters of the alphabet, is nearly always refused: only the
        /// checksum can let it through.
        #[test]
        fn token_shaped_text_reads_only_if_it_spells_one(
            body in "[0-9A-Za-z]{49}",
        ) {
            let text = format!("qdk_{body}");
            if let Ok(payload) = decode(Kind::DeviceKey, &text) {
                prop_assert_eq!(encode(Kind::DeviceKey, &payload), text);
            }
        }
    }
}
