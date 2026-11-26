/// Linux MPRIS (Media Player Remote Interfacing Specification) backend over D-Bus
/// Controls dedicated music players (Spotify, VLC, Rhythmbox, etc.) while ignoring web browsers.

use crate::media_backend::TrackInfo;

#[cfg(any(target_os = "linux", test))]
pub(crate) fn is_browser_identity(identity: &str, bus_name: &str) -> bool {
    let id = identity.to_ascii_lowercase();
    let bus = bus_name.to_ascii_lowercase();
    const BROWSER_MARKERS: [&str; 14] = [
        "chrome",
        "chromium",
        "firefox",
        "brave",
        "edge",
        "msedge",
        "opera",
        "vivaldi",
        "zen",
        "arc",
        "floorp",
        "librewolf",
        "waterfox",
        "plasma-browser-integration",
    ];
    BROWSER_MARKERS
        .iter()
        .any(|marker| id.contains(marker) || bus.contains(marker))
}

#[cfg(target_os = "linux")]
use mpris::{PlaybackStatus, PlayerFinder};

#[cfg(target_os = "linux")]
pub struct MprisBackend;

#[cfg(target_os = "linux")]
impl MprisBackend {
    pub fn new() -> Self {
        Self
    }

    fn get_player(&self) -> Result<mpris::Player, String> {
        let finder = PlayerFinder::new().map_err(|e| format!("Failed to connect to D-Bus: {}", e))?;
        let players = finder
            .find_all()
            .map_err(|e| format!("No MPRIS players found: {}", e))?;

        let non_browsers: Vec<mpris::Player> = players
            .into_iter()
            .filter(|p| !is_browser_identity(p.identity(), p.bus_name()))
            .collect();

        if non_browsers.is_empty() {
            return Err("No non-browser MPRIS media player found".into());
        }

        let mut paused_fallback: Option<mpris::Player> = None;
        let mut any_fallback: Option<mpris::Player> = None;

        for player in non_browsers {
            match player.get_playback_status() {
                Ok(PlaybackStatus::Playing) => return Ok(player),
                Ok(PlaybackStatus::Paused) if paused_fallback.is_none() => {
                    paused_fallback = Some(player);
                }
                _ if any_fallback.is_none() => {
                    any_fallback = Some(player);
                }
                _ => {}
            }
        }

        paused_fallback
            .or(any_fallback)
            .ok_or_else(|| "No active non-browser MPRIS media player found".into())
    }

    pub fn pause(&self) -> Result<(), String> {
        let player = self.get_player()?;
        player.pause().map_err(|e| e.to_string())?;
        println!("[MPRIS] Paused media player: {}", player.identity());
        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        let player = self.get_player()?;
        player.play().map_err(|e| e.to_string())?;
        println!("[MPRIS] Resumed media player: {}", player.identity());
        Ok(())
    }

    #[allow(dead_code)]
    pub fn toggle(&self) -> Result<(), String> {
        let player = self.get_player()?;
        player.play_pause().map_err(|e| e.to_string())?;
        println!("[MPRIS] Toggled media player: {}", player.identity());
        Ok(())
    }

    pub fn skip_next(&self) -> Result<(), String> {
        let player = self.get_player()?;
        player.next().map_err(|e| e.to_string())?;
        println!("[MPRIS] Skipped to next track");
        Ok(())
    }

    pub fn skip_prev(&self) -> Result<(), String> {
        let player = self.get_player()?;
        player.previous().map_err(|e| e.to_string())?;
        println!("[MPRIS] Skipped to previous track");
        Ok(())
    }

    pub fn seek_backward(&self, seconds: u32) -> Result<(), String> {
        let player = self.get_player()?;
        let duration = std::time::Duration::from_secs(seconds as u64);
        player.seek_backwards(&duration).map_err(|e| e.to_string())?;
        println!("[MPRIS] Smart Rewind: stepped back {}s", seconds);
        Ok(())
    }

    pub fn set_volume(&self, volume_percent: u32) -> Result<(), String> {
        let player = self.get_player()?;
        let vol = (volume_percent as f64 / 100.0).clamp(0.0, 1.0);
        player.set_volume(vol).map_err(|e| e.to_string())?;
        println!("[MPRIS] Set volume to {}% on {}", volume_percent, player.identity());
        Ok(())
    }

    pub fn get_volume(&self) -> Option<u32> {
        self.get_player()
            .ok()
            .and_then(|p| p.get_volume().ok())
            .map(|v| (v * 100.0).round() as u32)
    }

    pub fn is_playing(&self) -> bool {
        self.get_player()
            .map(|p| p.get_playback_status().ok() == Some(PlaybackStatus::Playing))
            .unwrap_or(false)
    }

    pub fn get_track_info(&self) -> TrackInfo {
        let player = match self.get_player() {
            Ok(p) => p,
            Err(_) => return TrackInfo::default(),
        };

        let is_playing = player.get_playback_status().ok() == Some(PlaybackStatus::Playing);
        let identity = player.identity().to_string();

        let (track_name, artist_name, album_name, album_art) = match player.get_metadata() {
            Ok(meta) => {
                let title = meta.title().unwrap_or("").to_string();
                let artist = meta.artists().map(|a| a.join(", ")).unwrap_or_default();
                let album = meta.album_name().unwrap_or("").to_string();
                let art = meta.art_url().unwrap_or("").to_string();
                (title, artist, album, art)
            }
            Err(_) => (String::new(), String::new(), String::new(), String::new()),
        };

        TrackInfo {
            is_playing,
            track_name,
            artist_name,
            album_art,
            album_name,
            device_name: identity,
            device_type: "Linux".to_string(),
            volume_percent: None,
        }
    }
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub struct MprisBackend;

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
impl MprisBackend {
    pub fn new() -> Self { Self }
    pub fn pause(&self) -> Result<(), String> { Err("MPRIS not supported on this platform".into()) }
    pub fn resume(&self) -> Result<(), String> { Err("MPRIS not supported on this platform".into()) }
    #[allow(dead_code)]
    pub fn toggle(&self) -> Result<(), String> { Err("MPRIS not supported on this platform".into()) }
    pub fn skip_next(&self) -> Result<(), String> { Err("MPRIS not supported on this platform".into()) }
    pub fn skip_prev(&self) -> Result<(), String> { Err("MPRIS not supported on this platform".into()) }
    pub fn seek_backward(&self, _seconds: u32) -> Result<(), String> { Err("MPRIS not supported on this platform".into()) }
    pub fn set_volume(&self, _volume_percent: u32) -> Result<(), String> { Err("MPRIS not supported on this platform".into()) }
    pub fn get_volume(&self) -> Option<u32> { None }
    pub fn is_playing(&self) -> bool { false }
    pub fn get_track_info(&self) -> TrackInfo { TrackInfo::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_browser_mpris_identities() {
        assert!(is_browser_identity("Mozilla Firefox", "org.mpris.MediaPlayer2.firefox.instance123"));
        assert!(is_browser_identity("Google Chrome", "org.mpris.MediaPlayer2.chromium.instance9"));
        assert!(is_browser_identity("Brave", "org.mpris.MediaPlayer2.chromium.instance42"));
        assert!(!is_browser_identity("Spotify", "org.mpris.MediaPlayer2.spotify"));
        assert!(!is_browser_identity("VLC media player", "org.mpris.MediaPlayer2.vlc"));
    }
}
