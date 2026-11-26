/// Windows SMTC (System Media Transport Controls) backend
/// Controls whatever app currently owns the OS media session — Spotify, YouTube Music, Apple Music, etc.
/// Zero setup, zero OAuth, works fully offline.
use crate::media_backend::TrackInfo;

// ─── Windows implementation ───

#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "windows")]
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

#[cfg(target_os = "windows")]
static LAST_ART_CACHE: OnceLock<Mutex<(String, u64, String)>> = OnceLock::new();

#[cfg(target_os = "windows")]
struct ComGuard(bool);

#[cfg(target_os = "windows")]
impl ComGuard {
    fn init() -> Self {
        use windows::Win32::Foundation::{S_FALSE, S_OK};
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self(hr == S_OK || hr == S_FALSE)
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                windows::Win32::System::Com::CoUninitialize();
            }
        }
    }
}

#[cfg(any(target_os = "windows", test))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn is_browser_id(app_id: &str) -> bool {
    let lower = app_id.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    const SUBSTRING_BROWSERS: &[&str] = &[
        "chrome",
        "chromium",
        "msedge",
        "firefox",
        "mozilla",
        "opera",
        "brave",
        "vivaldi",
        "thorium",
        "floorp",
        "librewolf",
        "waterfox",
        "yandex",
        "duckduckgo",
        "sidekick",
        "wavebox",
        "palemoon",
        "stillsound_studio",
    ];
    if SUBSTRING_BROWSERS.iter().any(|b| lower.contains(b)) {
        return true;
    }
    // Mozilla Firefox / Gecko Windows installs use 16-char uppercase hex AppUserModelID hashes (e.g. 308046B0AF4A39CB)
    if lower.len() == 16 && lower.bytes().all(|b| b.is_ascii_hexdigit()) {
        return true;
    }
    // Token-aware checks to avoid false matches on unrelated words (e.g. "search", "knowledge")
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| matches!(tok, "edge" | "arc" | "zen" | "tor" | "comet"))
}

#[cfg(target_os = "windows")]
fn is_primary_music_id(lower: &str) -> bool {
    const PRIMARY: &[&str] = &[
        "spotify",
        "music",
        "itunes",
        "tidal",
        "deezer",
        "foobar",
        "winamp",
        "aimp",
        "cider",
        "qobuz",
        "musicbee",
    ];
    !is_browser_id(lower) && PRIMARY.iter().any(|p| lower.contains(p))
}

#[cfg(target_os = "windows")]
fn is_general_media_id(lower: &str) -> bool {
    const GENERAL: &[&str] = &["vlc", "mpc", "potplayer", "mpv"];
    !is_browser_id(lower) && GENERAL.iter().any(|p| lower.contains(p))
}

#[cfg(target_os = "windows")]
fn is_music_player_exe(name_lower: &str) -> bool {
    const MUSIC_EXES: &[&str] = &[
        "spotify.exe",
        "vlc.exe",
        "music.ui.exe",
        "applemusic.exe",
        "tidal.exe",
        "foobar2000.exe",
        "wmplayer.exe",
        "aimp.exe",
        "cider.exe",
        "deezer.exe",
        "itunes.exe",
        "amazon music.exe",
        "winamp.exe",
        "qobuz.exe",
        "musicbee.exe",
        "mpc-hc64.exe",
        "mpc-hc.exe",
        "potplayermini64.exe",
        "mpv.exe",
    ];
    MUSIC_EXES.iter().any(|m| name_lower.ends_with(m))
}

#[cfg(target_os = "windows")]
pub struct SmtcBackend;

#[cfg(target_os = "windows")]
impl SmtcBackend {
    pub fn new() -> Self {
        Self
    }

    fn get_session(&self) -> Result<GlobalSystemMediaTransportControlsSession, String> {
        let _com = ComGuard::init();
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .map_err(|e| format!("Failed to request SMTC manager: {}", e))?
            .get()
            .map_err(|e| format!("Failed to get SMTC manager: {}", e))?;

        if let Ok(sessions) = manager.GetSessions() {
            if let Ok(size) = sessions.Size() {
                // 1a. Highest Priority: Primary Music Apps (Spotify, Apple Music, etc.) currently PLAYING
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if is_primary_music_id(&lower) && Self::session_is_playing(&session) {
                                return Ok(session);
                            }
                        }
                    }
                }

