pub mod media_backend;
mod smtc_backend;
mod mpris_backend;
mod macos_backend;
mod mic_monitor;

pub use media_backend::{SyncMode, SiteAction};

#[cfg(target_os = "windows")]
use smtc_backend::SmtcBackend as ActiveBackend;
#[cfg(target_os = "linux")]
use mpris_backend::MprisBackend as ActiveBackend;
#[cfg(target_os = "macos")]
use macos_backend::MacosBackend as ActiveBackend;
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
use smtc_backend::SmtcBackend as ActiveBackend;

use tauri::{AppHandle, Runtime, Emitter, Manager};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::menu::{Menu, MenuItem};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::accept_async;
use tokio::net::TcpListener;
use reqwest::Client;
use std::collections::HashMap;
use rand::RngCore;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Bundled Spotify Client ID — safe to embed (PKCE, no client secret).
// Override at build time: STILLSOUND_CLIENT_ID=xxx cargo build
const DEFAULT_CLIENT_ID: &str = match option_env!("STILLSOUND_CLIENT_ID") {
    Some(id) => id,
    None => "PLACEHOLDER_STILLSOUND_CLIENT_ID",
};

// --- State Models ---

#[derive(Serialize, Deserialize, Clone)]
pub struct AppState {
    pub client_id: Option<String>,
    pub code_verifier: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub app_paused_spotify: bool,
    pub manual_pause_detected: bool,
    pub sync_enabled: bool,
    pub autostart: bool,
    pub autostart_minimized: bool,
    pub minimize_to_tray: bool,
    pub volume: u32,
    pub spotify_ready: bool,
    pub yt_playing: bool,
    pub current_device_id: Option<String>,
    #[serde(default)]
    pub sync_mode: SyncMode,
    #[serde(default)]
    pub site_actions: HashMap<String, SiteAction>,
    #[serde(default = "default_ducking_level")]
    pub ducking_level: u32,
    #[serde(default)]
    pub is_ducked: bool,
    #[serde(default)]
    pub pre_duck_volume: Option<u32>,
    #[serde(default = "default_true")]
    pub smart_rewind_enabled: bool,
    #[serde(default = "default_rewind_seconds")]
    pub smart_rewind_seconds: u32,
    #[serde(default = "default_rewind_threshold")]
    pub smart_rewind_threshold_secs: u32,
    #[serde(default)]
    pub pause_timestamp: Option<u64>,
    #[serde(default = "default_true")]
    pub mic_aware_enabled: bool,
    #[serde(default = "default_mic_action")]
    pub mic_action: SiteAction,
    #[serde(default)]
    pub mic_active: bool,
    #[serde(default)]
    pub mic_paused_or_ducked: bool,
    #[serde(default)]
    pub mini_hud_enabled: bool,
    #[serde(default)]
    pub mini_hud_pos: Option<(i32, i32)>,
    #[serde(default)]
    pub focus_minutes_this_week: u32,
    #[serde(default)]
    pub music_minutes_this_week: u32,
    #[serde(default)]
    pub auto_pause_count: u32,
    #[serde(default)]
    pub auth_failure_count: u32,
    // Legacy support for backwards compatibility
    #[serde(default)]
    pub enabled_sites: HashMap<String, bool>,
    #[serde(skip)]
    pub active_bridges: HashMap<String, (bool, SiteAction)>,
}

fn default_true() -> bool { true }
fn default_ducking_level() -> u32 { 20 }
fn default_rewind_seconds() -> u32 { 5 }
fn default_rewind_threshold() -> u32 { 30 }
fn default_mic_action() -> SiteAction { SiteAction::Duck }

impl Default for AppState {
    fn default() -> Self {
        Self {
            sync_enabled: true,
            autostart: false,
            autostart_minimized: false,
            minimize_to_tray: false,
            volume: 50,
            spotify_ready: false,
            yt_playing: false,
            current_device_id: None,
            sync_mode: SyncMode::Universal,
            client_id: None,
            code_verifier: None,
            access_token: None,
            refresh_token: None,
            app_paused_spotify: false,
            manual_pause_detected: false,
            focus_minutes_this_week: 0,
            music_minutes_this_week: 0,
            auto_pause_count: 0,
            auth_failure_count: 0,
            ducking_level: 20,
            is_ducked: false,
            pre_duck_volume: None,
            smart_rewind_enabled: true,
            smart_rewind_seconds: 5,
            smart_rewind_threshold_secs: 30,
            pause_timestamp: None,
            mic_aware_enabled: true,
            mic_action: SiteAction::Duck,
            mic_active: false,
            mic_paused_or_ducked: false,
            mini_hud_enabled: false,
            mini_hud_pos: None,
            site_actions: {
                let mut map = HashMap::new();
                // Streaming -> Pause
                for s in &["youtube.com", "netflix.com", "primevideo.com", "hotstar.com", "disneyplus.com", "twitch.tv", "crunchyroll.com", "vimeo.com", "jiocinema.com"] {
                    map.insert(s.to_string(), SiteAction::Pause);
                }
                // Social Media -> Duck
                for s in &["twitter.com", "x.com", "reddit.com", "instagram.com", "facebook.com", "tiktok.com"] {
                    map.insert(s.to_string(), SiteAction::Duck);
                }
                // Educational -> Pause
                for s in &["udemy.com", "coursera.org", "khanacademy.org", "nptel.ac.in"] {
                    map.insert(s.to_string(), SiteAction::Pause);
                }
                // Conferencing -> Pause
                for s in &["meet.google.com", "zoom.us", "teams.microsoft.com"] {
                    map.insert(s.to_string(), SiteAction::Pause);
                }
                map
            },
            enabled_sites: HashMap::new(),
            active_bridges: HashMap::new(),
        }
    }
}

pub struct StateWrapper(pub Arc<Mutex<AppState>>);

impl AppState {
    fn get_save_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let mut path = PathBuf::from(appdata);
            path.push("StillSound");
            fs::create_dir_all(&path).ok();
            path.push("settings.json");
            path
        } else {
            let mut path = std::env::current_dir().unwrap_or_default();
            path.push("stillsound_settings.json");
            path
        }
    }

    pub fn load() -> Self {
        let path = Self::get_save_path();
        if let Ok(content) = fs::read_to_string(path) {
            let mut state: Self = serde_json::from_str(&content).unwrap_or_default();
            state.app_paused_spotify = false;
            state.manual_pause_detected = false;
            state.is_ducked = false;
            state.pre_duck_volume = None;
            state.pause_timestamp = None;
            state.mic_active = false;
            state.mic_paused_or_ducked = false;

            // Migrate legacy enabled_sites to site_actions if needed
            if state.site_actions.is_empty() && !state.enabled_sites.is_empty() {
                println!("[MIGRATION] Migrating legacy site toggles to site_actions matrix...");
                let social_sites = ["twitter.com", "x.com", "reddit.com", "instagram.com", "facebook.com", "tiktok.com"];
                for (site, enabled) in &state.enabled_sites {
                    if !enabled {
                        state.site_actions.insert(site.clone(), SiteAction::Ignore);
                    } else if social_sites.iter().any(|s| site.contains(s)) {
                        state.site_actions.insert(site.clone(), SiteAction::Duck);
                    } else {
                        state.site_actions.insert(site.clone(), SiteAction::Pause);
                    }
                }
            }

            // Ensure all known default sites exist in site_actions
            let default_state = Self::default();
            for (site, action) in default_state.site_actions {
                state.site_actions.entry(site).or_insert(action);
            }

            state
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = Self::get_save_path();
        if let Ok(content) = serde_json::to_string_pretty(self) {
            fs::write(path, content).ok();
        }
    }
}

// --- Volume Ducking & Smooth Ramping Engine ---

