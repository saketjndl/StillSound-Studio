/// macOS media session backend using AppleScript (osascript)
/// Controls Spotify or Apple Music on macOS without OAuth or external crates.

use crate::media_backend::TrackInfo;

#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn build_seek_backward_script(seconds: u32) -> String {
    format!(
        concat!(
            "if application \"Spotify\" is running then\n",
            "    tell application \"Spotify\"\n",
            "        try\n",
            "            set targetPos to (player position) - {sec}\n",
            "            if targetPos < 0 then set targetPos to 0\n",
            "            set player position to targetPos\n",
            "        end try\n",
            "    end tell\n",
            "else if application \"Music\" is running then\n",
            "    tell application \"Music\"\n",
            "        try\n",
            "            set targetPos to (player position) - {sec}\n",
            "            if targetPos < 0 then set targetPos to 0\n",
            "            set player position to targetPos\n",
            "        end try\n",
            "    end tell\n",
            "else\n",
            "    error \"No supported macOS media player is running\"\n",
            "end if"
        ),
        sec = seconds
    )
}

#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_macos_track_output(output: &str) -> TrackInfo {
    let parts: Vec<&str> = output.split("|||").collect();
    if parts.len() < 6 {
        return TrackInfo::default();
    }

    TrackInfo {
        is_playing: parts[0].trim().eq_ignore_ascii_case("playing"),
        track_name: parts[1].trim().to_string(),
        artist_name: parts[2].trim().to_string(),
        album_name: parts[3].trim().to_string(),
        album_art: parts[4].trim().to_string(),
        device_name: parts[5].trim().to_string(),
        device_type: "macOS".to_string(),
        volume_percent: None,
    }
}

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

    fn run_player_command(&self, spotify_cmd: &str, music_cmd: &str) -> Result<(), String> {
        let script = format!(
            concat!(
                "set acted to false\n",
                "if application \"Spotify\" is running then\n",
                "    tell application \"Spotify\" to {}\n",
                "    set acted to true\n",
                "else if application \"Music\" is running then\n",
                "    tell application \"Music\" to {}\n",
                "    set acted to true\n",
                "end if\n",
                "if not acted then error \"No supported macOS media player is running\""
            ),
            spotify_cmd, music_cmd
        );
        self.run_osascript(&script).map(|_| ())
    }

    pub fn pause(&self) -> Result<(), String> {
        self.run_osascript(concat!(
            "set acted to false\n",
            "if application \"Spotify\" is running then\n",
            "    tell application \"Spotify\"\n",
            "        if player state is playing then pause\n",
            "    end tell\n",
            "    set acted to true\n",
            "end if\n",
            "if application \"Music\" is running then\n",
            "    tell application \"Music\"\n",
            "        if player state is playing then pause\n",
            "    end tell\n",
            "    set acted to true\n",
            "end if\n",
            "if not acted then error \"No supported macOS media player is running\""
        ))?;
        println!("[macOS] Paused media session via AppleScript");
        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        self.run_player_command("play", "play")?;
        println!("[macOS] Resumed media session via AppleScript");
        Ok(())
    }

    #[allow(dead_code)]
    pub fn toggle(&self) -> Result<(), String> {
        self.run_player_command("playpause", "playpause")?;
        println!("[macOS] Toggled media session via AppleScript");
        Ok(())
    }

    pub fn skip_next(&self) -> Result<(), String> {
        self.run_player_command("next track", "next track")?;
        println!("[macOS] Skipped to next track via AppleScript");
        Ok(())
    }

    pub fn skip_prev(&self) -> Result<(), String> {
        self.run_player_command("previous track", "previous track")?;
        println!("[macOS] Skipped to previous track via AppleScript");
        Ok(())
    }

    pub fn seek_backward(&self, seconds: u32) -> Result<(), String> {
        let script = build_seek_backward_script(seconds);
        self.run_osascript(&script)?;
        println!("[macOS] Smart Rewind: stepped back {}s", seconds);
        Ok(())
    }

    pub fn set_volume(&self, volume_percent: u32) -> Result<(), String> {
        let vol = volume_percent.clamp(0, 100);
        let script = format!(
            concat!(
                "set acted to false\n",
                "if application \"Spotify\" is running then\n",
                "    tell application \"Spotify\"\n",
                "        if player state is playing then\n",
                "            set sound volume to {vol}\n",
                "            set acted to true\n",
                "        end if\n",
                "    end tell\n",
                "end if\n",
                "if (not acted) and (application \"Music\" is running) then\n",
                "    tell application \"Music\"\n",
                "        if player state is playing then\n",
                "            set sound volume to {vol}\n",
                "            set acted to true\n",
                "        end if\n",
                "    end tell\n",
                "end if\n",
                "if (not acted) and (application \"Spotify\" is running) then\n",
                "    tell application \"Spotify\" to set sound volume to {vol}\n",
                "    set acted to true\n",
                "else if (not acted) and (application \"Music\" is running) then\n",
                "    tell application \"Music\" to set sound volume to {vol}\n",
                "    set acted to true\n",
                "end if\n",
                "if not acted then error \"No supported macOS media player is running\""
            ),
            vol = vol
        );
        self.run_osascript(&script)?;
        println!("[macOS] Set volume to {}% via AppleScript", vol);
        Ok(())
    }

    pub fn get_volume(&self) -> Option<u32> {
        // AppleScript sound volume is 0–100 on Spotify/Music; prefer whichever is actively playing.
        let script = concat!(
            "if application \"Spotify\" is running then\n",
            "    tell application \"Spotify\"\n",
            "        if player state is playing then return sound volume\n",
            "    end tell\n",
            "end if\n",
            "if application \"Music\" is running then\n",
            "    tell application \"Music\"\n",
            "        if player state is playing then return sound volume\n",
            "    end tell\n",
            "end if\n",
            "if application \"Spotify\" is running then return (sound volume of application \"Spotify\")\n",
            "if application \"Music\" is running then return (sound volume of application \"Music\")\n",
            "return \"\""
        );
        self.run_osascript(script)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    pub fn is_playing(&self) -> bool {
        let script = concat!(
            "if application \"Spotify\" is running then\n",
            "    tell application \"Spotify\"\n",
            "        if player state is playing then return \"playing\"\n",
            "    end tell\n",
            "end if\n",
            "if application \"Music\" is running then\n",
            "    tell application \"Music\"\n",
            "        if player state is playing then return \"playing\"\n",
            "    end tell\n",
            "end if\n",
            "return \"stopped\""
        );
        self.run_osascript(script)
            .map(|state| state.eq_ignore_ascii_case("playing"))
            .unwrap_or(false)
    }

    pub fn get_track_info(&self) -> TrackInfo {
        let script = r#"
        on readSpotify()
            tell application "Spotify"
                set pState to (player state as string)
                set trackName to ""
                set artistName to ""
                set albumName to ""
                set artUrl to ""
                try
                    set trackName to (name of current track as string)
                    set artistName to (artist of current track as string)
                    set albumName to (album of current track as string)
                end try
                try
                    set artUrl to (artwork url of current track as string)
                end try
                return pState & "|||" & trackName & "|||" & artistName & "|||" & albumName & "|||" & artUrl & "|||Spotify"
            end tell
        end readSpotify

        on readMusic()
            tell application "Music"
                set pState to (player state as string)
                set trackName to ""
                set artistName to ""
                set albumName to ""
                try
                    set trackName to (name of current track as string)
                    set artistName to (artist of current track as string)
                    set albumName to (album of current track as string)
                end try
                return pState & "|||" & trackName & "|||" & artistName & "|||" & albumName & "||||||Apple Music"
            end tell
        end readMusic

        if application "Spotify" is running then
            tell application "Spotify"
                if player state is playing then return my readSpotify()
            end tell
        end if
        if application "Music" is running then
            tell application "Music"
                if player state is playing then return my readMusic()
            end tell
        end if
        if application "Spotify" is running then return my readSpotify()
        if application "Music" is running then return my readMusic()
        return "stopped|||||||||||||||"
        "#;

        let output = self.run_osascript(script).unwrap_or_default();
        parse_macos_track_output(&output)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_backward_script_clamps_negative_position() {
        let script = build_seek_backward_script(5);
        assert!(script.contains("if targetPos < 0 then set targetPos to 0"));
        assert!(script.contains("- 5"));
    }

    #[test]
    fn parse_track_output_handles_playing_and_stopped() {
        let playing = parse_macos_track_output("playing|||Song|||Artist|||Album|||https://img|||Spotify");
        assert!(playing.is_playing);
        assert_eq!(playing.track_name, "Song");
        assert_eq!(playing.device_name, "Spotify");
        assert_eq!(playing.album_art, "https://img");

        let stopped = parse_macos_track_output("stopped|||||||||||||||");
        assert!(!stopped.is_playing);
        assert!(stopped.track_name.is_empty());
    }
}
