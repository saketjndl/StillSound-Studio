/// macOS media session backend using AppleScript (osascript)
/// Controls Spotify or Apple Music on macOS without OAuth or external crates.

use crate::media_backend::TrackInfo;

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
pub struct MacosBackend;

#[cfg(target_os = "macos")]
impl MacosBackend {
    pub fn new() -> Self {
        Self
    }

    fn run_osascript(&self, script: &str) -> Result<String, String> {
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| format!("Failed to run AppleScript: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    pub fn pause(&self) -> Result<(), String> {
        self.run_osascript("if application \"Spotify\" is running then tell application \"Spotify\" to pause\nif application \"Music\" is running then tell application \"Music\" to pause")?;
        println!("[macOS] Paused media session via AppleScript");
        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        self.run_osascript("if application \"Spotify\" is running then tell application \"Spotify\" to play\nif application \"Music\" is running then tell application \"Music\" to play")?;
        println!("[macOS] Resumed media session via AppleScript");
        Ok(())
    }

    #[allow(dead_code)]
    pub fn toggle(&self) -> Result<(), String> {
        self.run_osascript("if application \"Spotify\" is running then tell application \"Spotify\" to playpause\nif application \"Music\" is running then tell application \"Music\" to playpause")?;
        println!("[macOS] Toggled media session via AppleScript");
        Ok(())
    }

    pub fn skip_next(&self) -> Result<(), String> {
        self.run_osascript("if application \"Spotify\" is running then tell application \"Spotify\" to next track\nif application \"Music\" is running then tell application \"Music\" to next track")?;
        println!("[macOS] Skipped to next track via AppleScript");
        Ok(())
    }

    pub fn skip_prev(&self) -> Result<(), String> {
        self.run_osascript("if application \"Spotify\" is running then tell application \"Spotify\" to previous track\nif application \"Music\" is running then tell application \"Music\" to previous track")?;
        println!("[macOS] Skipped to previous track via AppleScript");
        Ok(())
    }

    pub fn seek_backward(&self, seconds: u32) -> Result<(), String> {
        let _ = self.run_osascript(&format!(
            "if application \"Spotify\" is running then tell application \"Spotify\" to set player position to (player position - {})\nif application \"Music\" is running then tell application \"Music\" to set player position to (player position - {})",
            seconds, seconds
        ));
        println!("[macOS] Smart Rewind: stepped back {}s", seconds);
        Ok(())
    }

    pub fn set_volume(&self, volume_percent: u32) -> Result<(), String> {
        let vol = volume_percent.clamp(0, 100);
        let script = format!(
            "if application \"Spotify\" is running then tell application \"Spotify\" to set sound volume to {}\nif application \"Music\" is running then tell application \"Music\" to set sound volume to {}",
            vol, vol
        );
        let _ = self.run_osascript(&script);
        println!("[macOS] Set volume to {}% via AppleScript", vol);
        Ok(())
    }

    pub fn get_volume(&self) -> Option<u32> {
        // AppleScript sound volume is 0–100 on Spotify/Music
        let script = "if application \"Spotify\" is running then return (sound volume of application \"Spotify\")\nif application \"Music\" is running then return (sound volume of application \"Music\")\nreturn \"\"";
        self.run_osascript(script)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    pub fn is_playing(&self) -> bool {
        let res = self.run_osascript("if application \"Spotify\" is running then return (player state of application \"Spotify\" as string)\nif application \"Music\" is running then return (player state of application \"Music\" as string)\nreturn \"stopped\"");
        match res {
            Ok(state) => state.to_lowercase() == "playing",
            Err(_) => false,
        }
    }

    pub fn get_track_info(&self) -> TrackInfo {
        let script = r#"
        if application "Spotify" is running then
            tell application "Spotify"
                set state to player state as string
                set trackName to name of current track
                set artistName to artist of current track
                set albumName to album of current track
                set artUrl to artwork url of current track
                return state & "|||" & trackName & "|||" & artistName & "|||" & albumName & "|||" & artUrl & "|||Spotify"
            end tell
        else if application "Music" is running then
            tell application "Music"
                set state to player state as string
                set trackName to name of current track
                set artistName to artist of current track
                set albumName to album of current track
                return state & "|||" & trackName & "|||" & artistName & "|||" & albumName & "||||||Apple Music"
            end tell
        else
            return "stopped|||||||||||||||"
        end if
        "#;

        let output = self.run_osascript(script).unwrap_or_default();
        let parts: Vec<&str> = output.split("|||").collect();
        if parts.len() < 6 {
            return TrackInfo::default();
        }

        let is_playing = parts[0].to_lowercase() == "playing";
        let track_name = parts[1].to_string();
        let artist_name = parts[2].to_string();
        let album_name = parts[3].to_string();
        let album_art = parts[4].to_string();
        let device_name = parts[5].to_string();

        TrackInfo {
            is_playing,
            track_name,
            artist_name,
            album_art,
            album_name,
            device_name,
            device_type: "macOS".to_string(),
            volume_percent: None,
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
pub struct MacosBackend;

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
impl MacosBackend {
    pub fn new() -> Self { Self }
    pub fn pause(&self) -> Result<(), String> { Err("macOS backend not supported on this platform".into()) }
    pub fn resume(&self) -> Result<(), String> { Err("macOS backend not supported on this platform".into()) }
    #[allow(dead_code)]
    pub fn toggle(&self) -> Result<(), String> { Err("macOS backend not supported on this platform".into()) }
    pub fn skip_next(&self) -> Result<(), String> { Err("macOS backend not supported on this platform".into()) }
    pub fn skip_prev(&self) -> Result<(), String> { Err("macOS backend not supported on this platform".into()) }
    pub fn seek_backward(&self, _seconds: u32) -> Result<(), String> { Err("macOS backend not supported on this platform".into()) }
    pub fn set_volume(&self, _volume_percent: u32) -> Result<(), String> { Err("macOS backend not supported on this platform".into()) }
    pub fn get_volume(&self) -> Option<u32> { None }
    pub fn is_playing(&self) -> bool { false }
    pub fn get_track_info(&self) -> TrackInfo { TrackInfo::default() }
}