async fn smooth_duck_volume(state: &Arc<Mutex<AppState>>, target_vol: u32) {
    let sync_mode = {
        state.lock().unwrap().sync_mode.clone()
    };

    let universal_vol = if sync_mode == SyncMode::Universal {
        tokio::task::spawn_blocking(|| {
            ActiveBackend::new().get_volume()
        }).await.unwrap_or(None)
    } else {
        None
    };

    let (current_vol, device_id) = {
        let mut s = state.lock().unwrap();
        if s.is_ducked {
            return; // Already ducked
        }
        let cur = universal_vol.unwrap_or(s.volume).max(75);
        s.volume = cur;
        s.pre_duck_volume = Some(cur);
        s.is_ducked = true;
        (cur, s.current_device_id.clone())
    };

    println!("[DUCK] Smooth ducking volume: {}% -> {}% (mode: {})", current_vol, target_vol, sync_mode);

    let steps = 4;
    let diff = current_vol.saturating_sub(target_vol);
    for i in 1..=steps {
        let step_vol = current_vol.saturating_sub(diff * i / steps).max(target_vol);
        match sync_mode {
            SyncMode::Spotify => {
                let _ = spotify_set_volume(state, step_vol, device_id.clone()).await;
            }
            SyncMode::Universal => {
                let _ = tokio::task::spawn_blocking(move || {
                    ActiveBackend::new().set_volume(step_vol)
                }).await;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
    }
}

async fn smooth_unduck_volume(state: &Arc<Mutex<AppState>>) {
    let (target_vol, current_duck_vol, device_id, sync_mode) = {
        let mut s = state.lock().unwrap();
        if !s.is_ducked {
            return;
        }
        s.is_ducked = false;
        let pre = s.pre_duck_volume.unwrap_or(s.volume).max(75);
        s.pre_duck_volume = None;
        (pre, s.ducking_level, s.current_device_id.clone(), s.sync_mode.clone())
    };

    println!("[DUCK] Smooth restoring volume: {}% -> {}% (mode: {})", current_duck_vol, target_vol, sync_mode);
    let steps = 4;
    let start_vol = current_duck_vol.min(target_vol);
    let diff = target_vol.saturating_sub(start_vol);
    for i in 1..=steps {
        let step_vol = start_vol + (diff * i / steps);
        match sync_mode {
            SyncMode::Spotify => {
                let _ = spotify_set_volume(state, step_vol, device_id.clone()).await;
            }
            SyncMode::Universal => {
                let _ = tokio::task::spawn_blocking(move || {
                    ActiveBackend::new().set_volume(step_vol)
                }).await;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
    }
}

// --- Smart Rewind Engine ---

async fn media_seek_backward(state: &Arc<Mutex<AppState>>, seconds: u32) -> Result<(), String> {
    let (mode, device_id) = {
        let s = state.lock().unwrap();
        (s.sync_mode.clone(), s.current_device_id.clone())
    };

    match mode {
        SyncMode::Universal => {
            tokio::task::spawn_blocking(move || {
                let backend = ActiveBackend::new();
                backend.seek_backward(seconds)
            }).await.map_err(|e| e.to_string())?
        }
        SyncMode::Spotify => {
            if let Some(playback) = spotify_get_playback_state(state).await {
                if let Some(progress_ms) = playback["progress_ms"].as_u64() {
                    let rewind_ms = (seconds as u64) * 1000;
                    let target_ms = progress_ms.saturating_sub(rewind_ms);
                    let _ = spotify_seek(state, target_ms, device_id).await;
                    println!("[STILLSOUND] Spotify Smart Rewind: stepped back {}s to {}ms", seconds, target_ms);
                }
            }
            Ok(())
        }
    }
}

async fn spotify_seek(state: &Arc<Mutex<AppState>>, position_ms: u64, device_id: Option<String>) -> Result<(), String> {
    let token = state.lock().unwrap().access_token.clone().ok_or("No token")?;
    let client = Client::new();
    let mut url = format!("https://api.spotify.com/v1/me/player/seek?position_ms={}", position_ms);
    if let Some(ref id) = device_id {
        url = format!("{}&device_id={}", url, id);
    }
    let res = client.put(&url)
        .bearer_auth(&token)
        .header("Content-Length", "0")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Ok(new_at) = spotify_refresh_token(state).await {
            let _ = client.put(&url)
                .bearer_auth(&new_at)
                .header("Content-Length", "0")
                .send()
                .await;
        }
    }
    Ok(())
}

// --- WebSocket Sync Engine (Extension Bridge) ---

async fn get_spotify_track_info(state: &Arc<Mutex<AppState>>) -> serde_json::Value {
    if let Some(playback) = spotify_get_playback_state(state).await {
        let track = &playback["item"];
        let is_playing = playback["is_playing"].as_bool().unwrap_or(false);
        let album_art = track["album"]["images"].as_array()
            .and_then(|imgs| {
                imgs.iter()
                    .find(|i| {
                        let h = i["height"].as_u64().unwrap_or(0);
                        h >= 64 && h <= 300
                    })
                    .or(imgs.first())
            })
            .and_then(|i| i["url"].as_str())
            .unwrap_or("");

        let volume = playback["device"]["volume_percent"].as_u64();
        let device_name = playback["device"]["name"].as_str().unwrap_or("Spotify");
        let device_type = playback["device"]["type"].as_str().unwrap_or("Device");

        serde_json::json!({
            "type": "spotify_state",
            "is_playing": is_playing,
            "track_name": track["name"].as_str().unwrap_or(""),
            "artist_name": track["artists"][0]["name"].as_str().unwrap_or(""),
            "album_art": album_art,
            "album_name": track["album"]["name"].as_str().unwrap_or(""),
            "volume_percent": volume,
            "device_name": device_name,
            "device_type": device_type
        })
    } else {
        serde_json::json!({
            "type": "spotify_state",
            "is_playing": false,
            "track_name": "",
            "artist_name": "",
            "album_art": "",
            "album_name": "",
            "volume_percent": null,
            "device_name": "",
            "device_type": ""
        })
    }
}

/// Mode-aware track info: returns Spotify or SMTC data depending on sync_mode
async fn get_current_track_info(state: &Arc<Mutex<AppState>>) -> serde_json::Value {
    let (mode, yt_playing, app_paused, is_ducked, mic_active, duck_level) = {
        let s = state.lock().unwrap();
        (s.sync_mode.clone(), s.yt_playing, s.app_paused_spotify, s.is_ducked, s.mic_active, s.ducking_level)
    };
    let mut val = match mode {
        SyncMode::Universal => {
            let info = tokio::task::spawn_blocking(|| {
                ActiveBackend::new().get_track_info()
            }).await.unwrap_or_default();
            info.to_json()
        }
        SyncMode::Spotify => {
            let mut info = get_spotify_track_info(state).await;
            if info["track_name"].as_str().unwrap_or("").is_empty() {
                let fallback_info = tokio::task::spawn_blocking(|| {
                    ActiveBackend::new().get_track_info()
                }).await.unwrap_or_default();
                if !fallback_info.track_name.is_empty() {
                    info = fallback_info.to_json();
                }
            }
            info
        }
    };
    if let Some(obj) = val.as_object_mut() {
        obj.insert("yt_playing".to_string(), serde_json::Value::Bool(yt_playing));
        obj.insert("app_paused_spotify".to_string(), serde_json::Value::Bool(app_paused));
        obj.insert("is_ducked".to_string(), serde_json::Value::Bool(is_ducked));
        obj.insert("mic_active".to_string(), serde_json::Value::Bool(mic_active));
        obj.insert("ducking_level".to_string(), serde_json::Value::Number(duck_level.into()));
    }
    val
}

/// Mode-aware media control: pause/resume through the active backend
async fn media_control(state: &Arc<Mutex<AppState>>, play: bool, device_id: Option<String>) -> Result<(), String> {
    let mode = state.lock().unwrap().sync_mode.clone();
    match mode {
        SyncMode::Universal => {
            tokio::task::spawn_blocking(move || {
                let backend = ActiveBackend::new();
                if play { backend.resume() } else { backend.pause() }
            }).await.map_err(|e| e.to_string())?
        }
        SyncMode::Spotify => {
            match spotify_control(state, play, device_id).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    println!("[STILLSOUND] Spotify API control failed ({}), falling back to Universal OS media control!", e);
                    tokio::task::spawn_blocking(move || {
                        let backend = ActiveBackend::new();
                        if play { backend.resume() } else { backend.pause() }
                    }).await.map_err(|err| err.to_string())?
                }
            }
        }
    }
}

/// Mode-aware skip control
async fn media_skip(state: &Arc<Mutex<AppState>>, next: bool, device_id: Option<String>) -> Result<(), String> {
    let mode = state.lock().unwrap().sync_mode.clone();
    match mode {
        SyncMode::Universal => {
            tokio::task::spawn_blocking(move || {
                let backend = ActiveBackend::new();
                if next { backend.skip_next() } else { backend.skip_prev() }
            }).await.map_err(|e| e.to_string())?
        }
        SyncMode::Spotify => {
            match spotify_skip(state, next, device_id).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    println!("[STILLSOUND] Spotify API skip failed ({}), falling back to Universal OS media control!", e);
                    tokio::task::spawn_blocking(move || {
                        let backend = ActiveBackend::new();
                        if next { backend.skip_next() } else { backend.skip_prev() }
                    }).await.map_err(|err| err.to_string())?
                }
            }
        }
    }
}

