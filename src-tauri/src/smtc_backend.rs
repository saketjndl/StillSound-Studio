/// Windows SMTC (System Media Transport Controls) backend
/// Controls whatever app currently owns the OS media session — Spotify, YouTube Music, Apple Music, etc.
/// Zero setup, zero OAuth, works fully offline.
use crate::media_backend::TrackInfo;

// ─── Windows implementation ───

#[cfg(target_os = "windows")]
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

#[cfg(target_os = "windows")]
pub struct SmtcBackend;

#[cfg(target_os = "windows")]
impl SmtcBackend {
    pub fn new() -> Self {
        Self
    }

    fn get_session(&self) -> Result<GlobalSystemMediaTransportControlsSession, String> {
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .map_err(|e| format!("Failed to request SMTC manager: {}", e))?
            .get()
            .map_err(|e| format!("Failed to get SMTC manager: {}", e))?;

        if let Ok(sessions) = manager.GetSessions() {
            if let Ok(size) = sessions.Size() {
                // 1. Highest Priority: Any session currently PLAYING audio (and not a web browser unless browser is the only thing playing)
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            let is_browser = lower.contains("chrome")
                                || lower.contains("chromium")
                                || lower.contains("edge")
                                || lower.contains("msedge")
                                || lower.contains("firefox")
                                || lower.contains("opera")
                                || lower.contains("brave");
                            if !is_browser {
                                if let Ok(info) = session.GetPlaybackInfo() {
                                    if let Ok(status) = info.PlaybackStatus() {
                                        if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
                                            return Ok(session);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 2a. Second Priority (Primary Music Apps): Dedicated music player (Spotify, Apple Music, Tidal, etc.) with valid track Title
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if lower.contains("spotify")
                                || lower.contains("music")
                                || lower.contains("itunes")
                                || lower.contains("tidal")
                                || lower.contains("deezer")
                                || lower.contains("foobar")
                                || lower.contains("winamp")
                                || lower.contains("aimp")
                                || lower.contains("cider")
                            {
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

                // 2b. Second Priority (General Media Players): VLC, PotPlayer, etc. with valid track Title
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if lower.contains("vlc") || lower.contains("mpc") || lower.contains("potplayer") || lower.contains("mpv") {
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

                // 3a. Third Priority (Primary Music Apps): Even if paused/empty title
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if lower.contains("spotify")
                                || lower.contains("music")
                                || lower.contains("itunes")
                                || lower.contains("tidal")
                                || lower.contains("deezer")
                                || lower.contains("foobar")
                                || lower.contains("winamp")
                                || lower.contains("aimp")
                                || lower.contains("cider")
                            {
                                return Ok(session);
                            }
                        }
                    }
                }

                // 3b. Third Priority (General Media Players): Even if paused/empty title
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if lower.contains("vlc") || lower.contains("mpc") || lower.contains("potplayer") || lower.contains("mpv") {
                                return Ok(session);
                            }
                        }
                    }
                }

                // 4. Fourth Priority: Anything NOT a web browser (avoids controlling YouTube/Netflix in Chrome/Edge/Firefox)
                for i in 0..size {
                    if let Ok(session) = sessions.GetAt(i) {
                        if let Ok(app_id) = session.SourceAppUserModelId() {
                            let lower = app_id.to_string().to_lowercase();
                            if !lower.contains("chrome")
                                && !lower.contains("chromium")
                                && !lower.contains("edge")
                                && !lower.contains("msedge")
                                && !lower.contains("firefox")
                                && !lower.contains("opera")
                                && !lower.contains("brave")
                            {
                                return Ok(session);
                            }
                        }
                    }
                }
            }
        }

