//! The devices holding replicas of a graph, and what each has stored.
//!
//! This is sync metadata rather than content: it lives in its own map in the
//! graph's document, not in any block, so it is never exported. Each device
//! writes only its own entry, so entries never conflict.

use jiff::{SignedDuration, Timestamp};
use loro::{Container, LoroValue, ValueOrContainer};

use crate::{Graph, Result, Version};

const DEVICES: &str = "devices";
const STORED: &str = "stored";
const SEEN: &str = "seen";

/// A device as it last recorded itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    /// The version the device had durably stored.
    pub stored: Version,
    /// When it recorded that.
    pub seen: Timestamp,
}

impl Graph {
    /// Records that the device `id` has stored this replica as it is now.
    /// The record is an edit, and syncs to other devices like any edit.
    pub fn record_stored(&self, id: &str, now: Timestamp) -> Result<()> {
        self.record_stored_at(id, &self.version(), now)
    }

    /// Records that the device `id` has stored `stored`, which the caller
    /// knows is already durable: other devices act on the record the moment
    /// it reaches them, by dropping history up to it, so it must never run
    /// ahead of what the device has. That is why a save records the version
    /// the file held before it, not the one it is about to write.
    pub fn record_stored_at(&self, id: &str, stored: &Version, now: Timestamp) -> Result<()> {
        let entry = self.doc().get_map(DEVICES).ensure_mergeable_map(id)?;
        entry.insert(STORED, stored.encode())?;
        entry.insert(SEEN, now.as_millisecond())?;
        self.commit();
        Ok(())
    }

    /// Records the device `id`, which this replica is about to bring up to
    /// date from `stored`, unless it is already known. Until a newcomer
    /// records itself, this is what stops devices compacting past what it
    /// has: every record this replica made before is included in what the
    /// newcomer receives, and every later one arrives with this one.
    pub fn vouch_for(&self, id: &str, stored: &Version, now: Timestamp) -> Result<()> {
        let devices = self.doc().get_map(DEVICES);
        if devices.get(id).is_some() {
            return Ok(());
        }
        let entry = devices.ensure_mergeable_map(id)?;
        entry.insert(STORED, stored.encode())?;
        entry.insert(SEEN, now.as_millisecond())?;
        self.commit();
        Ok(())
    }

    /// Every device that has recorded itself, in no particular order.
    /// Entries this build can't read are skipped.
    pub fn devices(&self) -> Vec<Device> {
        let mut devices = Vec::new();
        self.doc().get_map(DEVICES).for_each(|id, entry| {
            let ValueOrContainer::Container(Container::Map(entry)) = entry else {
                return;
            };
            let stored = match entry.get(STORED) {
                Some(ValueOrContainer::Value(LoroValue::Binary(b))) => Version::decode(&b).ok(),
                _ => None,
            };
            let seen = match entry.get(SEEN) {
                Some(ValueOrContainer::Value(LoroValue::I64(ms))) => {
                    Timestamp::from_millisecond(ms).ok()
                }
                _ => None,
            };
            if let (Some(stored), Some(seen)) = (stored, seen) {
                devices.push(Device {
                    id: id.to_owned(),
                    stored,
                    seen,
                });
            }
        });
        devices
    }

    /// The version this replica can safely compact at (see
    /// [`Graph::shallow_snapshot`]): what every device seen within `active`
    /// of `now` has stored. It's only safe once this replica has everything
    /// any of them has stored, since an edit made before its device recorded
    /// itself may have been made on older history; after that, a device's
    /// edits are made on what it recorded. So until then, there is none.
    ///
    /// A device silent for longer than `active` stops holding compaction
    /// back. If it returns with edits made on history others have dropped,
    /// they can't be applied.
    pub fn compaction_point(&self, now: Timestamp, active: SignedDuration) -> Option<Version> {
        self.compaction_point_of(self.devices(), now, active)
    }

    /// [`Graph::compaction_point`] as it will be once the device `id` has
    /// stored `stored`, which it is about to: a save compacts at what the file
    /// will then hold, though it may record only what it held before.
    pub fn compaction_point_as(
        &self,
        now: Timestamp,
        active: SignedDuration,
        id: &str,
        stored: &Version,
    ) -> Option<Version> {
        let mut devices = self.devices();
        devices.retain(|d| d.id != id);
        devices.push(Device {
            id: id.to_owned(),
            stored: stored.clone(),
            seen: now,
        });
        self.compaction_point_of(devices, now, active)
    }

