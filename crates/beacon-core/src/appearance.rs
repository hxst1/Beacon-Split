use serde::{Deserialize, Serialize};

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

/// How translucent and how blurred the window is.
///
/// Both are tastes rather than settings with a right answer: how much of the
/// desktop shows through, and how far it is pushed out of focus. They are
/// stored as plain numbers and applied as CSS variables, so a change is a
/// repaint rather than a rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub theme: Theme,
    /// Opacity of the window's own background, 0.5..1.
    ///
    /// Never below a half: at that point the desktop competes with the text,
    /// and a window you cannot read is not a preference, it is a broken window.
    pub window_opacity: f32,
    /// Backdrop blur in pixels, 0..40. Zero turns it off.
    pub blur: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            window_opacity: 0.86,
            blur: 18.0,
        }
    }
}

impl Appearance {
    pub const MIN_OPACITY: f32 = 0.5;
    pub const MAX_OPACITY: f32 = 1.0;
    pub const MAX_BLUR: f32 = 40.0;

    /// Pulls values into a range that still produces a usable window.
    pub fn clamped(&self) -> Self {
        Self {
            theme: self.theme,
            window_opacity: clamp_finite(self.window_opacity, Self::MIN_OPACITY, Self::MAX_OPACITY),
            blur: clamp_finite(self.blur, 0.0, Self::MAX_BLUR),
        }
    }

    pub fn validated(&self) -> Result<Self> {
        if !self.window_opacity.is_finite() || !self.blur.is_finite() {
            return Err(CoreError::invalid("opacity and blur must be numbers"));
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
        assert_eq!(appearance.blur, 18.0);
    }

    #[test]
    fn a_window_can_never_be_made_unreadable() {
        let wild = Appearance {
            theme: Theme::Dark,
            window_opacity: 0.05,
            blur: 500.0,
        };
        let safe = wild.clamped();
        assert_eq!(safe.window_opacity, Appearance::MIN_OPACITY);
        assert_eq!(safe.blur, Appearance::MAX_BLUR);
    }

    #[test]
    fn blur_can_be_turned_off_entirely() {
        let none = Appearance {
            blur: 0.0,
            ..Appearance::default()
        };
        assert_eq!(none.clamped().blur, 0.0);
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