/// Mode-aware playback state check
async fn media_is_playing(state: &Arc<Mutex<AppState>>) -> bool {
    let mode = state.lock().unwrap().sync_mode.clone();
    match mode {
        SyncMode::Universal => {
            tokio::task::spawn_blocking(|| ActiveBackend::new().is_playing())
                .await.unwrap_or(false)
        }
        SyncMode::Spotify => {
            if let Some(playback) = spotify_get_playback_state(state).await {
                let is_playing = playback["is_playing"].as_bool().unwrap_or(false);
                if is_playing {
                    true
                } else {
                    tokio::task::spawn_blocking(|| ActiveBackend::new().is_playing()).await.unwrap_or(false)
                }
            } else {
                tokio::task::spawn_blocking(|| ActiveBackend::new().is_playing()).await.unwrap_or(false)
            }
        }
    }
}

async fn start_extension_bridge<R: Runtime>(app: AppHandle<R>, state: Arc<Mutex<AppState>>) {
    let ports = [9876, 9878, 9880, 9882];
    let mut listener_opt = None;

    for port in ports {
        let addr = format!("127.0.0.1:{}", port);
        match TcpListener::bind(&addr).await {
            Ok(l) => {
                println!("[STILLSOUND] Native WS Console listening on {}", addr);
                listener_opt = Some(l);
                break;
            }
            Err(e) => {
                println!("[STILLSOUND] Port {} busy ({}), trying next fallback...", port, e);
            }
        }
    }

    let listener = match listener_opt {
        Some(l) => l,
        None => {
            eprintln!("[STILLSOUND] WARNING: All fallback WS ports (9876-9882) are busy. Extension bridge will be offline.");
            return;
        }
    };

    while let Ok((stream, _)) = listener.accept().await {
        let app_clone = app.clone();
        let state_clone = state.clone();
        
        tauri::async_runtime::spawn(async move {
            let bridge_id = (rand::random::<u64>()).to_string();
            let ws_stream = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    eprintln!("[STILLSOUND] WS handshake failed: {}", e);
                    return;
                }
            };
            println!("[STILLSOUND] Extension Linked: {}", bridge_id);
            {
                let mut s = state_clone.lock().unwrap();
                s.active_bridges.insert(bridge_id.clone(), (false, SiteAction::Pause));
            }
            app_clone.emit("bridge_linked", ()).unwrap();

            let (mut write, mut read) = ws_stream.split();

            // Send initial media state on connect (mode-aware)
            let info = get_current_track_info(&state_clone).await;
            if let Ok(json) = serde_json::to_string(&info) {
                let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
            }
            
            // Send site actions config
            let site_actions = state_clone.lock().unwrap().site_actions.clone();
            let config_msg = serde_json::json!({
                "type": "site_config",
                "site_actions": site_actions
            });
            if let Ok(json) = serde_json::to_string(&config_msg) {
                let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
            }

            while let Some(msg) = read.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();
                        let event: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
                        
                        if let Some(event_type) = event["type"].as_str() {
                            match event_type {
                                "get_spotify_info" => {
                                    let info = get_current_track_info(&state_clone).await;
                                    if let Ok(json) = serde_json::to_string(&info) {
                                        let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                                    }
                                }
                                "spotify_play" => {
                                    println!("[STILLSOUND] Extension requested: PLAY");
                                    let (device_id, yt_playing, sync_enabled) = {
                                        let mut s = state_clone.lock().unwrap();
                                        s.manual_pause_detected = false;
                                        s.app_paused_spotify = false;
                                        (s.current_device_id.clone(), s.yt_playing, s.sync_enabled)
                                    };

                                    match media_control(&state_clone, true, device_id.clone()).await {
                                        Ok(_) => println!("[STILLSOUND] Play command succeeded"),
                                        Err(e) => println!("[STILLSOUND] Play command failed: {}", e),
                                    }

                                    if yt_playing && sync_enabled {
                                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                        let _ = media_control(&state_clone, false, device_id).await;
                                        let mut s = state_clone.lock().unwrap();
                                        s.app_paused_spotify = true;
                                        s.auto_pause_count += 1;
                                        s.save();
                                    }

                                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                    let info = get_current_track_info(&state_clone).await;
                                    if let Ok(json) = serde_json::to_string(&info) {
                                        let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                                    }
                                }
                                "spotify_pause" => {
                                    println!("[STILLSOUND] Extension requested: PAUSE");
                                    let device_id = {
                                        let mut s = state_clone.lock().unwrap();
                                        s.manual_pause_detected = true;
                                        s.app_paused_spotify = false;
                                        s.current_device_id.clone()
                                    };

                                    match media_control(&state_clone, false, device_id).await {
                                        Ok(_) => println!("[STILLSOUND] Pause command succeeded"),
                                        Err(e) => println!("[STILLSOUND] Pause command failed: {}", e),
                                    }

                                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                    let info = get_current_track_info(&state_clone).await;
                                    if let Ok(json) = serde_json::to_string(&info) {
                                        let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                                    }
                                }
                                "spotify_next" | "spotify_prev" => {
                                    let is_next = event_type == "spotify_next";
                                    let (device_id, yt_playing, sync_enabled) = {
                                        let s = state_clone.lock().unwrap();
                                        (s.current_device_id.clone(), s.yt_playing, s.sync_enabled)
                                    };

                                    match media_skip(&state_clone, is_next, device_id.clone()).await {
                                        Ok(_) => println!("[STILLSOUND] Skip command succeeded"),
                                        Err(e) => println!("[STILLSOUND] Skip command failed: {}", e),
                                    }

                                    if yt_playing && sync_enabled {
                                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                        let _ = media_control(&state_clone, false, device_id).await;
                                        let mut s = state_clone.lock().unwrap();
                                        s.app_paused_spotify = true;
                                        s.auto_pause_count += 1;
                                        s.save();
                                    }

                                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                                    let info = get_current_track_info(&state_clone).await;
                                    if let Ok(json) = serde_json::to_string(&info) {
                                        let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                                    }
                                }
                                _ => {
                                    // Handle video_playing, video_paused with hostname & action
                                    let hostname = event["hostname"].as_str().map(|s| s.to_string());
                                    let action_str = event["action"].as_str();
                                    let action = action_str.map(|a| match a {
                                        "duck" => SiteAction::Duck,
                                        "ignore" => SiteAction::Ignore,
                                        _ => SiteAction::Pause,
                                    });
                                    handle_extension_event(event_type, Some(bridge_id.clone()), hostname, action, &app_clone, &state_clone).await;
                                }
                            }
                        }
                    }
                }
            }
            
            // Cleanup on disconnect
            println!("[STILLSOUND] Extension Unlinked: {}", bridge_id);
            handle_extension_event("disconnect", Some(bridge_id), None, None, &app_clone, &state_clone).await;
            app_clone.emit("bridge_unlinked", ()).unwrap();
        });
    }
}

