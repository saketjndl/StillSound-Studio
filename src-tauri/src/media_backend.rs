#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Which backend handles media control
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// OS-native media session control (SMTC on Windows, MPRIS on Linux)
    /// Works with any media player, needs zero setup
    Universal,
    /// Spotify Web API (OAuth PKCE)
    /// Full features: album art glow, volume sync, multi-device handoff
    Spotify,
}

impl Default for SyncMode {
    fn default() -> Self {
        SyncMode::Universal
    }
}

impl std::fmt::Display for SyncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncMode::Universal => write!(f, "universal"),
            SyncMode::Spotify => write!(f, "spotify"),
        }
    }
}

/// Action to take when video/media plays on a specific site or app
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum SiteAction {
    Pause,
    Duck,
    Ignore,
}

impl Default for SiteAction {
    fn default() -> Self {
        SiteAction::Pause
    }
}

impl std::fmt::Display for SiteAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SiteAction::Pause => write!(f, "pause"),
            SiteAction::Duck => write!(f, "duck"),
            SiteAction::Ignore => write!(f, "ignore"),
        }
    }
}

/// Normalized track information returned by any media backend
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TrackInfo {
    pub is_playing: bool,
    pub track_name: String,
    pub artist_name: String,
    pub album_art: String,
    pub album_name: String,
    pub device_name: String,
    pub device_type: String,
    pub volume_percent: Option<u64>,
}

impl TrackInfo {
    /// Convert to the JSON format expected by the frontend and extension
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "spotify_state",
            "is_playing": self.is_playing,
            "track_name": self.track_name,
            "artist_name": self.artist_name,
            "album_art": self.album_art,
            "album_name": self.album_name,
            "volume_percent": self.volume_percent,
            "device_name": self.device_name,
            "device_type": self.device_type,
        })
    }
}
