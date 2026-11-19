//! Chooses the GDK backend on Linux.
//!
//! Measured on niri (Wayland, Intel Iris Xe, fractional scale 1.25): the native Wayland backend fails
//! to initialise GL ("GTK failed to initialize GL: No available configurations for the given pixel
//! format"), so WebKit paints in software and, because GTK3 rounds the scale up to 2, at four times
//! the pixels. Scrolling a long page ran at about 3 fps. Under XWayland the same build holds 60 fps
//! (GL works and the scale stays 1.25). So on that compositor the app starts through XWayland.
//!
//! Other desktops are left alone: GNOME and KDE usually have working GL on native Wayland, and
//! XWayland would only make their fractional scaling blurry.
//!
//! Override with `AMI_DISPLAY=wayland` (never switch) or `AMI_DISPLAY=x11` (switch wherever an X
//! display exists). An explicit `GDK_BACKEND` always wins.

/// Whether to start through XWayland, given an environment lookup (injectable for tests).
pub fn wants_x11(var: impl Fn(&str) -> Option<String>) -> bool {
    if var("GDK_BACKEND").is_some() {
        return false; // the user already chose
    }
    let has_x = var("DISPLAY").is_some();
    match var("AMI_DISPLAY").as_deref() {
        Some("wayland") => return false,
        Some("x11") => return has_x,
        _ => {}
    }
    if var("WAYLAND_DISPLAY").is_none() || !has_x {
        return false;
    }
    let desktop = format!(
        "{} {}",
        var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        var("XDG_SESSION_DESKTOP").unwrap_or_default()
    )
    .to_lowercase();
    desktop
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == "niri")
}

/// Call first thing in `main`, before GTK is initialised and before any other thread exists.
#[cfg(target_os = "linux")]
pub fn prefer_x11_where_native_wayland_is_slow() {
    if wants_x11(|k| std::env::var(k).ok().filter(|v| !v.is_empty())) {
        // SAFETY: nothing else runs yet, so no other thread can read the environment concurrently.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    const NIRI: [(&str, &str); 3] = [
        ("WAYLAND_DISPLAY", "wayland-1"),
        ("DISPLAY", ":1"),
        ("XDG_CURRENT_DESKTOP", "niri"),
    ];

    #[test]
    fn niri_with_xwayland_starts_through_x11() {
        assert!(wants_x11(env(&NIRI)));
        // The session variable alone is enough, and case does not matter.
        assert!(wants_x11(env(&[
            ("WAYLAND_DISPLAY", "w"),
            ("DISPLAY", ":0"),
            ("XDG_SESSION_DESKTOP", "Niri")
        ])));
    }

    #[test]
    fn other_desktops_keep_native_wayland() {
        for desktop in ["GNOME", "KDE", "sway", "Hyprland", "ubuntu:GNOME", ""] {
            let e = env(&[
                ("WAYLAND_DISPLAY", "w"),
                ("DISPLAY", ":0"),
                ("XDG_CURRENT_DESKTOP", desktop),
            ]);
            assert!(!wants_x11(e), "{desktop}");
        }
        // "niri" must be a whole word, not part of another name.
        assert!(!wants_x11(env(&[
            ("WAYLAND_DISPLAY", "w"),
            ("DISPLAY", ":0"),
            ("XDG_CURRENT_DESKTOP", "sniriff")
        ])));
    }

    #[test]
    fn without_an_x_display_or_a_wayland_session_nothing_changes() {
        assert!(!wants_x11(env(&[
            ("WAYLAND_DISPLAY", "w"),
            ("XDG_CURRENT_DESKTOP", "niri")
        ])));
        assert!(!wants_x11(env(&[
            ("DISPLAY", ":0"),
            ("XDG_CURRENT_DESKTOP", "niri")
        ])));
        assert!(!wants_x11(env(&[])));
    }

    #[test]
    fn an_explicit_choice_always_wins() {
        let mut with_backend = NIRI.to_vec();
        with_backend.push(("GDK_BACKEND", "wayland"));
        assert!(!wants_x11(env(&with_backend)));
        let mut opt_out = NIRI.to_vec();
        opt_out.push(("AMI_DISPLAY", "wayland"));
        assert!(!wants_x11(env(&opt_out)));
    }

    #[test]
    fn ami_display_x11_forces_it_wherever_an_x_display_exists() {
        assert!(wants_x11(env(&[
            ("AMI_DISPLAY", "x11"),
            ("DISPLAY", ":0"),
            ("XDG_CURRENT_DESKTOP", "GNOME")
        ])));
        assert!(!wants_x11(env(&[
            ("AMI_DISPLAY", "x11"),
            ("XDG_CURRENT_DESKTOP", "GNOME")
        ])));
        // An unknown value is ignored.
        assert!(wants_x11(env(&[
            ("AMI_DISPLAY", "auto"),
            ("WAYLAND_DISPLAY", "w"),
            ("DISPLAY", ":0"),
            ("XDG_CURRENT_DESKTOP", "niri")
        ])));
    }
}
