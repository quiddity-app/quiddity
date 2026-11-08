//! The colours the window shows itself, which the page can't reach: what is
//! behind the page before it has painted (the flash of white a dark window
//! otherwise opens with) and in the margin of a resize.
//!
//! They are the page's own ground, `--surface` in `apps/ui/app/globals.css`,
//! chosen by the system's theme as the page chooses by
//! `prefers-color-scheme`. A test holds them to the stylesheet. The title bar
//! isn't ours to colour: the system draws it, in the same theme.

use tauri::window::Color;
use tauri::{Theme, WebviewWindow, WindowEvent};

/// `--surface` in the light theme (slate 50).
const LIGHT: Color = Color(248, 250, 252, 255);
/// `--surface` in the dark theme (slate 900).
const DARK: Color = Color(15, 23, 42, 255);

/// The ground for a theme.
pub fn background(theme: Theme) -> Color {
    match theme {
        Theme::Dark => DARK,
        _ => LIGHT,
    }
}

/// Paints the window's background for the system's theme now, and again when
/// the system changes it.
pub fn follow_theme(window: &WebviewWindow) {
    paint(window, window.theme().unwrap_or(Theme::Light));
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::ThemeChanged(theme) = event {
            paint(&handle, *theme);
        }
    });
}

fn paint(window: &WebviewWindow, theme: Theme) {
    // A window that can't be painted is still a working window.
    if let Err(error) = window.set_background_color(Some(background(theme))) {
        eprintln!("could not set the window's background: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `--name: #rrggbb;` in the stylesheet, in order.
    fn declared(css: &str, name: &str) -> Vec<Color> {
        let key = format!("{name}:");
        css.match_indices(&key)
            // `--surface:` and not `--surface-pane:`, or a longer name ending in it.
            .filter(|(at, _)| !css[..*at].ends_with(|c: char| c.is_alphanumeric() || c == '-'))
            .filter_map(|(at, _)| {
                let hex = css[at + key.len()..]
                    .trim_start()
                    .strip_prefix('#')?
                    .get(..6)?;
                let byte = |from: usize| u8::from_str_radix(&hex[from..from + 2], 16).ok();
                Some(Color(byte(0)?, byte(2)?, byte(4)?, 255))
            })
            .collect()
    }

    #[test]
    fn the_window_is_painted_in_the_pages_own_ground() {
        let css = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ui/app/globals.css"
        ))
        .expect("the UI's stylesheet");
        let surface = declared(&css, "--surface");
        // The light value is declared first, in :root, and the dark one in the
        // media query that follows.
        assert_eq!(surface, [LIGHT, DARK]);
    }

    #[test]
    fn a_theme_chooses_its_ground() {
        assert_eq!(background(Theme::Dark), DARK);
        assert_eq!(background(Theme::Light), LIGHT);
    }
}
