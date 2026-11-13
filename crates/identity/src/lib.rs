//! Quiddity's identifiers, each its own type so that one can't be passed
//! where another is wanted, and how each is written down.
//!
//! - [`DeviceKey`]: the one secret. A device's root, never used directly: what
//!   is used is derived from it ([`DeviceKey::endpoint_seed`],
//!   [`DeviceKey::node_mask`]).
//! - [`DeviceId`]: the public half of the key a device is known by on the
//!   network.
//! - [`GraphId`]: which graph this is. Random, kept in the graph.
//! - [`NodeId`]: which replica this is, as Loro's peer ID.
//!
//! The text forms have the shape of GitHub's own tokens: a prefix, the
//! payload in base 62 and a checksum. See `docs/Identity.md`.

use std::fmt;
use std::str::FromStr;

use zeroize::{Zeroize, Zeroizing};

mod token;

pub use token::Kind;

/// Why text wasn't read as an identifier. It never holds the text itself,
/// since the text may be a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("that is not a {}", .expected.name())]
    NotAToken { expected: Kind },
    #[error("that is a {}, and a {} was wanted", .found.name(), .expected.name())]
    WrongKind { expected: Kind, found: Kind },
    #[error("that is the wrong length for a {}", .kind.name())]
    BadLength { kind: Kind },
    #[error("that has characters in it that a {} doesn't", .kind.name())]
    Malformed { kind: Kind },
    #[error("that {} has a mistake in it: its checksum doesn't match", .kind.name())]
    BadChecksum { kind: Kind },
}

/// Everything but the secret is written the same way, and read back by
/// `FromStr`.
macro_rules! public_id {
    ($name:ident, $kind:expr) => {
        impl $name {
            /// How the ID is written down.
            pub const KIND: Kind = $kind;
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&token::encode($kind, &self.to_payload()))
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(text: &str) -> Result<Self, Error> {
                let payload = token::decode($kind, text)?;
                Ok(Self::from_payload(
                    payload.try_into().expect("decode gives the kind's length"),
                ))
            }
        }
    };
}

/// A graph's identity: 128 random bits, made when the graph is, and kept in
/// its document so that every replica has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphId([u8; 16]);

impl GraphId {
    /// The ID with these bytes, which must be random: the caller says where
    /// from, so this crate needs no source of randomness, and builds for
    /// WASM.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn to_bytes(self) -> [u8; 16] {
        self.0
    }

    fn to_payload(self) -> [u8; 16] {
        self.0
    }

    fn from_payload(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

public_id!(GraphId, Kind::GraphId);

/// A replica's identity: Loro's peer ID. Two replicas must never edit under
/// one, so it is never reused by a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    fn to_payload(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    fn from_payload(bytes: [u8; 8]) -> Self {
        Self(u64::from_be_bytes(bytes))
    }

    /// This ID in the form to keep it in, readable only with the mask it was
    /// sealed with.
    pub fn seal(self, mask: &NodeMask) -> SealedNodeId {
        let mut bytes = [0; 12];
        bytes[..8].copy_from_slice(&(self.0 ^ mask.0).to_be_bytes());
        bytes[8..].copy_from_slice(&token::checksum(&self.to_payload()));
        SealedNodeId(bytes)
    }
}

public_id!(NodeId, Kind::NodeId);

/// A device's public ID: the public half of the key it is known by on the
/// network. It reveals nothing else about the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId([u8; 32]);

impl DeviceId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    fn to_payload(self) -> [u8; 32] {
        self.0
    }

    fn from_payload(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The bare form devices have always been listed under in a graph:
    /// 64 lower-case hexadecimal digits.
    pub fn to_bare(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Reads the bare form, or the written form.
    pub fn parse(text: &str) -> Result<Self, Error> {
        if text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit()) {
            let mut bytes = [0; 32];
            for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks(2)) {
                // ASCII hex digits, so each pair is valid UTF-8 and a number.
                *byte = u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII"), 16)
                    .expect("hex digits");
            }
            return Ok(Self(bytes));
        }
        text.parse()
    }
}

public_id!(DeviceId, Kind::DeviceId);