        manager
            .GetCurrentSession()
            .map_err(|e| format!("No active media session found: {}", e))
    }

    pub fn pause(&self) -> Result<(), String> {
        let session = self.get_session()?;
        let _ = session.TryPauseAsync().and_then(|op| op.get());
        
        // Spotify Windows desktop quirk fallback: if still playing after 150ms, send TogglePlayPause!
        std::thread::sleep(std::time::Duration::from_millis(150));
        if self.is_playing() {
            let _ = session.TryTogglePlayPauseAsync().and_then(|op| op.get());
        }
        println!("[SMTC] Paused media session");
        Ok(())
    }

    pub fn resume(&self) -> Result<(), String> {
        let session = self.get_session()?;
        let _ = session.TryPlayAsync().and_then(|op| op.get());
        
        // Spotify Windows desktop quirk fallback: if still paused after 150ms, send TogglePlayPause!
        std::thread::sleep(std::time::Duration::from_millis(150));
        if !self.is_playing() {
            let _ = session.TryTogglePlayPauseAsync().and_then(|op| op.get());
        }
        println!("[SMTC] Resumed media session");
        Ok(())
    }

    #[allow(dead_code)]
    pub fn toggle(&self) -> Result<(), String> {
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
        let session = self.get_session()?;
        if let Ok(timeline) = session.GetTimelineProperties() {
            if let Ok(pos) = timeline.Position() {
                let rewind_units = (seconds as i64) * 10_000_000;
                let current_units = pos.Duration;
                let target_units = (current_units - rewind_units).max(0);
                let _ = session.TryChangePlaybackPositionAsync(target_units).and_then(|op| op.get());
                println!("[SMTC] Smart Rewind: stepped back {}s to target pos {}ms", seconds, target_units / 10_000);
                return Ok(());
            }
        }
        Ok(())
    }

    pub fn set_volume(&self, volume_percent: u32) -> Result<(), String> {
        use windows::core::Interface;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        use windows::Win32::System::ProcessStatus::GetProcessImageFileNameA;
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
        use windows::Win32::Media::Audio::{
            MMDeviceEnumerator, IMMDeviceEnumerator, eRender, eConsole,
            IAudioSessionManager2, IAudioSessionControl2, ISimpleAudioVolume
        };
        use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;

        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: Result<IMMDeviceEnumerator, _> = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL);
            if let Ok(enumerator) = enumerator {
                if let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                    let scalar = (volume_percent as f32 / 100.0).clamp(0.0, 1.0);
                    let mut sessions_ducked = 0;

                    if let Ok(session_mgr) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                        if let Ok(session_enum) = session_mgr.GetSessionEnumerator() {
                            if let Ok(count) = session_enum.GetCount() {
                                for i in 0..count {
                                    if let Ok(control) = session_enum.GetSession(i) {
                                        let mut is_browser = false;
                                        let mut is_music_player = false;
                                        let mut proc_name = String::new();

                                        if let Ok(control2) = control.cast::<IAudioSessionControl2>() {
                                            if let Ok(pid) = control2.GetProcessId() {
                                                if pid > 4 {
                                                    if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                                                        let mut buffer = [0u8; 512];
                                                        let len = GetProcessImageFileNameA(handle, &mut buffer);
                                                        let _ = CloseHandle(handle);
                                                        if len > 0 {
                                                            if let Ok(name) = std::str::from_utf8(&buffer[..len as usize]) {
                                                                let name_lower = name.to_lowercase();
                                                                proc_name = name_lower.clone();
                                                                let browser_exes = ["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe", "opera.exe", "vivaldi.exe", "arc.exe", "zen.exe", "thorium.exe", "stillsound_studio.exe"];
                                                                let music_exes = ["spotify.exe", "vlc.exe", "music.ui.exe", "applemusic.exe", "tidal.exe", "foobar2000.exe", "wmplayer.exe", "aimp.exe", "cider.exe", "deezer.exe", "itunes.exe", "amazon music.exe"];
                                                                
                                                                is_browser = browser_exes.iter().any(|b| name_lower.ends_with(b));
                                                                is_music_player = music_exes.iter().any(|m| name_lower.ends_with(m));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Only duck audio sessions that are music players or non-browser sessions
                                        if !is_browser && (is_music_player || !proc_name.is_empty()) {
                                            if let Ok(simple_vol) = control.cast::<ISimpleAudioVolume>() {
                                                if simple_vol.SetMasterVolume(scalar, std::ptr::null()).is_ok() {
                                                    sessions_ducked += 1;
                                                    println!("[DUCK] Attenuated session '{}' to {}%", proc_name, volume_percent);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if sessions_ducked > 0 {
                        println!("[DUCK] Successfully set volume to {}% across {} media sessions", volume_percent, sessions_ducked);
                    } else {
                        // Fallback to endpoint master volume
                        if let Ok(endpoint_vol) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                            let _ = endpoint_vol.SetMasterVolumeLevelScalar(scalar, std::ptr::null());
                            println!("[DUCK] Fallback to endpoint master volume: {}%", volume_percent);
                        }
                    }
                }
            }
            CoUninitialize();
            Ok(())
        }
    }

    pub fn get_volume(&self) -> Option<u32> {
        use windows::core::Interface;
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
        use windows::Win32::Media::Audio::{
            MMDeviceEnumerator, IMMDeviceEnumerator, eRender, eConsole,
            IAudioSessionManager2, ISimpleAudioVolume
        };

        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: Result<IMMDeviceEnumerator, _> = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL);
            if let Ok(enumerator) = enumerator {
                if let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                    if let Ok(session_mgr) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                        if let Ok(session_enum) = session_mgr.GetSessionEnumerator() {
                            if let Ok(count) = session_enum.GetCount() {
                                for i in 0..count {
                                    if let Ok(control) = session_enum.GetSession(i) {
                                        if let Ok(simple_vol) = control.cast::<ISimpleAudioVolume>() {
                                            if let Ok(level) = simple_vol.GetMasterVolume() {
                                                CoUninitialize();
                                                return Some((level * 100.0).round() as u32);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            CoUninitialize();
            None
        }
    }

    pub fn is_playing(&self) -> bool {
        self.get_session()
            .and_then(|session| session.GetPlaybackInfo().map_err(|e| e.to_string()))
            .and_then(|info| info.PlaybackStatus().map_err(|e| e.to_string()))
            .map(|status| {
                status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
            })
            .unwrap_or(false)
    }

    pub fn get_track_info(&self) -> TrackInfo {
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let session = match self.get_session() {
            Ok(s) => s,
            Err(_) => return TrackInfo::default(),
        };

        let is_playing = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .map(|s| s == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
            .unwrap_or(false);

        let (track_name, artist_name, album_name, album_art) = match session
            .TryGetMediaPropertiesAsync()
            .and_then(|op| op.get())
        {
            Ok(props) => {
                let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
                let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
                let album = props
                    .AlbumTitle()
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let art = if let Ok(thumb_ref) = props.Thumbnail() {
                    if let Ok(stream) = thumb_ref.OpenReadAsync().and_then(|op| op.get()) {
                        if let Ok(size) = stream.Size() {
                            if size > 0 && size <= 5_000_000 {
                                let mut buffer = vec![0u8; size as usize];
                                if let Ok(reader) =
                                    windows::Storage::Streams::DataReader::CreateDataReader(&stream)
                                {
                                    if let Ok(_) =
                                        reader.LoadAsync(size as u32).and_then(|op| op.get())
                                    {
                                        if let Ok(_) = reader.ReadBytes(&mut buffer) {
                                            use base64::Engine;
                                            let b64 = base64::engine::general_purpose::STANDARD
                                                .encode(&buffer);
                                            let mime = if buffer
                                                .starts_with(&[0x89, 0x50, 0x4E, 0x47])
                                            {
                                                "image/png"
                                            } else if buffer.starts_with(&[0xFF, 0xD8, 0xFF]) {
                                                "image/jpeg"
                                            } else if buffer.starts_with(&[0x47, 0x49, 0x46]) {
                                                "image/gif"
                                            } else if buffer.starts_with(&[0x52, 0x49, 0x46, 0x46])
                                            {
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
                    }
                } else {
                    String::new()
                };

                (title, artist, album, art)
            }
            Err(_) => (String::new(), String::new(), String::new(), String::new()),
        };

        // Extract a friendly name from the source app ID
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
// Will be replaced by MPRIS (Linux) and macOS backend in Section 3

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
