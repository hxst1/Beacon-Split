use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{CoreError, Result};

/// Which palette the window uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// Follow the operating system, which is what most people expect and what
    /// makes Beacon match everything else on screen at dusk.
    #[default]
    System,
    Dark,
    Light,
}

/// How much of the desktop shows through the window, and how.
///
/// Tastes rather than settings with a right answer. Opacity is a CSS variable,
/// so changing it is a repaint; frosting is a window effect the operating
/// system applies, because nothing inside the page can reach past it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub theme: Theme,
    /// Opacity of the window's own background, 0.5..1.
    ///
    /// Never below a half: at that point the desktop competes with the text,
    /// and a window you cannot read is not a preference, it is a broken window.
    pub window_opacity: f32,
    /// Whether what shows through is frosted rather than sharp.
    ///
    /// A switch and not an amount, which is the honest shape for it. This was
    /// a blur radius in pixels once, fed to `backdrop-filter`, and it did
    /// nothing at all: a backdrop filter composites what is behind an element
    /// *within the page*, and behind Beacon's chrome is a flat colour. Blurring
    /// a flat colour returns the same flat colour. Reaching the desktop needs a
    /// window effect, and the operating system picks the radius for that — so
    /// an amount was a control with one working position and forty that lied.
    ///
    /// Reads an old configuration's `blur` number, where anything above zero
    /// meant it was meant to be on.
    #[serde(
        alias = "blur",
        default = "frosted_by_default",
        deserialize_with = "frosted_or_legacy_radius"
    )]
    pub frosted: bool,
}

fn frosted_by_default() -> bool {
    Appearance::default().frosted
}

/// Accepts `true`/`false`, or the blur radius this used to be.
fn frosted_or_legacy_radius<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Frosted(bool),
        Radius(f32),
    }

    Ok(match Stored::deserialize(deserializer)? {
        Stored::Frosted(on) => on,
        Stored::Radius(px) => px > 0.0,
    })
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            window_opacity: 0.86,
            frosted: true,
        }
    }
}

impl Appearance {
    pub const MIN_OPACITY: f32 = 0.5;
    pub const MAX_OPACITY: f32 = 1.0;

    /// Pulls values into a range that still produces a usable window.
    pub fn clamped(&self) -> Self {
        Self {
            theme: self.theme,
            window_opacity: clamp_finite(self.window_opacity, Self::MIN_OPACITY, Self::MAX_OPACITY),
            frosted: self.frosted,
        }
    }

    pub fn validated(&self) -> Result<Self> {
        if !self.window_opacity.is_finite() {
            return Err(CoreError::invalid("opacity must be a number"));
        }
        Ok(self.clamped())
    }
}

/// `clamp` on its own happily returns NaN, which then travels into CSS.
fn clamp_finite(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_look_beacon_shipped_with() {
        let appearance = Appearance::default();
        assert_eq!(appearance.theme, Theme::System);
        assert_eq!(appearance.window_opacity, 0.86);
        assert!(appearance.frosted);
    }

    #[test]
    fn a_window_can_never_be_made_unreadable() {
        let wild = Appearance {
            theme: Theme::Dark,
            window_opacity: 0.05,
            frosted: true,
        };
        assert_eq!(wild.clamped().window_opacity, Appearance::MIN_OPACITY);
    }

    /// Frosting used to be a blur radius in pixels. Somebody upgrading has that
    /// number in their configuration, and it has to keep meaning what they
    /// chose rather than resetting their window.
    #[test]
    fn a_configuration_written_before_frosting_was_a_switch_still_loads() {
        let on: Appearance =
            serde_json::from_str(r#"{"theme":"dark","windowOpacity":0.8,"blur":18.0}"#).unwrap();
        assert!(on.frosted, "a radius above zero meant frosting was wanted");

        let off: Appearance =
            serde_json::from_str(r#"{"theme":"dark","windowOpacity":0.8,"blur":0.0}"#).unwrap();
        assert!(!off.frosted, "zero meant it had been turned off");

        let current: Appearance =
            serde_json::from_str(r#"{"theme":"dark","windowOpacity":0.8,"frosted":false}"#)
                .unwrap();
        assert!(!current.frosted);

        let absent: Appearance =
            serde_json::from_str(r#"{"theme":"dark","windowOpacity":0.8}"#).unwrap();
        assert_eq!(absent.frosted, Appearance::default().frosted);
    }

    #[test]
    fn nonsense_is_refused_rather_than_written_into_css() {
        // NaN survives `clamp`, and `blur(NaNpx)` is a silently broken window.
        let broken = Appearance {
            window_opacity: f32::NAN,
            ..Appearance::default()
        };
        assert!(broken.validated().is_err());
        assert_eq!(broken.clamped().window_opacity, Appearance::MIN_OPACITY);
    }

    #[test]
    fn a_settings_file_without_an_appearance_gets_the_default() {
        let appearance: Appearance =
            serde_json::from_str(r#"{"theme":"light","windowOpacity":0.9,"blur":4.0}"#).unwrap();
        assert_eq!(appearance.theme, Theme::Light);
        assert_eq!(appearance.window_opacity, 0.9);
    }
}