/// A device's secret, from which everything it keeps secret is derived.
///
/// It is a seed and not a signing key: nothing signs or encrypts with it, so
/// it can't be used for the wrong thing. It has no `Display`, `Serialize` or
/// `Clone`, its `Debug` says nothing, and it is wiped when dropped (by [`Zeroizing`]). The one
/// way out is [`DeviceKey::to_token`], for the file that keeps it.
pub struct DeviceKey(Zeroizing<[u8; 32]>);

/// What derivation is tied to: BLAKE3's context strings, which must be unique
/// to the application and to the use.
const ENDPOINT_CONTEXT: &str = "quiddity 2026-11 identity v1 endpoint key";
const NODE_MASK_CONTEXT: &str = "quiddity 2026-11 identity v1 node mask";

impl DeviceKey {
    /// The key with these bytes, which must be random.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The key as it is written to the file that keeps it. The text is
    /// wiped when dropped.
    pub fn to_token(&self) -> Zeroizing<String> {
        Zeroizing::new(token::encode(Kind::DeviceKey, &*self.0))
    }

    /// The key made from the text [`DeviceKey::to_token`] gave. A failure
    /// says what is wrong without repeating the text.
    pub fn from_token(text: &str) -> Result<Self, Error> {
        let mut payload = Zeroizing::new(token::decode(Kind::DeviceKey, text)?);
        let mut bytes = [0; 32];
        bytes.copy_from_slice(&payload);
        payload.zeroize();
        Ok(Self::from_bytes(bytes))
    }

    /// The seed of the key a device is known by on the network, for one
    /// `salt`: the same key and salt give the same seed, and no two salts
    /// give related ones. Whoever has the seed is the device, so it is
    /// wiped when dropped.
    pub fn endpoint_seed(&self, salt: &Salt) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.derive(ENDPOINT_CONTEXT, salt))
    }

    /// The mask that ties a [`NodeId`] to this device and salt.
    pub fn node_mask(&self, salt: &Salt) -> NodeMask {
        let derived = Zeroizing::new(self.derive(NODE_MASK_CONTEXT, salt));
        let mut mask = [0; 8];
        mask.copy_from_slice(&derived[..8]);
        NodeMask(u64::from_be_bytes(mask))
    }

    fn derive(&self, context: &str, salt: &Salt) -> [u8; 32] {
        // Fixed sizes, so the concatenation is unambiguous.
        let mut material = Zeroizing::new([0u8; 48]);
        material[..32].copy_from_slice(&*self.0);
        material[32..].copy_from_slice(&salt.0);
        blake3::derive_key(context, &*material)
    }
}

impl fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceKey(..)")
    }
}

/// What a derived key is specific to, so that one device key gives each place
/// it is used its own. Not secret, and kept beside what it is for. It is
/// random for each graph file rather than the graph's ID, since a blank
/// replica must have a device ID, to be let in, before it knows the graph's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Salt([u8; 16]);

impl Salt {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn to_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// 64 bits derived from a [`DeviceKey`], to tie a [`NodeId`] to a device. It
/// is a binding and not a secret in itself: it says only whether a stored ID
/// was made here.
#[derive(Clone, PartialEq, Eq)]
pub struct NodeMask(u64);

impl fmt::Debug for NodeMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NodeMask(..)")
    }
}

/// A [`NodeId`] as it is kept: 12 bytes, the ID XORed with a [`NodeMask`] and
/// then a check of the ID. Opened with another device's mask, the check
/// fails, which is how a graph copied from another device is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SealedNodeId([u8; 12]);

impl SealedNodeId {
    pub const fn from_bytes(bytes: [u8; 12]) -> Self {
        Self(bytes)
    }

    pub const fn to_bytes(self) -> [u8; 12] {
        self.0
    }

