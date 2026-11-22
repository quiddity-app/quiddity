//! Quiddity's contacts, in Android's.
//!
//! Android has one contacts provider, which the system owns, so an app that
//! has contacts of its own doesn't *be* a contacts provider: it keeps an
//! account, and puts its contacts in the system's under that account, where
//! every contacts app shows them and can edit them. This is the phone's side
//! of [`quiddity_contacts`]: [`Phone`] is its [`Store`], which the Android
//! library (`android/`) does the work of.
//!
//! On anything but Android there is no such thing, and [`Contacts::status`]
//! says so.

use std::marker::PhantomData;

use quiddity_contacts::{Contact, Edited, Held, Store};
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "android")]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "uk.co.aylett.quiddity.contacts";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Not Android, so there are no contacts to keep.
    #[error("there are no contacts to keep on this system")]
    Unavailable,
    #[cfg(target_os = "android")]
    #[error("{0}")]
    Android(#[from] tauri::plugin::mobile::PluginInvokeError),
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Where contacts stand on this system, for the page to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// Whether this system has contacts to put them in.
    pub available: bool,
    /// Whether the app may use them: `granted`, `denied` or `prompt` (not
    /// yet asked).
    pub permission: String,
    /// Whether the account the contacts are kept under exists, which is
    /// what it means for them to be on: removing the account in the
    /// system's settings turns them off.
    pub enabled: bool,
}

impl Status {
    const NONE: Self = Self {
        available: false,
        permission: String::new(),
        enabled: false,
    };
}

/// The phone's contacts, as an app's state. Get it with [`ContactsExt`].
pub struct Contacts<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _marker: PhantomData<fn() -> R>,
}

impl<R: Runtime> std::fmt::Debug for Contacts<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Contacts")
    }
}

#[cfg(target_os = "android")]
impl<R: Runtime> Contacts<R> {
    fn call<T: serde::de::DeserializeOwned>(
        &self,
        command: &str,
        args: impl Serialize,
    ) -> Result<T, Error> {
        Ok(self.handle.run_mobile_plugin(command, args)?)
    }

    pub fn status(&self) -> Result<Status, Error> {
        self.call("status", ())
    }

    /// Asks for the permission if it hasn't been given, then makes the
    /// account the contacts are kept under. Blocks while the user answers.
    pub fn enable(&self) -> Result<Status, Error> {
        self.call("enable", ())
    }

    /// Removes the account, and with it every contact kept under it. The
    /// graph is untouched.
    pub fn disable(&self) -> Result<Status, Error> {
        self.call("disable", ())
    }
}

#[cfg(not(target_os = "android"))]
impl<R: Runtime> Contacts<R> {
    pub fn status(&self) -> Result<Status, Error> {
        Ok(Status::NONE)
    }

    pub fn enable(&self) -> Result<Status, Error> {
        Err(Error::Unavailable)
    }

    pub fn disable(&self) -> Result<Status, Error> {
        Err(Error::Unavailable)
    }
}

/// What the Android side answers with for a list: a result is an object.
#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct Rows<T> {
    rows: Vec<T>,
}

/// The phone's contacts as a [`Store`]: what [`quiddity_contacts::reconcile`]
/// keeps in step with the graph. Calls block, so run it off the async threads.
#[derive(Debug)]
pub struct Phone<'a, R: Runtime>(pub &'a Contacts<R>);

#[cfg(target_os = "android")]
impl<R: Runtime> Store for Phone<'_, R> {
    type Error = Error;

    fn edited(&mut self) -> Result<Vec<Edited>, Error> {
        Ok(self.0.call::<Rows<Edited>>("edited", ())?.rows)
    }

    fn held(&mut self) -> Result<Vec<Held>, Error> {
        Ok(self.0.call::<Rows<Held>>("held", ())?.rows)
    }

    fn put(&mut self, row: Option<&str>, contact: &Contact) -> Result<String, Error> {
        #[derive(Deserialize)]
        struct Put {
            row: String,
        }
        let put: Put = self
            .0
            .call("put", serde_json::json!({ "row": row, "contact": contact }))?;
        Ok(put.row)
    }

    fn remove(&mut self, row: &str) -> Result<(), Error> {
        self.0
            .call::<serde_json::Value>("remove", serde_json::json!({ "row": row }))?;
        Ok(())
    }
}

#[cfg(not(target_os = "android"))]
impl<R: Runtime> Store for Phone<'_, R> {
    type Error = Error;

    fn edited(&mut self) -> Result<Vec<Edited>, Error> {
        Err(Error::Unavailable)
    }

    fn held(&mut self) -> Result<Vec<Held>, Error> {
        Err(Error::Unavailable)
    }

    fn put(&mut self, _: Option<&str>, _: &Contact) -> Result<String, Error> {
        Err(Error::Unavailable)
    }

    fn remove(&mut self, _: &str) -> Result<(), Error> {
        Err(Error::Unavailable)
    }
}

/// Access to the phone's contacts from anything that can manage state.
pub trait ContactsExt<R: Runtime> {
    fn contacts(&self) -> &Contacts<R>;
}

impl<R: Runtime, T: Manager<R>> ContactsExt<R> for T {
    fn contacts(&self) -> &Contacts<R> {
        self.state::<Contacts<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("quiddity-contacts")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let contacts = Contacts {
                handle: api.register_android_plugin(PLUGIN_IDENTIFIER, "ContactsPlugin")?,
            };
            #[cfg(not(target_os = "android"))]
            let contacts = {
                let _ = api;
                Contacts::<R> {
                    _marker: PhantomData,
                }
            };
            app.manage(contacts);
            Ok(())
        })
        .build()
}