pub async fn handle_extension_event<R: Runtime>(
    event: &str,
    bridge_id: Option<String>,
    hostname: Option<String>,
    action_override: Option<SiteAction>,
    app: &AppHandle<R>,
    state: &Arc<Mutex<AppState>>
) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (mut device_id, sync_enabled, sync_mode, _site_action, ducking_level, smart_rewind_enabled, smart_rewind_secs, rewind_threshold, pause_time) = {
        let mut s = state.lock().unwrap();

        // Determine effective site action from authoritative backend site_actions map
        let action = if let Some(ref h) = hostname {
            let host_clean = h.trim_start_matches("www.");
            s.site_actions.iter()
                .find(|(k, _)| host_clean == k.as_str() || host_clean.ends_with(format!(".{}", k).as_str()))
                .map(|(_, a)| a.clone())
                .or(action_override)
                .unwrap_or(SiteAction::Pause)
        } else {
            action_override.unwrap_or(SiteAction::Pause)
        };

        println!("[STILLSOUND] Extension Event: '{}' | Host: {:?} | Action: {:?}", event, hostname, action);

        if let Some(bid) = bridge_id {
            match event {
                "video_playing" => {
                    let is_active = action != SiteAction::Ignore;
                    s.active_bridges.insert(bid, (is_active, action.clone()));
                }
                "video_paused" => {
                    s.active_bridges.insert(bid, (false, action.clone()));
                }
                "disconnect" => {
                    s.active_bridges.remove(&bid);
                }
                _ => {}
            }
        }

        (
            s.current_device_id.clone(),
            s.sync_enabled,
            s.sync_mode.clone(),
            action,
            s.ducking_level,
            s.smart_rewind_enabled,
            s.smart_rewind_seconds,
            s.smart_rewind_threshold_secs,
            s.pause_timestamp,
        )
    };

    // Calculate aggregated states across all tabs
    let (any_pause_playing, any_duck_playing) = {
        let s = state.lock().unwrap();
        let any_pause = s.active_bridges.values().any(|(p, a)| *p && *a == SiteAction::Pause);
        let any_duck = s.active_bridges.values().any(|(p, a)| *p && *a == SiteAction::Duck);
        (any_pause, any_duck)
    };

    let is_any_video_active = any_pause_playing || any_duck_playing;
    {
        let mut s = state.lock().unwrap();
        s.yt_playing = is_any_video_active;
    }

    app.emit("sync_event", if is_any_video_active { "yt_playing" } else { "yt_paused" }).unwrap();

    if !sync_enabled { return; }

    // Auto-discover Spotify device if needed
    if sync_mode == SyncMode::Spotify && device_id.is_none() {
        if let Some(found) = spotify_get_active_device(state).await {
            device_id = Some(found.clone());
            let mut s = state.lock().unwrap();
            s.current_device_id = Some(found);
            s.save();
        }
    }

    if any_pause_playing {
        // High priority: Full pause
        let is_playing = media_is_playing(state).await;
        if is_playing {
            // If ducked, unduck first before hard pausing to keep volume baseline consistent
            smooth_unduck_volume(state).await;
            if let Ok(_) = media_control(state, false, device_id).await {
                let mut s = state.lock().unwrap();
                s.app_paused_spotify = true;
                s.auto_pause_count += 1;
                s.manual_pause_detected = false;
                s.pause_timestamp = Some(now_secs);
                s.save();
                println!("[STILLSOUND] Paused media because video started (mode: {})", sync_mode);
            }
        } else {
            let mut s = state.lock().unwrap();
            if !s.app_paused_spotify {
                s.manual_pause_detected = true;
            }
        }
    } else if any_duck_playing {
        // Medium priority: Smooth Audio Ducking
        let was_paused_by_app = { state.lock().unwrap().app_paused_spotify };
        if was_paused_by_app {
            // Video stepped down from Pause -> Duck: resume playback at ducked level
            let _ = media_control(state, true, device_id.clone()).await;
            let mut s = state.lock().unwrap();
            s.app_paused_spotify = false;
        }
        let is_playing = media_is_playing(state).await;
        if is_playing {
            smooth_duck_volume(state, ducking_level).await;
        }
    } else {
        // Low priority: Restore / Unduck
        let (was_paused_by_app, manual_override, is_ducked) = {
            let s = state.lock().unwrap();
            (s.app_paused_spotify, s.manual_pause_detected, s.is_ducked)
        };

        if is_ducked {
            smooth_unduck_volume(state).await;
        }

        if was_paused_by_app && !manual_override {
            if event != "disconnect" {
                if let Ok(_) = media_control(state, true, device_id.clone()).await {
                    let mut s = state.lock().unwrap();
                    s.app_paused_spotify = false;
                    println!("[STILLSOUND] Resumed media (auto, mode: {})", sync_mode);

                    // Check for Smart Rewind (> threshold seconds paused)
                    if smart_rewind_enabled {
                        if let Some(paused_at) = pause_time {
                            let pause_duration = now_secs.saturating_sub(paused_at);
                            if pause_duration >= rewind_threshold as u64 {
                                println!("[STILLSOUND] Pause duration was {}s (>= {}s threshold) — triggering Smart Rewind", pause_duration, rewind_threshold);
                                let state_rewind = state.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                    let _ = media_seek_backward(&state_rewind, smart_rewind_secs).await;
                                });
                            }
                        }
                    }
                    s.pause_timestamp = None;
                }
            } else {
                println!("[STILLSOUND] Extension disconnected while paused — holding state.");
            }
        } else {
            let mut s = state.lock().unwrap();
            s.app_paused_spotify = false;
            s.pause_timestamp = None;
        }
    }
}

pub async fn handle_mic_event<R: Runtime>(
    mic_active: bool,
    _app: &AppHandle<R>,
    state: &Arc<Mutex<AppState>>
) {
    let (enabled, action, duck_level, device_id, smart_rewind_enabled, smart_rewind_secs, rewind_threshold, yt_playing) = {
        let s = state.lock().unwrap();
        (
            s.mic_aware_enabled,
            s.mic_action.clone(),
            s.ducking_level,
            s.current_device_id.clone(),
            s.smart_rewind_enabled,
            s.smart_rewind_seconds,
            s.smart_rewind_threshold_secs,
            s.yt_playing,
        )
    };

    if !enabled || yt_playing { return; }

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if mic_active {
        let is_playing = media_is_playing(state).await;
        if is_playing {
            match action {
                SiteAction::Duck => {
                    smooth_duck_volume(state, duck_level).await;
                    let mut s = state.lock().unwrap();
                    s.mic_paused_or_ducked = true;
                }
                SiteAction::Pause => {
                    if let Ok(_) = media_control(state, false, device_id).await {
                        let mut s = state.lock().unwrap();
                        s.mic_paused_or_ducked = true;
                        s.pause_timestamp = Some(now_secs);
                    }
                }
                SiteAction::Ignore => {}
            }
        }
    } else {
        let was_mic_actioned = { state.lock().unwrap().mic_paused_or_ducked };
        if was_mic_actioned {
            match action {
                SiteAction::Duck => {
                    smooth_unduck_volume(state).await;
                }
                SiteAction::Pause => {
                    let pause_time = { state.lock().unwrap().pause_timestamp };
                    if let Ok(_) = media_control(state, true, device_id).await {
                        if smart_rewind_enabled {
                            if let Some(paused_at) = pause_time {
                                let dur = now_secs.saturating_sub(paused_at);
                                if dur >= rewind_threshold as u64 {
                                    let state_rewind = state.clone();
                                    tauri::async_runtime::spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                                        let _ = media_seek_backward(&state_rewind, smart_rewind_secs).await;
                                    });
                                }
                            }
                        }
                    }
                }
                SiteAction::Ignore => {}
            }
            let mut s = state.lock().unwrap();
            s.mic_paused_or_ducked = false;
            s.pause_timestamp = None;
        }
    }
}

