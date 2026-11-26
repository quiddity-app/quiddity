//! What a peer has recently done on the network, kept in memory, for a person
//! working out why two devices don't sync.
//!
//! The same things are sent to `tracing`, but a phone has nowhere to read
//! that, and the interesting ones (a refused device, another graph) are not
//! errors, so they are not shown at the default level. This is the same story
//! kept where the debug page can show it. It holds device IDs and the reasons
//! connections ended, and nothing from the graph's content.

use std::collections::VecDeque;

use jiff::Timestamp;

/// How many events are kept: the oldest go first. A device that is off is
/// retried about once a minute, so this is a few hours of that.
pub const CAPACITY: usize = 200;

/// How much an event matters to someone troubleshooting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Something expected: a connection made or ended.
    Info,
    /// Something that stops devices syncing: a refusal, another graph, a
    /// connection that couldn't be made.
    Problem,
}

impl Level {
    /// As one word, for output that a program reads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Problem => "problem",
        }
    }
}

/// One thing a peer did or was told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// When, in milliseconds since the Unix epoch.
    pub at: i64,
    pub level: Level,
    /// The device it was about, by its whole ID, if it is about one.
    pub device: Option<String>,
    /// What happened, in a sentence.
    pub message: String,
}

/// The most recent events, oldest first.
#[derive(Debug, Default)]
pub struct Events(VecDeque<Event>);

impl Events {
    /// Keeps `message` as the latest event, and forgets the oldest if there are
    /// now too many.
    pub fn push(&mut self, level: Level, device: Option<&str>, message: String) {
        if self.0.len() == CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back(Event {
            at: Timestamp::now().as_millisecond(),
            level,
            device: device.map(str::to_owned),
            message,
        });
    }

    #[must_use]
    pub fn all(&self) -> Vec<Event> {
        self.0.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oldest_events_are_forgotten_first() {
        let mut events = Events::default();
        for n in 0..CAPACITY + 3 {
            events.push(Level::Info, None, n.to_string());
        }
        let kept = events.all();
        assert_eq!(kept.len(), CAPACITY);
        assert_eq!(kept.first().unwrap().message, "3");
        assert_eq!(kept.last().unwrap().message, (CAPACITY + 2).to_string());
    }

    #[test]
    fn a_level_is_one_word() {
        assert_eq!(Level::Info.as_str(), "info");
        assert_eq!(Level::Problem.as_str(), "problem");
    }

    #[test]
    fn an_event_says_when_who_and_what() {
        let mut events = Events::default();
        let before = Timestamp::now().as_millisecond();
        events.push(Level::Problem, Some("abc"), "refused".to_owned());
        let [event] = events.all().try_into().unwrap();
        assert!(event.at >= before);
        assert_eq!(event.level, Level::Problem);
        assert_eq!(event.device.as_deref(), Some("abc"));
        assert_eq!(event.message, "refused");
    }
}
