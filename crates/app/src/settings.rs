//! The user's interface preferences — language and theme — remembered between launches.
//!
//! Stored in `$XDG_CONFIG_HOME/cleanator/settings.json` (default `~/.config/cleanator/`).
//! This is the only file the application ever writes besides the cleanup script the user
//! asks for. Nothing else is stored: no scan results, no paths, no telemetry (R10).
//!
//! Resolution order for the language: `CLEANATOR_LANG`, then the saved preference, then
//! the system language. Same for the theme with `CLEANATOR_THEME`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// `"en"` or `"fr"`; absent until the user toggles the language once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// `"dark"` or `"light"`; absent until the user toggles the theme once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("cleanator").join("settings.json"))
    }

    /// The saved settings, or defaults when there is no file or it cannot be read.
    pub fn load() -> Settings {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Best effort: a failure to save never bothers the user (the toggle still applies).
    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, json + "\n");
        }
    }

    /// Language to start with: environment, saved preference, system.
    pub fn initial_lang(&self) -> Lang {
        if let Ok(v) = std::env::var("CLEANATOR_LANG") {
            if let Some(l) = Lang::from_code(&v) {
                return l;
            }
        }
        self.lang
            .as_deref()
            .and_then(Lang::from_code)
            .unwrap_or_else(Lang::from_env)
    }

    /// Dark theme to start with: environment, saved preference, light otherwise.
    pub fn initial_dark(&self) -> bool {
        match std::env::var("CLEANATOR_THEME").as_deref() {
            Ok("dark") => return true,
            Ok("light") => return false,
            _ => {}
        }
        self.theme.as_deref() == Some("dark")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_unknown_fields() {
        let s = Settings {
            lang: Some("en".into()),
            theme: Some("dark".into()),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        let partial: Settings = serde_json::from_str(r#"{"lang":"fr","future_key":1}"#).unwrap();
        assert_eq!(partial.lang.as_deref(), Some("fr"));
        assert_eq!(partial.theme, None);
        let empty: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Settings::default());
    }
}
