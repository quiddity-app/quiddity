//! Entities as contacts.
//!
//! An entity with a value for a property that `##contact` maps to a field is
//! a contact ([`contacts`]). A phone's contacts app edits its own copy, so an
//! edit comes back as what the contact was when it was last handed over, what
//! it is now, and which entity it was: [`apply`] works out what changed and
//! makes the same change to the entity, which may by then have moved on.
//!
//! Nothing here knows about any platform. A [`Contact`] is plain data that
//! serialises, for whatever carries it.

mod model;
mod write;

pub use model::{Contact, Entry, contact_of, contacts};
pub use write::{Applied, apply, forget};