                // 1b. General Media Players (VLC, mpv, MPC, etc.) currently PLAYING
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if is_general_media_id(&lower) && Self::session_is_playing(&session) {
                                return Ok(session);
                            }
                        }
                    }
                }

                // 2a. Primary Music Apps with a loaded track title (even if paused, stay bound to Spotify rather than hijacking unknown sessions)
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if is_primary_music_id(&lower) {
                                if let Ok(props_op) = session.TryGetMediaPropertiesAsync() {
                                    if let Ok(props) = props_op.get() {
                                        if let Ok(title) = props.Title() {
                                            if !title.is_empty() {
                                                return Ok(session);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 2b. General Media Players with a loaded track title
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if is_general_media_id(&lower) {
                                if let Ok(props_op) = session.TryGetMediaPropertiesAsync() {
                                    if let Ok(props) = props_op.get() {
                                        if let Ok(title) = props.Title() {
                                            if !title.is_empty() {
                                                return Ok(session);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 3a. Primary Music Apps even if paused/empty title
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if is_primary_music_id(&lower) {
                                return Ok(session);
                            }
                        }
                    }
                }

                // 3b. General Media Players even if paused/empty title
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if is_general_media_id(&lower) {
                                return Ok(session);
                            }
                        }
                    }
                }

                // 4a. Any other non-browser session currently PLAYING audio
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if !is_browser_id(&lower) && Self::session_is_playing(&session) {
                                return Ok(session);
                            }
                        }
                    }
                }

                // 4b. Anything NOT a web browser
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if !is_browser_id(&lower) {
                                return Ok(session);
                            }
                        }
                    }
                }
            }
        }

        // Never fall back to browser SMTC sessions (otherwise pausing audio pauses the user's video tab in a loop)
        if let Ok(current) = manager.GetCurrentSession() {
            if let Ok(app_id) = current.SourceAppUserModelId() {
                let lower = app_id.to_string().to_lowercase();
                if !is_browser_id(&lower) {
                    return Ok(current);
                }
            }
        }

        Err("No standalone desktop media player session found (ignoring browser tabs)".to_string())
    }

    fn session_is_playing(session: &GlobalSystemMediaTransportControlsSession) -> bool {
        session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .map(|s| s == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
            .unwrap_or(false)
    }

    pub fn pause(&self) -> Result<(), String> {
        let _com = ComGuard::init();
        let session = match self.get_session() {
            Ok(s) => s,
            Err(e) => {
                println!("[SMTC] Pause skipped: {} — open Spotify Desktop (with desktop media overlay enabled) or connect Spotify API for web players.", e);
                return Err(e);
            }
        };
        let app_id = session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_else(|_| "unknown".into());
        let pause_ok = session.TryPauseAsync().and_then(|op| op.get()).unwrap_or(false);

        // Give slow/waking desktop players (e.g., Spotify Electron) up to ~320ms to reflect state
        // before attempting a toggle fallback, preventing inverted play/pause commands.
        for wait_ms in [120u64, 200u64] {
            std::thread::sleep(std::time::Duration::from_millis(wait_ms));
            if !Self::session_is_playing(&session) {
                println!("[SMTC] Paused media session ({})", app_id);
                return Ok(());
            }
        }

        if !pause_ok && Self::session_is_playing(&session) {
            let _ = session.TryTogglePlayPauseAsync().and_then(|op| op.get());
        }
        println!("[SMTC] Paused media session ({})", app_id);
        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        let _com = ComGuard::init();
        let session = self.get_session()?;
        let app_id = session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_else(|_| "unknown".into());
        let play_ok = session.TryPlayAsync().and_then(|op| op.get()).unwrap_or(false);

        for wait_ms in [120u64, 200u64] {
            std::thread::sleep(std::time::Duration::from_millis(wait_ms));
            if Self::session_is_playing(&session) {
                println!("[SMTC] Resumed media session ({})", app_id);
                return Ok(());
            }
        }

        if !play_ok && !Self::session_is_playing(&session) {
            let _ = session.TryTogglePlayPauseAsync().and_then(|op| op.get());
        }
        println!("[SMTC] Resumed media session ({})", app_id);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn toggle(&self) -> Result<(), String> {
        let _com = ComGuard::init();
        let session = self.get_session()?;
        session
            .TryTogglePlayPauseAsync()
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;
        println!("[SMTC] Toggled media session");
        Ok(())
    }

    pub fn skip_next(&self) -> Result<(), String> {
        let _com = ComGuard::init();
        let session = self.get_session()?;
        session
            .TrySkipNextAsync()
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;
        println!("[SMTC] Skipped to next track");
        Ok(())
    }

    pub fn skip_prev(&self) -> Result<(), String> {
        let _com = ComGuard::init();
        let session = self.get_session()?;
        session
            .TrySkipPreviousAsync()
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;
        println!("[SMTC] Skipped to previous track");
        Ok(())
    }

    pub fn seek_backward(&self, seconds: u32) -> Result<(), String> {
        let _com = ComGuard::init();
        let session = self.get_session()?;
        let timeline = session
            .GetTimelineProperties()
            .map_err(|e| format!("Timeline unavailable: {}", e))?;
        let pos = timeline
            .Position()
            .map_err(|e| format!("Position unavailable: {}", e))?;

        let rewind_units = (seconds as i64) * 10_000_000;
        let current_units = pos.Duration;
        let target_units = (current_units - rewind_units).max(0);
        let ok = session
            .TryChangePlaybackPositionAsync(target_units)
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;

        if !ok {
            return Err("Active media session does not support position seeking".into());
        }
        println!(
            "[SMTC] Smart Rewind: stepped back {}s to target pos {}ms",
            seconds,
            target_units / 10_000
        );
        Ok(())
    }

    pub fn set_volume(&self, volume_percent: u32) -> Result<(), String> {
        use windows::core::Interface;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Media::Audio::{
            eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
            ISimpleAudioVolume, MMDeviceEnumerator,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        use windows::Win32::System::ProcessStatus::GetProcessImageFileNameA;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

        let _com = ComGuard::init();
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create MMDeviceEnumerator: {}", e))?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| format!("No default audio render endpoint: {}", e))?;

            let scalar = (volume_percent as f32 / 100.0).clamp(0.0, 1.0);
            let mut sessions_ducked = 0;

            if let Ok(session_mgr) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                if let Ok(session_enum) = session_mgr.GetSessionEnumerator() {
                    if let Ok(count) = session_enum.GetCount() {
                        for i in 0..count {
                            if let Ok(control) = session_enum.GetSession(i) {
                                if let Ok(control2) = control.cast::<IAudioSessionControl2>() {
                                    if let Ok(pid) = control2.GetProcessId() {
                                        if pid > 4 {
                                            if let Ok(handle) =
                                                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                                            {
                                                let mut buffer = [0u8; 512];
                                                let len = GetProcessImageFileNameA(handle, &mut buffer);
                                                let _ = CloseHandle(handle);
                                                if len > 0 {
                                                    if let Ok(name) = std::str::from_utf8(&buffer[..len as usize]) {
                                                        let name_lower = name.to_lowercase();
                                                        if is_music_player_exe(&name_lower) {
                                                            if let Ok(simple_vol) =
                                                                control.cast::<ISimpleAudioVolume>()
                                                            {
                                                                if simple_vol
                                                                    .SetMasterVolume(scalar, std::ptr::null())
                                                                    .is_ok()
                                                                {
                                                                    sessions_ducked += 1;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if sessions_ducked > 0 {
                println!(
                    "[DUCK] Set volume to {}% across {} music player session(s)",
                    volume_percent, sessions_ducked
                );
            }
            Ok(())
        }
    }

    pub fn get_volume(&self) -> Option<u32> {
        use windows::core::Interface;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Media::Audio::{
            eConsole, eRender, AudioSessionStateActive, IAudioSessionControl2,
            IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        use windows::Win32::System::ProcessStatus::GetProcessImageFileNameA;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

        let _com = ComGuard::init();
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let session_mgr = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None).ok()?;
            let session_enum = session_mgr.GetSessionEnumerator().ok()?;
            let count = session_enum.GetCount().ok()?;

            let mut fallback_vol: Option<u32> = None;
            for i in 0..count {
                if let Ok(control) = session_enum.GetSession(i) {
                    if let Ok(control2) = control.cast::<IAudioSessionControl2>() {
                        if let Ok(pid) = control2.GetProcessId() {
                            if pid > 4 {
                                if let Ok(handle) =
                                    OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                                {
                                    let mut buffer = [0u8; 512];
                                    let len = GetProcessImageFileNameA(handle, &mut buffer);
                                    let _ = CloseHandle(handle);
                                    if len > 0 {
                                        if let Ok(name) = std::str::from_utf8(&buffer[..len as usize]) {
                                            let name_lower = name.to_lowercase();
                                            if is_music_player_exe(&name_lower) {
                                                if let Ok(simple_vol) = control.cast::<ISimpleAudioVolume>() {
                                                    if let Ok(level) = simple_vol.GetMasterVolume() {
                                                        let pct = (level * 100.0).round() as u32;
                                                        let is_active = control
                                                            .GetState()
                                                            .map(|st| st == AudioSessionStateActive)
                                                            .unwrap_or(false);
                                                        if is_active {
                                                            return Some(pct);
                                                        }
                                                        if fallback_vol.is_none() {
                                                            fallback_vol = Some(pct);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            fallback_vol
        }
    }

    pub fn is_playing(&self) -> bool {
        let _com = ComGuard::init();
        self.get_session()
            .map(|session| Self::session_is_playing(&session))
            .unwrap_or(false)
    }

    pub fn get_track_info(&self) -> TrackInfo {
        let _com = ComGuard::init();
        let session = match self.get_session() {
            Ok(s) => s,
            Err(_) => return TrackInfo::default(),
        };

        let is_playing = Self::session_is_playing(&session);

        let (track_name, artist_name, album_name, album_art) = match session
            .TryGetMediaPropertiesAsync()
            .and_then(|op| op.get())
        {
            Ok(props) => {
                let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
                let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
                let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
                let cache_key = format!("{}|{}|{}", title, artist, album);

                let art = if let Ok(thumb_ref) = props.Thumbnail() {
                    if let Ok(stream) = thumb_ref.OpenReadAsync().and_then(|op| op.get()) {
                        if let Ok(size) = stream.Size() {
                            if size > 0 && size <= 5_000_000 {
                                let cache = LAST_ART_CACHE
                                    .get_or_init(|| Mutex::new((String::new(), 0, String::new())));
                                let cached = {
                                    let guard = cache.lock().unwrap();
                                    if guard.0 == cache_key && guard.1 == size && !guard.2.is_empty() {
                                        Some(guard.2.clone())
                                    } else {
                                        None
                                    }
                                };

                                if let Some(hit) = cached {
                                    hit
                                } else {
                                    let mut buffer = vec![0u8; size as usize];
                                    let encoded = if let Ok(reader) =
                                        windows::Storage::Streams::DataReader::CreateDataReader(&stream)
                                    {
                                        if reader.LoadAsync(size as u32).and_then(|op| op.get()).is_ok()
                                            && reader.ReadBytes(&mut buffer).is_ok()
                                        {
                                            use base64::Engine;
                                            let b64 = base64::engine::general_purpose::STANDARD
                                                .encode(&buffer);
                                            let mime = if buffer.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                                                "image/png"
                                            } else if buffer.starts_with(&[0xFF, 0xD8, 0xFF]) {
                                                "image/jpeg"
                                            } else if buffer.starts_with(&[0x47, 0x49, 0x46]) {
                                                "image/gif"
                                            } else if buffer.starts_with(&[0x52, 0x49, 0x46, 0x46]) {
                                                "image/webp"
                                            } else {
                                                "image/jpeg"
                                            };
                                            format!("data:{};base64,{}", mime, b64)
                                        } else {
                                            String::new()
                                        }
                                    } else {
                                        String::new()
                                    };

                                    if !encoded.is_empty() {
                                        let mut guard = cache.lock().unwrap();
                                        *guard = (cache_key, size, encoded.clone());
                                    }
                                    encoded
                                }
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                (title, artist, album, art)
            }
            Err(_) => (String::new(), String::new(), String::new(), String::new()),
        };

        let source_app = session
            .SourceAppUserModelId()
            .map(|s| {
                let app_id = s.to_string();
                let lower = app_id.to_lowercase();
                if lower.contains("spotify") {
                    "Spotify".to_string()
                } else if lower.contains("chrome") || lower.contains("chromium") {
                    "Chrome".to_string()
                } else if lower.contains("firefox") {
                    "Firefox".to_string()
                } else if lower.contains("music") {
                    "Music App".to_string()
                } else {
                    app_id
                        .split('\\')
                        .last()
                        .or_else(|| app_id.split('!').next())
                        .unwrap_or(&app_id)
                        .replace(".exe", "")
                        .to_string()
                }
            })
            .unwrap_or_else(|_| "Media Player".to_string());

        TrackInfo {
            is_playing,
            track_name,
            artist_name,
            album_art,
            album_name,
            device_name: source_app,
            device_type: "Local".to_string(),
            volume_percent: None,
        }
    }
}

// ─── Stub for non-Windows platforms ───

#[cfg(not(target_os = "windows"))]
pub struct SmtcBackend;

#[cfg(not(target_os = "windows"))]
impl SmtcBackend {
    pub fn new() -> Self {
        Self
    }
    pub fn pause(&self) -> Result<(), String> {
        Err("Universal mode not yet supported on this platform".into())
    }
    pub fn resume(&self) -> Result<(), String> {
        Err("Universal mode not yet supported on this platform".into())
    }
    pub fn toggle(&self) -> Result<(), String> {
        Err("Universal mode not yet supported on this platform".into())
    }
    pub fn skip_next(&self) -> Result<(), String> {
        Err("Universal mode not yet supported on this platform".into())
    }
    pub fn skip_prev(&self) -> Result<(), String> {
        Err("Universal mode not yet supported on this platform".into())
    }
    pub fn seek_backward(&self, _seconds: u32) -> Result<(), String> {
        Err("Universal mode not yet supported on this platform".into())
    }
    pub fn set_volume(&self, _volume_percent: u32) -> Result<(), String> {
        Err("Universal mode not yet supported on this platform".into())
    }
    pub fn get_volume(&self) -> Option<u32> {
        None
    }
    pub fn is_playing(&self) -> bool {
        false
    }
    pub fn get_track_info(&self) -> TrackInfo {
        TrackInfo::default()
    }
}

#[cfg(test)]
mod tests {
    use super::is_browser_id;

    #[test]
    fn browser_id_filter_blocks_browsers_and_preserves_music_apps() {
        assert!(is_browser_id("brave.exe"));
        assert!(is_browser_id("BraveSoftware.Brave-Browser"));
        assert!(is_browser_id("chrome.exe"));
        assert!(is_browser_id("firefox.exe"));
        assert!(is_browser_id("308046B0AF4A39CB"));
        assert!(is_browser_id("E7CF176E110C211B"));
        assert!(is_browser_id("MSEdge.exe"));
        assert!(is_browser_id("TheBrowserCompany.Arc"));
        assert!(is_browser_id("zen.exe"));
        assert!(is_browser_id("stillsound_studio.exe"));

        assert!(!is_browser_id("Spotify.exe"));
        assert!(!is_browser_id("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"));
        assert!(!is_browser_id("AppleInc.AppleMusicWin_njac!App"));
        assert!(!is_browser_id("vlc.exe"));
        assert!(!is_browser_id("MusicSearchApp.exe"));
    }
}