    /// The ID, if `mask` is the one it was sealed with. A wrong mask is
    /// told apart with all but a 1 in 2^32 chance.
    pub fn open(&self, mask: &NodeMask) -> Option<NodeId> {
        let (masked, check) = self.0.split_at(8);
        let id = NodeId(u64::from_be_bytes(masked.try_into().ok()?) ^ mask.0);
        (token::checksum(&id.to_payload()) == check).then_some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn key(byte: u8) -> DeviceKey {
        DeviceKey::from_bytes([byte; 32])
    }

    fn salt(byte: u8) -> Salt {
        Salt::from_bytes([byte; 16])
    }

    #[test]
    fn each_identifier_has_its_own_prefix() {
        assert!(key(1).to_token().starts_with("qdk_"));
        assert!(
            DeviceId::from_bytes([1; 32])
                .to_string()
                .starts_with("qdi_")
        );
        assert!(GraphId::from_bytes([1; 16]).to_string().starts_with("qgi_"));
        assert!(NodeId::new(1).to_string().starts_with("qni_"));
    }

    #[test]
    fn a_key_is_read_back_from_its_token() {
        let token = key(9).to_token();
        assert_eq!(token.len(), 4 + 43 + 6);
        let again = DeviceKey::from_token(&token).unwrap();
        assert_eq!(again.to_token(), token);
    }

    #[test]
    fn a_secret_says_nothing_about_itself() {
        let token = key(9).to_token();
        let body = &token[4..];
        assert_eq!(format!("{:?}", key(9)), "DeviceKey(..)");
        assert_eq!(format!("{:?}", key(9).node_mask(&salt(1))), "NodeMask(..)");
        // Nor does failing to read one.
        let mut wrong = token.to_string();
        wrong.replace_range(10..11, if &wrong[10..11] == "0" { "1" } else { "0" });
        let error = DeviceKey::from_token(&wrong).unwrap_err().to_string();
        assert!(
            !error.contains(body) && !error.contains(&wrong[4..]),
            "{error}"
        );
        assert!(!error.contains("qdk_"), "{error}");
    }

    #[test]
    fn one_kind_is_never_read_as_another() {
        let id = DeviceId::from_bytes([3; 32]).to_string();
        assert_eq!(
            DeviceKey::from_token(&id).err(),
            Some(Error::WrongKind {
                expected: Kind::DeviceKey,
                found: Kind::DeviceId
            })
        );
        let graph = GraphId::from_bytes([3; 16]).to_string();
        assert!(graph.parse::<NodeId>().is_err());
        assert!(graph.parse::<DeviceId>().is_err());
        assert_eq!(
            graph.parse::<GraphId>().unwrap(),
            GraphId::from_bytes([3; 16])
        );
        // The secret can be named in the message, though never echoed.
        let message = DeviceKey::from_token(&id).unwrap_err().to_string();
        assert_eq!(message, "that is a device ID, and a device key was wanted");
    }

    #[test]
    fn a_device_id_reads_bare_or_written() {
        let id = DeviceId::from_bytes(std::array::from_fn(|i| i as u8 * 7));
        let bare = id.to_bare();
        assert_eq!(bare.len(), 64);
        assert!(bare.starts_with("00070e151c"));
        assert_eq!(DeviceId::parse(&bare).unwrap(), id);
        assert_eq!(DeviceId::parse(&bare.to_uppercase()).unwrap(), id);
        assert_eq!(DeviceId::parse(&id.to_string()).unwrap(), id);
        assert!(DeviceId::parse(&bare[1..]).is_err());
        assert!(DeviceId::parse(&format!("{}g", &bare[1..])).is_err());
        assert!(DeviceId::parse("").is_err());
    }

    #[test]
    fn derivation_depends_on_the_key_the_salt_and_the_use() {
        let seed = |k: u8, s: u8| *key(k).endpoint_seed(&salt(s));
        assert_eq!(seed(1, 1), seed(1, 1));
        assert_ne!(seed(1, 1), seed(2, 1), "another key");
        assert_ne!(seed(1, 1), seed(1, 2), "another salt");
        // Neither is the key itself, nor the other use of it.
        assert_ne!(seed(1, 1), [1; 32]);
        let mask = key(1).node_mask(&salt(1));
        assert_ne!(mask, key(2).node_mask(&salt(1)));
        assert_ne!(mask, key(1).node_mask(&salt(2)));
        assert_ne!(
            mask.0.to_be_bytes(),
            seed(1, 1)[..8],
            "uses of one key are separated by context"
        );
    }

    #[test]
    fn the_salt_and_key_are_not_interchangeable_in_the_input() {
        // Key bytes and salt bytes make one 48-byte input: moving a byte
        // across the boundary must not give the same output.
        let mut one = [0u8; 32];
        one[31] = 1;
        let a = DeviceKey::from_bytes(one).endpoint_seed(&Salt::from_bytes([0; 16]));
        let b = DeviceKey::from_bytes([0; 32]).endpoint_seed(&{
            let mut s = [0; 16];
            s[0] = 1;
            Salt::from_bytes(s)
        });
        assert_ne!(*a, *b);
    }

    #[test]
    fn a_sealed_node_id_opens_with_its_own_mask_only() {
        let mine = key(1).node_mask(&salt(1));
        let sealed = NodeId::new(0x0123_4567_89AB_CDEF).seal(&mine);
        assert_eq!(sealed.open(&mine), Some(NodeId::new(0x0123_4567_89AB_CDEF)));
        assert_eq!(sealed.open(&key(2).node_mask(&salt(1))), None);
        assert_eq!(sealed.open(&key(1).node_mask(&salt(2))), None);
        // The plain ID isn't in what is kept.
        assert!(
            !sealed
                .to_bytes()
                .windows(8)
                .any(|w| w == 0x0123_4567_89AB_CDEFu64.to_be_bytes())
        );
    }

    #[test]
    fn a_damaged_sealed_node_id_does_not_open() {
        let mask = key(1).node_mask(&salt(1));
        let sealed = NodeId::new(42).seal(&mask).to_bytes();
        for at in 0..12 {
            for bit in 0..8 {
                let mut damaged = sealed;
                damaged[at] ^= 1 << bit;
                assert_eq!(SealedNodeId::from_bytes(damaged).open(&mask), None);
            }
        }
    }

    proptest! {
        /// The bytes an ID is made from are the bytes it gives back.
        #[test]
        fn bytes_come_back_as_they_went_in(
            graph in any::<[u8; 16]>(),
            node in any::<u64>(),
            device in any::<[u8; 32]>(),
            salt in any::<[u8; 16]>(),
            sealed in any::<[u8; 12]>(),
        ) {
            prop_assert_eq!(GraphId::from_bytes(graph).to_bytes(), graph);
            prop_assert_eq!(NodeId::new(node).get(), node);
            prop_assert_eq!(DeviceId::from_bytes(device).to_bytes(), device);
            prop_assert_eq!(Salt::from_bytes(salt).to_bytes(), salt);
            prop_assert_eq!(SealedNodeId::from_bytes(sealed).to_bytes(), sealed);
        }

        #[test]
        fn every_public_id_reads_back(
            graph in any::<[u8; 16]>(),
            node in any::<u64>(),
            device in any::<[u8; 32]>(),
        ) {
            let graph = GraphId::from_bytes(graph);
            prop_assert_eq!(graph.to_string().parse::<GraphId>().unwrap(), graph);
            let node = NodeId::new(node);
            prop_assert_eq!(node.to_string().parse::<NodeId>().unwrap(), node);
            let device = DeviceId::from_bytes(device);
            prop_assert_eq!(DeviceId::parse(&device.to_string()).unwrap(), device);
            prop_assert_eq!(DeviceId::parse(&device.to_bare()).unwrap(), device);
        }

        #[test]
        fn a_key_reads_back(bytes in any::<[u8; 32]>()) {
            let token = DeviceKey::from_bytes(bytes).to_token();
            let again = DeviceKey::from_token(&token).unwrap();
            prop_assert_eq!(again.to_token(), token);
        }

        /// Sealing is reversible, and with any other mask almost never is.
        #[test]
        fn sealing_opens_with_the_mask_it_was_sealed_with(
            id in any::<u64>(),
            a in any::<[u8; 32]>(),
            b in any::<[u8; 32]>(),
            s in any::<[u8; 16]>(),
        ) {
            let salt = Salt::from_bytes(s);
            let mine = DeviceKey::from_bytes(a).node_mask(&salt);
            let sealed = NodeId::new(id).seal(&mine);
            prop_assert_eq!(sealed.open(&mine), Some(NodeId::new(id)));
            if a != b {
                prop_assert_eq!(sealed.open(&DeviceKey::from_bytes(b).node_mask(&salt)), None);
            }
        }
    }
}
