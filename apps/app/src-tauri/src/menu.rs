//! The macOS menu bar: Delete Entity in the File menu, and a Go menu beside it
//! that goes to the app's entrypoints. The rest is Tauri's default menu, so
//! nothing the app already had is lost.
//!
//! The menu is only a way of asking: it tells the page what was chosen, on the
//! `menu` event, and the page does it (`apps/ui/lib/menu.ts`), since the page
//! knows which entity is open and does the navigating and the deleting. The
//! event's payload is the item's ID: `delete-entity`, `check-for-updates`, or
//! `go:` and a path.

use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tracing::warn;

/// The Go menu: each entrypoint's title and path. The paths are those in
/// `entrypoints` in `apps/ui/lib/nav.ts`, which a test holds this to.
#[cfg(any(target_os = "macos", test))]
const GO: [(&str, &str); 5] = [
    ("Journals", "/journals/"),
    ("All Entities", "/all/"),
    ("Graph", "/graph/"),
    ("Devices", "/devices/"),
    ("Stats and Debug", "/stats/"),
];

#[cfg(target_os = "macos")]
const DELETE_ENTITY: &str = "delete-entity";

#[cfg(target_os = "macos")]
const CHECK_FOR_UPDATES: &str = "check-for-updates";

/// Says whether an entity is open, which is when Delete Entity can be chosen.
/// There is no menu to change anywhere but macOS.
pub fn set_entity_open(app: &AppHandle, open: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        if let Some(item) = app.try_state::<native::DeleteEntity>()
            && let Err(error) = item.0.set_enabled(open)
        {
            warn!(%error, "couldn't update the menu");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, open);
}

#[cfg(target_os = "macos")]
pub use native::{install, on_event};

#[cfg(target_os = "macos")]
mod native {
    use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tauri::{AppHandle, Emitter, Manager, Wry};
    use tracing::warn;

    use super::{CHECK_FOR_UPDATES, DELETE_ENTITY, GO};

    /// The File menu's Delete Entity, kept to enable and disable it.
    pub struct DeleteEntity(pub MenuItem<Wry>);

    /// Puts the menu bar in place. Delete Entity starts disabled, as no entity
    /// is open until the page says so.
    pub fn install(app: &AppHandle) -> tauri::Result<()> {
        let menu = Menu::default(app)?;
        let delete = MenuItem::with_id(app, DELETE_ENTITY, "Delete Entity", false, None::<&str>)?;

        // Tauri's File menu is found by its title: it has no ID. If a later
        // version changes it, make our own rather than go without.
        let items = menu.items()?;
        let file = items.iter().enumerate().find_map(|(at, item)| {
            let submenu = item.as_submenu()?;
            (submenu.text().ok()? == "File").then_some((at, submenu))
        });
        let file_at = match file {
            Some((at, file)) => {
                // Above Close Window, which comes last.
                file.insert(&delete, 0)?;
                file.insert(&PredefinedMenuItem::separator(app)?, 1)?;
                at
            }
            None => {
                menu.insert(&Submenu::with_items(app, "File", true, &[&delete])?, 1)?;
                1
            }
        };

        // The first menu is the app's, whose first item is About: macOS puts
        // Check for Updates straight after it.
        if let Some(app_menu) = items.first().and_then(|item| item.as_submenu()) {
            let check = MenuItem::with_id(
                app,
                CHECK_FOR_UPDATES,
                "Check for Updates…",
                true,
                None::<&str>,
            )?;
            app_menu.insert(&check, 1)?;
        }

        let entries = GO
            .iter()
            .map(|(title, path)| {
                MenuItem::with_id(app, format!("go:{path}"), title, true, None::<&str>)
            })
            .collect::<tauri::Result<Vec<_>>>()?;
        let entries: Vec<&dyn IsMenuItem<Wry>> =
            entries.iter().map(|e| e as &dyn IsMenuItem<Wry>).collect();
        menu.insert(
            &Submenu::with_items(app, "Go", true, &entries)?,
            file_at + 1,
        )?;

        app.set_menu(menu)?;
        app.manage(DeleteEntity(delete));
        Ok(())
    }

    /// Passes what was chosen on to the page.
    pub fn on_event(app: &AppHandle, event: MenuEvent) {
        let id = event.id().as_ref();
        if (id == DELETE_ENTITY || id == CHECK_FOR_UPDATES || id.starts_with("go:"))
            && let Err(error) = app.emit_to("main", "menu", id)
        {
            warn!(%error, id, "couldn't tell the page what was chosen");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Go menu is the UI's entrypoints, so that a page added there is added
    /// here as well.
    #[test]
    fn the_go_menu_has_every_entrypoint_in_the_same_order() {
        let nav =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/lib/nav.ts"))
                .expect("the UI's navigation is beside the app");
        // A path is the string after `href: `, in whichever quotes the formatter
        // leaves it: what follows `href: ` in a type is not a string.
        let paths: Vec<&str> = nav
            .match_indices("href: ")
            .filter_map(|(at, key)| {
                let rest = &nav[at + key.len()..];
                let quote = rest.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
                let rest = &rest[1..];
                Some(&rest[..rest.find(quote).expect("a closing quote")])
            })
            .collect();
        let ours: Vec<&str> = GO.iter().map(|(_, path)| *path).collect();
        assert_eq!(paths, ours);
    }
}