    fn compaction_point_of(
        &self,
        devices: Vec<Device>,
        now: Timestamp,
        active: SignedDuration,
    ) -> Option<Version> {
        let stored: Vec<Version> = devices
            .into_iter()
            .filter(|d| now.duration_since(d.seen) <= active)
            .map(|d| d.stored)
            .collect();
        let everything = stored.iter().fold(Version::default(), |mut all, v| {
            all.merge(v);
            all
        });
        if !self.version().includes(&everything) {
            return None;
        }
        stored.into_iter().reduce(|a, b| a.meet(&b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    const MONTH: SignedDuration = SignedDuration::from_hours(24 * 30);

    #[test]
    fn records_read_back_and_vouching_defers_to_them() -> Result<()> {
        let graph = Graph::new();
        graph.vouch_for("new", &Version::default(), at("2026-10-01T00:00Z"))?;
        let vouched = graph.version();
        graph.record_stored("me", at("2026-10-02T00:00Z"))?;
        graph.vouch_for("me", &Version::default(), at("2026-10-03T00:00Z"))?;

        let mut devices = graph.devices();
        devices.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(
            devices,
            [
                Device {
                    id: "me".into(),
                    stored: vouched,
                    seen: at("2026-10-02T00:00Z"),
                },
                Device {
                    id: "new".into(),
                    stored: Version::default(),
                    seen: at("2026-10-01T00:00Z"),
                },
            ]
        );
        Ok(())
    }

    /// A device vouched for at the version it says it has, before its edits
    /// have arrived, holds compaction back until they do.
    #[test]
    fn compaction_waits_for_everything_stored() -> Result<()> {
        let now = at("2026-10-26T00:00Z");
        let (a, b) = (Graph::new(), Graph::blank());
        a.record_stored("a", now)?;
        b.import(&a.snapshot()?)?;
        b.create_block(None, None, "only on b")?;
        b.commit();

        a.vouch_for("b", &b.version(), now)?;
        assert_eq!(a.compaction_point(now, MONTH), None);
        a.import(&b.updates_since(&a.version())?)?;
        assert!(a.compaction_point(now, MONTH).is_some());
        Ok(())
    }

    #[test]
    fn silent_devices_stop_holding_compaction_back() -> Result<()> {
        let graph = Graph::new();
        graph.vouch_for("silent", &Version::default(), at("2026-01-01T00:00Z"))?;
        graph.record_stored("me", at("2026-10-26T00:00Z"))?;
        let point = |now: &str| graph.compaction_point(at(now), MONTH);
        assert_eq!(point("2026-01-15T00:00Z"), Some(Version::default()));
        let stored = graph
            .devices()
            .into_iter()
            .find(|d| d.id == "me")
            .unwrap()
            .stored;
        assert_eq!(point("2026-10-27T00:00Z"), Some(stored));
        Ok(())
    }

    /// A record says the version it is given, not the one the replica has
    /// reached since.
    #[test]
    fn a_record_says_what_it_is_given() -> Result<()> {
        let graph = Graph::new();
        let stored = graph.version();
        graph.create_block(None, None, "later")?;
        graph.commit();
        graph.record_stored_at("me", &stored, at("2026-10-26T00:00Z"))?;
        assert_eq!(graph.devices()[0].stored, stored);
        Ok(())
    }

    /// Compacting as if a device had stored a version takes that in place of
    /// what it recorded, and leaves the others as they are.
    #[test]
    fn compaction_as_if_a_device_had_stored() -> Result<()> {
        let now = at("2026-10-26T00:00Z");
        let graph = Graph::new();
        let old = graph.version();
        graph.vouch_for("other", &old, now)?;
        graph.record_stored_at("me", &old, now)?;
        graph.create_block(None, None, "new")?;
        graph.commit();
        let new = graph.version();

        assert_eq!(graph.compaction_point(now, MONTH), Some(old.clone()));
        assert_eq!(
            graph.compaction_point_as(now, MONTH, "me", &new),
            Some(old.clone()),
            "the other device still holds it back"
        );
        graph.record_stored_at("other", &new, now)?;
        assert_eq!(
            graph.compaction_point_as(now, MONTH, "me", &new),
            Some(new.clone())
        );
        assert_eq!(graph.compaction_point(now, MONTH), Some(old));

        // A device that has never recorded counts too, and counts as seen now.
        let fresh = Graph::new();
        let v = fresh.version();
        assert_eq!(fresh.compaction_point_as(now, MONTH, "me", &v), Some(v));
        Ok(())
    }
}