// --- Spotify Callback Server ---

async fn start_callback_server<R: Runtime>(app: AppHandle<R>, state: Arc<Mutex<AppState>>) {
    use warp::Filter;

    let app_clone = app.clone();
    let state_clone = state.clone();

    let callback = warp::path("callback")
        .and(warp::query::<HashMap<String, String>>())
        .map(move |params: HashMap<String, String>| {
            if let Some(code) = params.get("code") {
                let code = code.clone();
                let app_inner = app_clone.clone();
                let state_inner = state_clone.clone();
                
                tauri::async_runtime::spawn(async move {
                    handle_spotify_code(code, app_inner, state_inner).await;
                });
                
                warp::reply::html("<h1>Authentication Successful!</h1><p>StillSound Studio is now linked. You can close this window.</p>")
            } else {
                warp::reply::html("<h1>Authentication Failed</h1>")
            }
        });

    println!("[AUTH] Callback server starting on 127.0.0.1:8921");
    warp::serve(callback).run(([127, 0, 0, 1], 8921)).await;
}

// --- OBS Studio Overlay Server ---

async fn start_obs_server() {
    use warp::Filter;

    let overlay_html = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>StillSound OBS Overlay</title>
    <style>
        @import url('https://fonts.googleapis.com/css2?family=Inter:wght@500;600;700&display=swap');
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { background: transparent; font-family: 'Inter', -apple-system, sans-serif; overflow: hidden; padding: 20px; }
        .overlay-card { background: rgba(28, 28, 30, 0.85); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px); border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 28px; padding: 12px 20px; display: inline-flex; align-items: center; gap: 14px; box-shadow: 0 12px 35px rgba(0, 0, 0, 0.5); transition: all 0.4s cubic-bezier(0.16, 1, 0.3, 1); opacity: 0; transform: translateY(20px) scale(0.95); }
        .overlay-card.visible { opacity: 1; transform: translateY(0) scale(1); }
        .overlay-card.paused-video { border-color: rgba(255, 76, 0, 0.5); background: linear-gradient(135deg, rgba(28, 28, 30, 0.9) 0%, rgba(45, 20, 10, 0.9) 100%); }
        .overlay-card.ducked-video { border-color: rgba(255, 170, 0, 0.5); background: linear-gradient(135deg, rgba(28, 28, 30, 0.9) 0%, rgba(45, 35, 10, 0.9) 100%); }
        .album-art { width: 48px; height: 48px; border-radius: 12px; object-fit: cover; background: #2c2c2e; flex-shrink: 0; box-shadow: 0 4px 12px rgba(0,0,0,0.3); }
        .info { display: flex; flex-direction: column; gap: 2px; max-width: 280px; }
        .track-name { color: #ffffff; font-size: 15px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .track-artist { color: rgba(255, 255, 255, 0.7); font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
        .paused-video .track-artist { color: #ff5e1a; font-weight: 600; }
        .ducked-video .track-artist { color: #ffaa00; font-weight: 600; }
        .waveform { display: flex; align-items: center; gap: 3px; height: 18px; padding-left: 6px; }
        .waveform span { display: block; width: 3px; height: 4px; background: #1ed760; border-radius: 2px; transition: height 0.2s ease; }
        .waveform.active span:nth-child(1) { animation: bounce 0.8s infinite alternate ease-in-out; }
        .waveform.active span:nth-child(2) { animation: bounce 0.6s infinite alternate ease-in-out 0.2s; }
        .waveform.active span:nth-child(3) { animation: bounce 0.9s infinite alternate ease-in-out 0.4s; }
        .waveform.active span:nth-child(4) { animation: bounce 0.7s infinite alternate ease-in-out 0.1s; }
        .paused-video .waveform span { background: #ff4c00; }
        .ducked-video .waveform span { background: #ffaa00; }
        @keyframes bounce { 0% { height: 4px; } 100% { height: 18px; } }
    </style>
</head>
<body>
    <div class="overlay-card" id="card">
        <img class="album-art" id="art" src="" />
        <div class="info">
            <div class="track-name" id="title">Waiting for Spotify...</div>
            <div class="track-artist" id="artist">StillSound Studio</div>
        </div>
        <div class="waveform" id="wave">
            <span></span><span></span><span></span><span></span>
        </div>
    </div>
    <script>
        const card = document.getElementById('card');
        const art = document.getElementById('art');
        const title = document.getElementById('title');
        const artist = document.getElementById('artist');
        const wave = document.getElementById('wave');

        function connect() {
            const ws = new WebSocket('ws://127.0.0.1:9876');
            ws.onopen = () => {
                ws.send(JSON.stringify({ type: 'get_spotify_info' }));
                setInterval(() => {
                    if (ws.readyState === WebSocket.OPEN) {
                        ws.send(JSON.stringify({ type: 'get_spotify_info' }));
                    }
                }, 1000);
            };
            ws.onmessage = (e) => {
                try {
                    const data = JSON.parse(e.data);
                    if (data.track_name && data.track_name.length > 0) {
                        card.classList.add('visible');
                        title.textContent = data.track_name;
                        
                        let sub = data.artist_name || '';
                        if (data.album_name && data.album_name !== data.track_name) {
                            sub += sub ? ' · ' + data.album_name : data.album_name;
                        }
                        
                        card.classList.remove('paused-video', 'ducked-video');
                        if (data.is_ducked) {
                            card.classList.add('ducked-video');
                            artist.textContent = 'Ducked · Ambient Feed Video';
                            wave.classList.add('active');
                        } else if (data.yt_playing || data.app_paused_spotify) {
                            card.classList.add('paused-video');
                            artist.textContent = 'Auto-Paused · Watching Video';
                            wave.classList.remove('active');
                        } else if (!data.is_playing) {
                            artist.textContent = sub ? `${sub} (Paused)` : 'Paused';
                            wave.classList.remove('active');
                        } else {
                            artist.textContent = sub;
                            wave.classList.add('active');
                        }

                        if (data.album_art && art.src !== data.album_art) {
                            art.src = data.album_art;
                        }
                    } else {
                        card.classList.remove('visible');
                    }
                } catch (err) {}
            };
            ws.onclose = () => { setTimeout(connect, 2000); };
        }
        connect();
    </script>
</body>
</html>"#;

    let route = warp::any().map(move || warp::reply::html(overlay_html));
    println!("[OBS] Overlay server starting on http://127.0.0.1:9877/overlay");
    warp::serve(route).run(([127, 0, 0, 1], 9877)).await;
}

// --- Spotify Auth & API ---

async fn handle_spotify_code<R: Runtime>(code: String, app: AppHandle<R>, state: Arc<Mutex<AppState>>) {
    let (client_id, verifier) = {
        let s = state.lock().unwrap();
        (s.client_id.clone(), s.code_verifier.clone())
    };

    if let (Some(cid), Some(ver)) = (client_id, verifier) {
        let client = Client::new();
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", &code);
        params.insert("redirect_uri", "http://127.0.0.1:8921/callback");
        params.insert("client_id", &cid);
        params.insert("code_verifier", &ver);

        let res = client.post("https://accounts.spotify.com/api/token")
            .form(&params)
            .send()
            .await;

        if let Ok(res) = res {
            if let Ok(data) = res.json::<serde_json::Value>().await {
                if let Some(token) = data["access_token"].as_str() {
                    let token = token.to_string();
                    let refresh = data["refresh_token"].as_str().map(|s| s.to_string());
                    
                    {
                        let mut s = state.lock().unwrap();
                        s.access_token = Some(token.clone());
                        if refresh.is_some() {
                            s.refresh_token = refresh;
                        }
                        s.spotify_ready = true;
                        s.save();
                    }
                    
                    let device = spotify_get_active_device_internal(&token).await;
                    if let Some(device) = device {
                        let mut s = state.lock().unwrap();
                        s.current_device_id = Some(device);
                        s.save();
                    }
                    
                    println!("[AUTH] Successfully exchanged PKCE code for tokens!");
                    app.emit("auth_success", ()).unwrap();
                    return;
                }
            }
        }
    }
}

async fn spotify_refresh_token(state: &Arc<Mutex<AppState>>) -> Result<String, String> {
    let (client_id, refresh_token) = {
        let s = state.lock().unwrap();
        (s.client_id.clone(), s.refresh_token.clone())
    };

    if let (Some(cid), Some(rt)) = (client_id, refresh_token) {
        let client = Client::new();
        let mut params = HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", &rt);
        params.insert("client_id", &cid);

        let res = client.post("https://accounts.spotify.com/api/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if res.status().is_success() {
            let data: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
            if let Some(new_at) = data["access_token"].as_str() {
                let new_at = new_at.to_string();
                let new_rt = data["refresh_token"].as_str().map(|s| s.to_string());
                
                let mut s = state.lock().unwrap();
                s.access_token = Some(new_at.clone());
                if new_rt.is_some() {
                    s.refresh_token = new_rt;
                }
                s.auth_failure_count = 0;
                s.save();
                println!("[AUTH] Token refreshed successfully!");
                return Ok(new_at);
            }
        }
        
        let mut s = state.lock().unwrap();
        s.auth_failure_count += 1;
        if s.auth_failure_count >= 3 && s.sync_mode == SyncMode::Spotify {
            println!("[AUTH] 3 consecutive OAuth refresh failures — circuit breaking to Universal SMTC mode!");
            s.sync_mode = SyncMode::Universal;
        }
        s.save();
    } else {
        let mut s = state.lock().unwrap();
        s.auth_failure_count += 1;
        if s.auth_failure_count >= 3 && s.sync_mode == SyncMode::Spotify {
            println!("[AUTH] 3 consecutive OAuth refresh failures — circuit breaking to Universal SMTC mode!");
            s.sync_mode = SyncMode::Universal;
            s.save();
        }
    }
    Err("Failed to refresh token".into())
}

async fn spotify_get_active_device_internal(access_token: &str) -> Option<String> {
    let client = Client::new();
    let res = client.get("https://api.spotify.com/v1/me/player/devices")
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;

    let data: serde_json::Value = res.json().await.ok()?;
    let devices = data["devices"].as_array()?;
    
    devices.iter().find(|d| d["is_active"].as_bool().unwrap_or(false))
        .or(devices.get(0))
        .and_then(|d| d["id"].as_str().map(|s| s.to_string()))
}

async fn spotify_get_active_device(state: &Arc<Mutex<AppState>>) -> Option<String> {
    let token = state.lock().unwrap().access_token.clone()?;
    if let Some(device) = spotify_get_active_device_internal(&token).await {
        return Some(device);
    }
    if let Ok(new_at) = spotify_refresh_token(state).await {
        return spotify_get_active_device_internal(&new_at).await;
    }
    None
}

async fn spotify_get_playback_state_internal(access_token: &str) -> Option<serde_json::Value> {
    let client = Client::new();
    let res = client.get("https://api.spotify.com/v1/me/player")
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;
    
    if res.status() == 200 {
        res.json().await.ok()
    } else {
        None
    }
}

async fn spotify_get_playback_state(state: &Arc<Mutex<AppState>>) -> Option<serde_json::Value> {
    let token = state.lock().unwrap().access_token.clone()?;
    let res = spotify_get_playback_state_internal(&token).await;
    if res.is_some() {
        return res;
    }
    if let Ok(new_at) = spotify_refresh_token(state).await {
        return spotify_get_playback_state_internal(&new_at).await;
    }
    None
}

async fn spotify_control_internal(access_token: &str, play: bool, device_id: Option<String>) -> Result<reqwest::Response, String> {
    let client = Client::new();
    let mut url = if play {
        "https://api.spotify.com/v1/me/player/play".to_string()
    } else {
        "https://api.spotify.com/v1/me/player/pause".to_string()
    };

    if let Some(ref id) = device_id {
        url = format!("{}?device_id={}", url, id);
    }

    client.put(&url)
        .bearer_auth(access_token)
        .header("Content-Length", "0")
        .send()
        .await
        .map_err(|e| e.to_string())
}

async fn spotify_control(state: &Arc<Mutex<AppState>>, play: bool, device_id: Option<String>) -> Result<(), String> {
    let token = state.lock().unwrap().access_token.clone().ok_or("No token")?;
    
    if let Some(playback) = spotify_get_playback_state_internal(&token).await {
        let is_currently_playing = playback["is_playing"].as_bool().unwrap_or(false);
        if play == is_currently_playing {
            return Ok(());
        }
    }

    let res = spotify_control_internal(&token, play, device_id.clone()).await?;
    
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Ok(new_at) = spotify_refresh_token(state).await {
            let res = spotify_control_internal(&new_at, play, device_id).await?;
            if res.status().is_success() {
                return Ok(());
            }
        }
    } else if res.status().is_success() {
        return Ok(());
    }

    Err(format!("Control failed with status: {}", res.status()))
}

async fn spotify_set_volume_internal(access_token: &str, volume: u32, device_id: Option<String>) -> Result<reqwest::Response, String> {
    let client = Client::new();
    let vol = volume.min(100);
    let mut url = format!("https://api.spotify.com/v1/me/player/volume?volume_percent={}", vol);
    if let Some(ref id) = device_id {
        url = format!("{}&device_id={}", url, id);
    }
    client.put(&url)
        .bearer_auth(access_token)
        .header("Content-Length", "0")
        .send()
        .await
        .map_err(|e| e.to_string())
}

async fn spotify_set_volume(state: &Arc<Mutex<AppState>>, volume: u32, device_id: Option<String>) -> Result<(), String> {
    let token = state.lock().unwrap().access_token.clone().ok_or("No token")?;
    let res = spotify_set_volume_internal(&token, volume, device_id.clone()).await?;
    
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Ok(new_at) = spotify_refresh_token(state).await {
            let res = spotify_set_volume_internal(&new_at, volume, device_id).await?;
            if res.status().is_success() {
                return Ok(());
            }
        }
    } else if res.status().is_success() {
        return Ok(());
    }

    Err(format!("Volume failed with status: {}", res.status()))
}

async fn spotify_skip_internal(access_token: &str, next: bool, device_id: Option<String>) -> Result<reqwest::Response, String> {
    let client = Client::new();
    let mut url = if next {
        "https://api.spotify.com/v1/me/player/next".to_string()
    } else {
        "https://api.spotify.com/v1/me/player/previous".to_string()
    };

    if let Some(ref id) = device_id {
        url = format!("{}?device_id={}", url, id);
    }

    client.post(&url)
        .bearer_auth(access_token)
        .header("Content-Length", "0")
        .send()
        .await
        .map_err(|e| e.to_string())
}

async fn spotify_skip(state: &Arc<Mutex<AppState>>, next: bool, device_id: Option<String>) -> Result<(), String> {
    let token = state.lock().unwrap().access_token.clone().ok_or("No token")?;
    let res = spotify_skip_internal(&token, next, device_id.clone()).await?;
    
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Ok(new_at) = spotify_refresh_token(state).await {
            let res = spotify_skip_internal(&new_at, next, device_id).await?;
            if res.status().is_success() {
                return Ok(());
            }
        }
    } else if res.status().is_success() {
        return Ok(());
    }

    Err(format!("Skip failed with status: {}", res.status()))
}

// --- Tauri Commands ---

#[tauri::command]
fn update_settings(
    app: tauri::AppHandle,
    state: tauri::State<StateWrapper>, 
    sync: bool, 
    vol: u32,
    autostart: bool,
    autostart_minimized: bool,
    minimize_to_tray: bool,
    site_actions: Option<HashMap<String, SiteAction>>,
    ducking_level: Option<u32>,
    smart_rewind_enabled: Option<bool>,
    smart_rewind_seconds: Option<u32>,
    mic_aware_enabled: Option<bool>,
    mic_action: Option<String>,
    mini_hud_enabled: Option<bool>,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    
    let mut s = state.0.lock().unwrap();
    
    if s.autostart != autostart {
        let manager = app.autolaunch();
        if autostart {
            let _ = manager.enable();
        } else {
            let _ = manager.disable();
        }
    }

    s.sync_enabled = sync;
    s.volume = vol;
    s.autostart = autostart;
    s.autostart_minimized = autostart_minimized;
    s.minimize_to_tray = minimize_to_tray;
    
    if let Some(actions) = site_actions {
        s.site_actions = actions;
    }
    if let Some(dl) = ducking_level {
        s.ducking_level = dl.clamp(5, 80);
    }
    if let Some(sre) = smart_rewind_enabled {
        s.smart_rewind_enabled = sre;
    }
    if let Some(srs) = smart_rewind_seconds {
        s.smart_rewind_seconds = srs.clamp(1, 30);
    }
    if let Some(mae) = mic_aware_enabled {
        s.mic_aware_enabled = mae;
    }
    if let Some(ma) = mic_action {
        s.mic_action = match ma.as_str() {
            "duck" => SiteAction::Duck,
            "pause" => SiteAction::Pause,
            _ => SiteAction::Ignore,
        };
    }
    if let Some(mhe) = mini_hud_enabled {
        s.mini_hud_enabled = mhe;
    }
    
    s.save();
    Ok(())
}

#[tauri::command]
async fn set_volume(state: tauri::State<'_, StateWrapper>, vol: u32) -> Result<(), String> {
    let device_id = {
        let mut s = state.0.lock().unwrap();
        s.volume = vol;
        if !s.is_ducked {
            s.pre_duck_volume = None;
        }
        s.save();
        s.current_device_id.clone()
    };

    spotify_set_volume(&state.0, vol, device_id).await
}

#[tauri::command]
async fn start_auth(state: tauri::State<'_, StateWrapper>, client_id: Option<String>) -> Result<(), String> {
    let effective_client_id = client_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());

    if effective_client_id == "PLACEHOLDER_STILLSOUND_CLIENT_ID" {
        return Err("No Spotify Client ID configured. Please provide one in Advanced Setup, or build with STILLSOUND_CLIENT_ID env var.".into());
    }

    let mut random_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    {
        let mut s = state.0.lock().unwrap();
        s.client_id = Some(effective_client_id.clone());
        s.code_verifier = Some(verifier);
        s.save();
    }

    let auth_url = format!(
        "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri=http://127.0.0.1:8921/callback&scope=user-modify-playback-state%20user-read-playback-state%20user-read-currently-playing&code_challenge_method=S256&code_challenge={}",
        effective_client_id, challenge
    );
    
    open::that(auth_url).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn refresh_spotify_device(state: tauri::State<'_, StateWrapper>) -> Result<(), String> {
    if let Some(device) = spotify_get_active_device(&state.0).await {
        let mut s = state.0.lock().unwrap();
        s.current_device_id = Some(device);
        s.save();
        return Ok(());
    }
    Err("No active device found on Spotify. Please open Spotify on one of your devices.".into())
}

#[tauri::command]
fn close_window<R: Runtime>(app: AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
fn minimize_window<R: Runtime>(app: AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }
}

#[tauri::command]
fn get_initial_state(state: tauri::State<StateWrapper>) -> AppState {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    open::that(url).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_spotify_volume(state: tauri::State<'_, StateWrapper>) -> Result<u32, String> {
    if let Some(playback) = spotify_get_playback_state(&state.0).await {
        if let Some(vol) = playback["device"]["volume_percent"].as_u64() {
            let mut s = state.0.lock().unwrap();
            s.volume = vol as u32;
            s.save();
            return Ok(vol as u32);
        }
    }
    let s = state.0.lock().unwrap();
    Ok(s.volume)
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn get_track_info(state: tauri::State<'_, StateWrapper>) -> Result<serde_json::Value, String> {
    let info = get_current_track_info(&state.0).await;
    Ok(info)
}

#[tauri::command]
async fn spotify_play_pause(state: tauri::State<'_, StateWrapper>, play: Option<bool>) -> Result<(), String> {
    let device_id = {
        let s = state.0.lock().unwrap();
        s.current_device_id.clone()
    };

    let target_play = match play {
        Some(p) => p,
        None => !media_is_playing(&state.0).await,
    };

    media_control(&state.0, target_play, device_id).await
}

#[tauri::command]
async fn spotify_skip_track(state: tauri::State<'_, StateWrapper>, next: bool) -> Result<(), String> {
    let device_id = {
        let s = state.0.lock().unwrap();
        s.current_device_id.clone()
    };
    media_skip(&state.0, next, device_id).await
}

#[tauri::command]
fn set_sync_mode(state: tauri::State<StateWrapper>, mode: String) -> Result<(), String> {
    let sync_mode = match mode.as_str() {
        "universal" => SyncMode::Universal,
        "spotify" => SyncMode::Spotify,
        _ => return Err(format!("Unknown sync mode: {}", mode)),
    };
    let mut s = state.0.lock().unwrap();
    s.sync_mode = sync_mode;
    s.save();
    println!("[STILLSOUND] Sync mode changed to: {}", mode);
    Ok(())
}

#[tauri::command]
fn get_weekly_recap(state: tauri::State<'_, StateWrapper>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    serde_json::json!({
        "focus_minutes": s.focus_minutes_this_week,
        "music_minutes": s.music_minutes_this_week,
        "auto_pause_count": s.auto_pause_count,
        "hours_saved": (s.auto_pause_count as f32 * 0.1).round()
    })
}

#[tauri::command]
fn toggle_mini_hud<R: Runtime>(app: AppHandle<R>, state: tauri::State<'_, StateWrapper>, show: bool) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    s.mini_hud_enabled = show;
    s.save();
    
    let _ = app.emit("mini_hud_toggled", show);

    if let Some(hud) = app.get_webview_window("mini_hud") {
        if show {
            #[cfg(target_os = "windows")]
            {
                use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_NOACTIVATE};
                use windows::Win32::Foundation::HWND;
                if let Ok(hwnd) = hud.hwnd() {
                    unsafe {
                        let ex_style = GetWindowLongW(HWND(hwnd.0 as _), GWL_EXSTYLE);
                        SetWindowLongW(HWND(hwnd.0 as _), GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as i32);
                    }
                }
            }
            let _ = hud.show();
        } else {
            let _ = hud.hide();
        }
    }
    Ok(())
}

#[tauri::command]
fn save_hud_pos(state: tauri::State<'_, StateWrapper>, x: i32, y: i32) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    s.mini_hud_pos = Some((x, y));
    s.save();
    Ok(())
}

#[tauri::command]
fn set_ducking_level(state: tauri::State<'_, StateWrapper>, level: u32) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    s.ducking_level = level.clamp(5, 80);
    s.save();
    Ok(())
}

#[tauri::command]
fn set_site_action(state: tauri::State<'_, StateWrapper>, site: String, action: String) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    let act = match action.as_str() {
        "duck" => SiteAction::Duck,
        "ignore" => SiteAction::Ignore,
        _ => SiteAction::Pause,
    };
    s.site_actions.insert(site, act);
    s.save();
    Ok(())
}

#[tauri::command]
fn set_smart_rewind(state: tauri::State<'_, StateWrapper>, enabled: bool, seconds: u32) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    s.smart_rewind_enabled = enabled;
    s.smart_rewind_seconds = seconds.clamp(1, 30);
    s.save();
    Ok(())
}

#[tauri::command]
fn set_mic_aware(state: tauri::State<'_, StateWrapper>, enabled: bool, action: String) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    s.mic_aware_enabled = enabled;
    s.mic_action = match action.as_str() {
        "pause" => SiteAction::Pause,
        _ => SiteAction::Duck,
    };
    s.save();
    Ok(())
}

#[tauri::command]
fn drag_mini_hud(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn spotify_seek_backward(state: tauri::State<'_, StateWrapper>, seconds: u32) -> Result<(), String> {
    media_seek_backward(&state.0, seconds).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let initial_state = AppState::load();
    let state = Arc::new(Mutex::new(initial_state));
    let state_wrapper = StateWrapper(state.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, _shortcut, event| {
            if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let state = app.state::<StateWrapper>().0.clone();
                tauri::async_runtime::spawn(async move {
                    let device_id = {
                        let s = state.lock().unwrap();
                        s.current_device_id.clone()
                    };
                    let is_playing = media_is_playing(&state).await;
                    let _ = media_control(&state, !is_playing, device_id).await;
                });
            }
        }).build())
        .manage(state_wrapper)
        .setup(move |app| {
            // Handle autostart minimized
            let args: Vec<String> = std::env::args().collect();
            if args.contains(&"--minimized".to_string()) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            // Restore Mini-HUD if enabled
            let (hud_enabled, hud_pos) = {
                let s = state.lock().unwrap();
                (s.mini_hud_enabled, s.mini_hud_pos)
            };
            if let Some(hud) = app.get_webview_window("mini_hud") {
                #[cfg(target_os = "windows")]
                {
                    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_NOACTIVATE};
                    use windows::Win32::Foundation::HWND;
                    if let Ok(hwnd) = hud.hwnd() {
                        unsafe {
                            let ex_style = GetWindowLongW(HWND(hwnd.0 as _), GWL_EXSTYLE);
                            SetWindowLongW(HWND(hwnd.0 as _), GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as i32);
                        }
                    }
                }
                if let Some((x, y)) = hud_pos {
                    let _ = hud.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                }
                if hud_enabled {
                    let _ = hud.show();
                }
            }

            // Tray Menu
            let play_pause_i = MenuItem::with_id(app, "play_pause", "Play / Pause", true, None::<&str>).unwrap();
            let next_i = MenuItem::with_id(app, "next", "Next Track", true, None::<&str>).unwrap();
            let prev_i = MenuItem::with_id(app, "prev", "Previous Track", true, None::<&str>).unwrap();
            let show_hud_i = MenuItem::with_id(app, "toggle_hud", "Toggle Mini-HUD", true, None::<&str>).unwrap();
            let show_i = MenuItem::with_id(app, "show", "Show StillSound Studio", true, None::<&str>).unwrap();
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();
            let menu = Menu::with_items(app, &[&show_i, &show_hud_i, &play_pause_i, &next_i, &prev_i, &quit_i]).unwrap();

            let tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("StillSound Studio")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "toggle_hud" => {
                        if let Some(hud) = app.get_webview_window("mini_hud") {
                            let is_visible = hud.is_visible().unwrap_or(false);
                            if is_visible {
                                let _ = hud.hide();
                            } else {
                                let _ = hud.show();
                                let _ = hud.set_focus();
                            }
                            let state = app.state::<StateWrapper>().0.clone();
                            let mut s = state.lock().unwrap();
                            s.mini_hud_enabled = !is_visible;
                            s.save();
                        }
                    }
                    "play_pause" => {
                        let state = app.state::<StateWrapper>().0.clone();
                        tauri::async_runtime::spawn(async move {
                            let device_id = {
                                let s = state.lock().unwrap();
                                s.current_device_id.clone()
                            };
                            let is_playing = media_is_playing(&state).await;
                            let _ = media_control(&state, !is_playing, device_id).await;
                        });
                    }
                    "next" => {
                        let state = app.state::<StateWrapper>().0.clone();
                        tauri::async_runtime::spawn(async move {
                            let device_id = {
                                let s = state.lock().unwrap();
                                s.current_device_id.clone()
                            };
                            let _ = media_skip(&state, true, device_id).await;
                        });
                    }
                    "prev" => {
                        let state = app.state::<StateWrapper>().0.clone();
                        tauri::async_runtime::spawn(async move {
                            let device_id = {
                                let s = state.lock().unwrap();
                                s.current_device_id.clone()
                            };
                            let _ = media_skip(&state, false, device_id).await;
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app).unwrap();

            let tray_handle = tray.clone();
            let state_tray = state.clone();
            let app_handle_for_sync = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let info = get_current_track_info(&state_tray).await;
                    let title = info["track_name"].as_str().unwrap_or("");
                    let artist = info["artist_name"].as_str().unwrap_or("");
                    if !title.is_empty() {
                        let _ = tray_handle.set_tooltip(Some(format!("StillSound: {} - {}", title, artist)));
                    } else {
                        let _ = tray_handle.set_tooltip(Some("StillSound Studio"));
                    }
                    // Emit HUD updates for the mini_hud overlay window
                    let _ = app_handle_for_sync.emit("hud_update", info);
                }
            });

            // Weekly focus recap ticker
            let state_recap = state.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    let mut s = state_recap.lock().unwrap();
                    if s.yt_playing {
                        s.focus_minutes_this_week += 1;
                        s.save();
                    } else {
                        s.music_minutes_this_week += 1;
                        s.save();
                    }
                }
            });

            // Start Mic / Call Activity Monitor
            let app_mic = app.handle().clone();
            let state_mic = state.clone();
            tauri::async_runtime::spawn(async move {
                mic_monitor::start_mic_monitor(app_mic, state_mic).await;
            });

            // Global hotkey registration
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            if let Err(e) = app.global_shortcut().register("ctrl+alt+p") {
                println!("[HOTKEY] Could not register ctrl+alt+p: {}", e);
            } else {
                println!("[HOTKEY] Registered global hotkey: Ctrl+Alt+P");
            }
            if let Err(e) = app.global_shortcut().register("alt+shift+p") {
                println!("[HOTKEY] Could not register alt+shift+p: {}", e);
            } else {
                println!("[HOTKEY] Registered global hotkey: Alt+Shift+P");
            }

            // Minimize to tray handling
            let state_for_event = state.clone();
            if let Some(window) = app.get_webview_window("main") {
                let win_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Resized(_) = event {
                        if win_clone.is_minimized().unwrap_or(false) {
                            let to_tray = {
                                let s = state_for_event.lock().unwrap();
                                s.minimize_to_tray
                            };
                            if to_tray {
                                let _ = win_clone.hide();
                            }
                        }
                    }
                });
            }

            let handle = app.handle().clone();
            let state_arc = state.clone();
            tauri::async_runtime::spawn(async move {
                start_extension_bridge(handle, state_arc).await;
            });

            let handle_cb = app.handle().clone();
            let state_cb = state.clone();
            tauri::async_runtime::spawn(async move {
                start_callback_server(handle_cb, state_cb).await;
            });

            tauri::async_runtime::spawn(async move {
                start_obs_server().await;
            });

            // Re-validate session on startup
            let state_init = state.clone();
            tauri::async_runtime::spawn(async move {
                let has_token = state_init.lock().unwrap().access_token.is_some();
                if has_token {
                    println!("[INIT] Verifying Spotify session...");
                    if spotify_get_active_device(&state_init).await.is_some() {
                        println!("[INIT] Session valid!");
                    } else {
                        println!("[INIT] Session invalid or no device found.");
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            update_settings, 
            set_volume,
            start_auth, 
            refresh_spotify_device,
            close_window,
            minimize_window,
            get_initial_state,
            get_spotify_volume,
            get_app_version,
            open_url,
            get_track_info,
            spotify_play_pause,
            spotify_skip_track,
            set_sync_mode,
            get_weekly_recap,
            toggle_mini_hud,
            save_hud_pos,
            set_ducking_level,
            set_site_action,
            set_smart_rewind,
            set_mic_aware,
            spotify_seek_backward,
            drag_mini_hud
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                } else if window.label() == "mini_hud" {
                    api.prevent_close();
                    let _ = window.hide();
                    let app_handle = window.app_handle();
                    let state = app_handle.state::<StateWrapper>();
                    let mut s = state.0.lock().unwrap();
                    s.mini_hud_enabled = false;
                    s.save();
                    let _ = app_handle.emit("mini_hud_toggled", false);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
