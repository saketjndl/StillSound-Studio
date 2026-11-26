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
use std::sync::{Arc, Mutex, OnceLock};
use futures_util::{StreamExt, SinkExt};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio::net::TcpListener;
use reqwest::Client;
use std::collections::HashMap;
use rand::RngCore;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::fs;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
static REFRESH_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static CONFIG_TX: OnceLock<tokio::sync::broadcast::Sender<String>> = OnceLock::new();

fn http_client() -> &'static Client {
    HTTP_CLIENT.get_or_init(Client::new)
}

fn refresh_lock() -> &'static tokio::sync::Mutex<()> {
    REFRESH_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn config_broadcaster() -> &'static tokio::sync::broadcast::Sender<String> {
    CONFIG_TX.get_or_init(|| {
        let (tx, _) = tokio::sync::broadcast::channel(16);
        tx
    })
}

fn broadcast_site_config(site_actions: &HashMap<String, SiteAction>) {
    let config_msg = serde_json::json!({
        "type": "site_config",
        "site_actions": site_actions
    });
    if let Ok(json) = serde_json::to_string(&config_msg) {
        let _ = config_broadcaster().send(json);
    }
}

fn domain_matches(hostname: &str, domain: &str) -> bool {
    hostname == domain || hostname.ends_with(&format!(".{}", domain))
}

fn resolve_site_action(site_actions: &HashMap<String, SiteAction>, hostname: &str) -> Option<SiteAction> {
    let clean = hostname.trim().to_lowercase();
    let clean = clean.strip_prefix("www.").unwrap_or(&clean);
    if clean.is_empty() {
        return None;
    }

    if let Some(action) = site_actions.get(clean) {
        return Some(action.clone());
    }

    let parts: Vec<&str> = clean.split('.').collect();
    for i in 1..parts.len().saturating_sub(1) {
        let parent = parts[i..].join(".");
        if let Some(action) = site_actions.get(&parent) {
            return Some(action.clone());
        }
    }
    None
}

fn is_allowed_ws_origin(origin: &str) -> bool {
    let lower = origin.to_ascii_lowercase();
    lower.starts_with("chrome-extension://")
        || lower.starts_with("moz-extension://")
        || lower.starts_with("safari-web-extension://")
        || lower == "http://127.0.0.1:9877"
        || lower == "http://localhost:9877"
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

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
    #[serde(default = "default_true")]
    pub album_art_spin: bool,
    #[serde(default = "default_hud_mode")]
    pub mini_hud_mode: String,
    #[serde(default)]
    pub mini_hud_locked: bool,
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
    #[serde(default)]
    pub recap_week_start: u64,
    // Legacy support for backwards compatibility
    #[serde(default)]
    pub enabled_sites: HashMap<String, bool>,
    #[serde(skip)]
    pub active_bridges: HashMap<String, (bool, SiteAction)>,
    #[serde(skip)]
    pub bridge_clients: HashMap<String, String>,
    #[serde(skip)]
    pub active_video_host: Option<String>,
    #[serde(skip)]
    pub snooze_until: Option<u64>,
    #[serde(skip)]
    pub snooze_indefinite: bool,
    #[serde(skip)]
    pub video_pause_instant: Option<Instant>,
    #[serde(skip)]
    pub mic_pause_instant: Option<Instant>,
    #[serde(skip)]
    pub duck_generation: u64,
    #[serde(skip)]
    pub oauth_state: Option<String>,
    #[serde(skip)]
    pub last_music_playing: bool,
}

fn default_true() -> bool { true }
fn default_ducking_level() -> u32 { 20 }
fn default_rewind_seconds() -> u32 { 5 }
fn default_rewind_threshold() -> u32 { 30 }
fn default_mic_action() -> SiteAction { SiteAction::Duck }
fn default_hud_mode() -> String { "pinned".to_string() }

fn is_sync_active(s: &AppState) -> bool {
    if !s.sync_enabled || s.snooze_indefinite {
        return false;
    }
    if let Some(until) = s.snooze_until {
        if current_unix_secs() < until {
            return false;
        }
    }
    true
}

fn snooze_remaining_secs(s: &AppState) -> Option<u64> {
    let until = s.snooze_until?;
    let now = current_unix_secs();
    if until > now {
        Some(until - now)
    } else {
        None
    }
}

fn parse_site_action(action: &str) -> SiteAction {
    match action {
        "duck" => SiteAction::Duck,
        "ignore" => SiteAction::Ignore,
        _ => SiteAction::Pause,
    }
}

fn normalize_hud_mode(mode: &str) -> String {
    match mode {
        "autohide" | "peek" | "pinned" => mode.to_string(),
        _ => "pinned".to_string(),
    }
}

fn format_bridge_clients(clients: &HashMap<String, String>, count: usize) -> Vec<String> {
    let mut names: Vec<String> = clients.values().cloned().collect();
    names.sort();
    names.dedup();
    if names.is_empty() && count > 0 {
        vec![if count == 1 { "Browser".to_string() } else { format!("{} Browsers", count) }]
    } else {
        names
    }
}

fn apply_site_preset_map(map: &mut HashMap<String, SiteAction>, preset: &str) -> bool {
    const STREAMING: [&str; 9] = [
        "youtube.com", "netflix.com", "primevideo.com", "hotstar.com",
        "disneyplus.com", "twitch.tv", "crunchyroll.com", "vimeo.com", "jiocinema.com",
    ];
    const SOCIAL: [&str; 6] = [
        "twitter.com", "x.com", "reddit.com", "instagram.com", "facebook.com", "tiktok.com",
    ];
    const EDUCATIONAL: [&str; 4] = [
        "udemy.com", "coursera.org", "khanacademy.org", "nptel.ac.in",
    ];
    const CONFERENCING: [&str; 3] = [
        "meet.google.com", "zoom.us", "teams.microsoft.com",
    ];

    match preset {
        "study" => {
            for s in STREAMING.iter().chain(EDUCATIONAL.iter()).chain(CONFERENCING.iter()).chain(SOCIAL.iter()) {
                map.insert((*s).to_string(), SiteAction::Pause);
            }
            true
        }
        "chill" => {
            for s in STREAMING.iter().chain(EDUCATIONAL.iter()).chain(CONFERENCING.iter()) {
                map.insert((*s).to_string(), SiteAction::Pause);
            }
            for s in SOCIAL.iter() {
                map.insert((*s).to_string(), SiteAction::Duck);
            }
            true
        }
        "meeting" => {
            for s in CONFERENCING.iter().chain(STREAMING.iter()).chain(EDUCATIONAL.iter()) {
                map.insert((*s).to_string(), SiteAction::Pause);
            }
            for s in SOCIAL.iter() {
                map.insert((*s).to_string(), SiteAction::Ignore);
            }
            true
        }
        _ => false,
    }
}

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
            recap_week_start: current_unix_secs(),
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
            album_art_spin: true,
            mini_hud_mode: "pinned".to_string(),
            mini_hud_locked: false,
            mini_hud_pos: None,
            site_actions: {
                let mut map = HashMap::new();
                apply_site_preset_map(&mut map, "chill");
                map
            },
            enabled_sites: HashMap::new(),
            active_bridges: HashMap::new(),
            bridge_clients: HashMap::new(),
            active_video_host: None,
            snooze_until: None,
            snooze_indefinite: false,
            video_pause_instant: None,
            mic_pause_instant: None,
            duck_generation: 0,
            oauth_state: None,
            last_music_playing: false,
        }
    }
}

pub struct StateWrapper(pub Arc<Mutex<AppState>>);

impl AppState {
    fn get_save_path() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let mut path = PathBuf::from(appdata);
                path.push("StillSound");
                fs::create_dir_all(&path).ok();
                path.push("settings.json");
                return path;
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Ok(home) = std::env::var("HOME") {
                let mut path = PathBuf::from(home);
                path.push("Library");
                path.push("Application Support");
                path.push("StillSound");
                fs::create_dir_all(&path).ok();
                path.push("settings.json");
                return path;
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                if !xdg.trim().is_empty() {
                    let mut path = PathBuf::from(xdg);
                    path.push("StillSound");
                    fs::create_dir_all(&path).ok();
                    path.push("settings.json");
                    return path;
                }
            }
            if let Ok(home) = std::env::var("HOME") {
                let mut path = PathBuf::from(home);
                path.push(".config");
                path.push("StillSound");
                fs::create_dir_all(&path).ok();
                path.push("settings.json");
                return path;
            }
        }

        let mut path = std::env::current_dir().unwrap_or_default();
        path.push("stillsound_settings.json");
        path
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
            state.video_pause_instant = None;
            state.mic_pause_instant = None;
            state.duck_generation = 0;
            state.oauth_state = None;
            state.last_music_playing = false;

            let now = current_unix_secs();
            if state.recap_week_start == 0 || now.saturating_sub(state.recap_week_start) >= 7 * 86400 {
                state.focus_minutes_this_week = 0;
                state.music_minutes_this_week = 0;
                state.auto_pause_count = 0;
                state.recap_week_start = now;
            }

            // Migrate legacy enabled_sites to site_actions if needed
            if state.site_actions.is_empty() && !state.enabled_sites.is_empty() {
                println!("[MIGRATION] Migrating legacy site toggles to site_actions matrix...");
                let social_sites = ["twitter.com", "x.com", "reddit.com", "instagram.com", "facebook.com", "tiktok.com"];
                for (site, enabled) in &state.enabled_sites {
                    let clean = site.trim_start_matches("www.");
                    if !enabled {
                        state.site_actions.insert(site.clone(), SiteAction::Ignore);
                    } else if social_sites.iter().any(|s| domain_matches(clean, s)) {
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

    let (current_vol, effective_target, device_id, my_gen) = {
        let mut s = state.lock().unwrap();
        if s.is_ducked {
            return; // Already ducked
        }
        let cur = universal_vol.unwrap_or(s.volume).clamp(5, 100);
        if cur <= target_vol {
            return; // Already quieter than or equal to target duck volume
        }
        s.volume = cur;
        s.pre_duck_volume = Some(cur);
        s.is_ducked = true;
        s.duck_generation = s.duck_generation.wrapping_add(1);
        (cur, target_vol.min(cur), s.current_device_id.clone(), s.duck_generation)
    };

    println!("[DUCK] Smooth ducking volume: {}% -> {}% (mode: {})", current_vol, effective_target, sync_mode);

    let (steps, step_delay_ms) = if sync_mode == SyncMode::Spotify { (2, 100) } else { (4, 40) };
    let diff = current_vol.saturating_sub(effective_target);
    for i in 1..=steps {
        if state.lock().unwrap().duck_generation != my_gen {
            return;
        }
        let step_vol = current_vol.saturating_sub(diff * i / steps).max(effective_target);
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
        if i < steps {
            tokio::time::sleep(tokio::time::Duration::from_millis(step_delay_ms)).await;
        }
    }
}

async fn smooth_unduck_volume(state: &Arc<Mutex<AppState>>) {
    let (target_vol, current_duck_vol, device_id, sync_mode, my_gen) = {
        let mut s = state.lock().unwrap();
        if !s.is_ducked {
            return;
        }
        s.is_ducked = false;
        let pre = s.pre_duck_volume.unwrap_or(s.volume).clamp(5, 100);
        s.pre_duck_volume = None;
        s.duck_generation = s.duck_generation.wrapping_add(1);
        (pre, s.ducking_level, s.current_device_id.clone(), s.sync_mode.clone(), s.duck_generation)
    };

    println!("[DUCK] Smooth restoring volume: {}% -> {}% (mode: {})", current_duck_vol, target_vol, sync_mode);
    let (steps, step_delay_ms) = if sync_mode == SyncMode::Spotify { (2, 100) } else { (4, 40) };
    let start_vol = current_duck_vol.min(target_vol);
    let diff = target_vol.saturating_sub(start_vol);
    for i in 1..=steps {
        if state.lock().unwrap().duck_generation != my_gen {
            return;
        }
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
        if i < steps {
            tokio::time::sleep(tokio::time::Duration::from_millis(step_delay_ms)).await;
        }
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
    let client = http_client();
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
        if let Ok(new_at) = spotify_refresh_token(state, Some(&token)).await {
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
    let (
        mode,
        yt_playing,
        app_paused,
        is_ducked,
        mic_active,
        duck_level,
        bridge_count,
        bridge_clients,
        album_art_spin,
        mini_hud_mode,
        mini_hud_locked,
        active_video_host,
        snooze_secs,
        snooze_indefinite,
        sync_effective,
    ) = {
        let s = state.lock().unwrap();
        (
            s.sync_mode.clone(),
            s.yt_playing,
            s.app_paused_spotify,
            s.is_ducked,
            s.mic_active,
            s.ducking_level,
            s.active_bridges.len(),
            format_bridge_clients(&s.bridge_clients, s.active_bridges.len()),
            s.album_art_spin,
            s.mini_hud_mode.clone(),
            s.mini_hud_locked,
            s.active_video_host.clone(),
            snooze_remaining_secs(&s),
            s.snooze_indefinite,
            is_sync_active(&s),
        )
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
        obj.insert("active_bridge_count".to_string(), serde_json::Value::Number(bridge_count.into()));
        obj.insert("bridge_clients".to_string(), serde_json::json!(bridge_clients));
        obj.insert("album_art_spin".to_string(), serde_json::Value::Bool(album_art_spin));
        obj.insert("mini_hud_mode".to_string(), serde_json::Value::String(mini_hud_mode));
        obj.insert("mini_hud_locked".to_string(), serde_json::Value::Bool(mini_hud_locked));
        obj.insert("active_video_host".to_string(), serde_json::json!(active_video_host));
        obj.insert("snooze_remaining_secs".to_string(), serde_json::json!(snooze_secs));
        obj.insert("snooze_indefinite".to_string(), serde_json::Value::Bool(snooze_indefinite));
        obj.insert("sync_effective".to_string(), serde_json::Value::Bool(sync_effective));
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
            let ws_stream = match accept_hdr_async(
                stream,
                |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
                    if let Some(origin) = req.headers().get("origin").and_then(|v| v.to_str().ok()) {
                        if !is_allowed_ws_origin(origin) {
                            let err = tokio_tungstenite::tungstenite::http::Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .body(Some("Forbidden WebSocket origin".to_string()))
                                .unwrap();
                            return Err(err);
                        }
                    }
                    Ok(resp)
                },
            )
            .await
            {
                Ok(ws) => ws,
                Err(e) => {
                    eprintln!("[STILLSOUND] WS handshake rejected/failed: {}", e);
                    return;
                }
            };

            println!("[STILLSOUND] Extension Linked: {}", bridge_id);
            let linked_count = {
                let mut s = state_clone.lock().unwrap();
                s.active_bridges.insert(bridge_id.clone(), (false, SiteAction::Pause));
                s.active_bridges.len()
            };
            let _ = app_clone.emit("bridge_linked", linked_count);

            let (mut write, mut read) = ws_stream.split();
            let mut config_rx = config_broadcaster().subscribe();
            let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(20));
            ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ping_interval.tick().await;
            let mut last_activity = Instant::now();

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

            loop {
                tokio::select! {
                    biased;
                    _ = ping_interval.tick() => {
                        // Evict half-open sockets (e.g. crashed/suspended browser workers)
                        if last_activity.elapsed() > std::time::Duration::from_secs(60) {
                            println!("[STILLSOUND] Extension heartbeat timeout for {}", bridge_id);
                            break;
                        }
                        if write.send(tokio_tungstenite::tungstenite::Message::Ping(Vec::new())).await.is_err() {
                            break;
                        }
                    }
                    config_update = config_rx.recv() => {
                        match config_update {
                            Ok(json) => {
                                if write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await.is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                let site_actions = state_clone.lock().unwrap().site_actions.clone();
                                let msg = serde_json::json!({ "type": "site_config", "site_actions": site_actions });
                                if let Ok(json) = serde_json::to_string(&msg) {
                                    let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    ws_next = read.next() => {
                        let Some(msg_res) = ws_next else { break; };
                        let Ok(msg) = msg_res else { break; };
                        last_activity = Instant::now();
                        if msg.is_close() {
                            break;
                        }
                        if msg.is_ping() {
                            let _ = write.send(tokio_tungstenite::tungstenite::Message::Pong(msg.into_data())).await;
                            continue;
                        }
                        if msg.is_pong() || !msg.is_text() {
                            continue;
                        }
                        let Ok(text) = msg.to_text() else { continue; };
                        let event: serde_json::Value = serde_json::from_str(text).unwrap_or_default();

                        if let Some(event_type) = event["type"].as_str() {
                            match event_type {
                                "ping" => {
                                    let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(r#"{"type":"pong"}"#.to_string())).await;
                                }
                                "hello" => {
                                    let client = event["client"].as_str().unwrap_or("Browser").trim();
                                    let label = if client.is_empty() { "Browser" } else { client };
                                    println!("[STILLSOUND] Extension identified as {} ({})", label, bridge_id);
                                    let count = {
                                        let mut s = state_clone.lock().unwrap();
                                        s.bridge_clients.insert(bridge_id.clone(), label.to_string());
                                        s.active_bridges.len()
                                    };
                                    let _ = app_clone.emit("bridge_linked", count);
                                }
                                "set_site_action" => {
                                    if let (Some(site), Some(act_str)) = (event["site"].as_str(), event["action"].as_str()) {
                                        let clean_site = site.trim().to_lowercase();
                                        let clean_site = clean_site.strip_prefix("www.").unwrap_or(&clean_site).to_string();
                                        if !clean_site.is_empty() {
                                            let act = parse_site_action(act_str);
                                            let actions_snapshot = {
                                                let mut s = state_clone.lock().unwrap();
                                                s.site_actions.insert(clean_site, act);
                                                s.save();
                                                broadcast_site_config(&s.site_actions);
                                                s.site_actions.clone()
                                            };
                                            let _ = app_clone.emit("site_actions_updated", actions_snapshot);
                                        }
                                    }
                                }
                                "get_spotify_info" => {
                                    let info = get_current_track_info(&state_clone).await;
                                    if let Ok(json) = serde_json::to_string(&info) {
                                        let _ = write.send(tokio_tungstenite::tungstenite::Message::Text(json)).await;
                                    }
                                }
                                "spotify_play" => {
                                    println!("[STILLSOUND] Extension requested: PLAY");
                                    let (device_id, yt_playing, sync_active) = {
                                        let mut s = state_clone.lock().unwrap();
                                        s.manual_pause_detected = false;
                                        s.app_paused_spotify = false;
                                        (s.current_device_id.clone(), s.yt_playing, is_sync_active(&s))
                                    };

                                    match media_control(&state_clone, true, device_id.clone()).await {
                                        Ok(_) => println!("[STILLSOUND] Play command succeeded"),
                                        Err(e) => println!("[STILLSOUND] Play command failed: {}", e),
                                    }

                                    if yt_playing && sync_active {
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
                                    let (device_id, was_ducked) = {
                                        let mut s = state_clone.lock().unwrap();
                                        s.manual_pause_detected = true;
                                        s.app_paused_spotify = false;
                                        (s.current_device_id.clone(), s.is_ducked)
                                    };

                                    if was_ducked {
                                        smooth_unduck_volume(&state_clone).await;
                                    }

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
                                    let (device_id, yt_playing, sync_active) = {
                                        let s = state_clone.lock().unwrap();
                                        (s.current_device_id.clone(), s.yt_playing, is_sync_active(&s))
                                    };

                                    match media_skip(&state_clone, is_next, device_id.clone()).await {
                                        Ok(_) => println!("[STILLSOUND] Skip command succeeded"),
                                        Err(e) => println!("[STILLSOUND] Skip command failed: {}", e),
                                    }

                                    if yt_playing && sync_active {
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
                                "video_playing" | "video_paused" => {
                                    let hostname = event["hostname"].as_str().map(|s| s.to_string());
                                    let action_str = event["action"].as_str();
                                    let action = action_str.map(|a| match a {
                                        "duck" => SiteAction::Duck,
                                        "ignore" => SiteAction::Ignore,
                                        _ => SiteAction::Pause,
                                    });
                                    handle_extension_event(event_type, Some(bridge_id.clone()), hostname, action, &app_clone, &state_clone).await;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            // Cleanup on disconnect
            println!("[STILLSOUND] Extension Unlinked: {}", bridge_id);
            handle_extension_event("disconnect", Some(bridge_id.clone()), None, None, &app_clone, &state_clone).await;
            let remaining_count = {
                let mut s = state_clone.lock().unwrap();
                s.bridge_clients.remove(&bridge_id);
                s.active_bridges.len()
            };
            let _ = app_clone.emit("bridge_unlinked", remaining_count);
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
    let (mut device_id, sync_enabled, sync_mode, _site_action, ducking_level, smart_rewind_enabled, smart_rewind_secs, rewind_threshold, pause_instant) = {
        let mut s = state.lock().unwrap();

        // Determine effective site action from authoritative backend site_actions map (exact match first)
        let action = if let Some(ref h) = hostname {
            resolve_site_action(&s.site_actions, h)
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
                    if is_active {
                        if let Some(ref h) = hostname {
                            s.active_video_host = Some(h.strip_prefix("www.").unwrap_or(h).to_string());
                        }
                    }
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
            is_sync_active(&s),
            s.sync_mode.clone(),
            action,
            s.ducking_level,
            s.smart_rewind_enabled,
            s.smart_rewind_seconds,
            s.smart_rewind_threshold_secs,
            s.video_pause_instant,
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
        if !is_any_video_active {
            s.active_video_host = None;
        }
    }

    let _ = app.emit("sync_event", if is_any_video_active { "yt_playing" } else { "yt_paused" });

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
            smooth_unduck_volume(state).await;
            if media_control(state, false, device_id).await.is_ok() {
                let mut s = state.lock().unwrap();
                s.app_paused_spotify = true;
                s.auto_pause_count += 1;
                s.manual_pause_detected = false;
                s.video_pause_instant = Some(Instant::now());
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
            if media_control(state, true, device_id.clone()).await.is_ok() {
                let mut s = state.lock().unwrap();
                s.app_paused_spotify = false;
                s.video_pause_instant = None;
            }
            // Player was just resumed by StillSound; apply duck directly without racing PlaybackStatus
            smooth_duck_volume(state, ducking_level).await;
        } else if media_is_playing(state).await {
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
                if media_control(state, true, device_id.clone()).await.is_ok() {
                    {
                        let mut s = state.lock().unwrap();
                        s.app_paused_spotify = false;
                        s.manual_pause_detected = false;
                        s.video_pause_instant = None;
                    }
                    println!("[STILLSOUND] Resumed media (auto, mode: {})", sync_mode);

                    if smart_rewind_enabled {
                        if let Some(paused_at) = pause_instant {
                            let pause_duration = paused_at.elapsed().as_secs();
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
                }
            } else {
                println!("[STILLSOUND] Extension disconnected while paused — holding state.");
            }
        } else if event != "disconnect" {
            let mut s = state.lock().unwrap();
            s.app_paused_spotify = false;
            s.manual_pause_detected = false;
            s.video_pause_instant = None;
        }
    }
}

pub async fn handle_mic_event<R: Runtime>(
    mic_active: bool,
    _app: &AppHandle<R>,
    state: &Arc<Mutex<AppState>>
) {
    let (enabled, sync_active, action, duck_level, device_id, smart_rewind_enabled, smart_rewind_secs, rewind_threshold, yt_playing, was_mic_actioned, manual_override, mic_pause_instant) = {
        let s = state.lock().unwrap();
        (
            s.mic_aware_enabled,
            is_sync_active(&s),
            s.mic_action.clone(),
            s.ducking_level,
            s.current_device_id.clone(),
            s.smart_rewind_enabled,
            s.smart_rewind_seconds,
            s.smart_rewind_threshold_secs,
            s.yt_playing,
            s.mic_paused_or_ducked,
            s.manual_pause_detected,
            s.mic_pause_instant,
        )
    };

    if !mic_active {
        if was_mic_actioned {
            if !yt_playing {
                match action {
                    SiteAction::Duck => {
                        smooth_unduck_volume(state).await;
                    }
                    SiteAction::Pause => {
                        if !manual_override && media_control(state, true, device_id).await.is_ok() {
                            if smart_rewind_enabled {
                                if let Some(paused_at) = mic_pause_instant {
                                    let dur = paused_at.elapsed().as_secs();
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
            }
            let mut s = state.lock().unwrap();
            s.mic_paused_or_ducked = false;
            s.mic_pause_instant = None;
        }
        return;
    }

    if !enabled || !sync_active || yt_playing { return; }

    if media_is_playing(state).await {
        match action {
            SiteAction::Duck => {
                smooth_duck_volume(state, duck_level).await;
                let mut s = state.lock().unwrap();
                s.mic_paused_or_ducked = true;
            }
            SiteAction::Pause => {
                if media_control(state, false, device_id).await.is_ok() {
                    let mut s = state.lock().unwrap();
                    s.mic_paused_or_ducked = true;
                    s.mic_pause_instant = Some(Instant::now());
                }
            }
            SiteAction::Ignore => {}
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
            let state_valid = {
                let s = state_clone.lock().unwrap();
                match (s.oauth_state.as_deref(), params.get("state").map(String::as_str)) {
                    (Some(expected), Some(actual)) => expected == actual,
                    _ => false,
                }
            };
            if !state_valid {
                return warp::reply::html(
                    "<h1>Authentication Failed</h1><p>Invalid or expired OAuth state parameter.</p>",
                );
            }

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
    match warp::serve(callback).try_bind_ephemeral(([127, 0, 0, 1], 8921)) {
        Ok((_addr, server)) => server.await,
        Err(e) => eprintln!("[AUTH] Callback server failed to bind 127.0.0.1:8921: {}", e),
    }
}

// --- OBS Studio Overlay Server ---

async fn start_obs_server(state: Arc<Mutex<AppState>>) {
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
        .album-art { width: 48px; height: 48px; border-radius: 12px; object-fit: cover; background: #2c2c2e; flex-shrink: 0; box-shadow: 0 4px 12px rgba(0,0,0,0.3); display: none; }
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
        <img class="album-art" id="art" src="" alt="" />
        <div class="info">
            <div class="track-name" id="title">Waiting for audio...</div>
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

        async function pollState() {
            try {
                const res = await fetch('/state', { cache: 'no-store' });
                if (!res.ok) return;
                const data = await res.json();
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

                    if (data.album_art) {
                        art.style.display = 'block';
                        if (art.src !== data.album_art) {
                            art.src = data.album_art;
                        }
                    } else {
                        art.style.display = 'none';
                        art.removeAttribute('src');
                    }
                } else {
                    card.classList.remove('visible');
                }
            } catch (err) {}
        }

        pollState();
        setInterval(pollState, 1000);
    </script>
</body>
</html>"#;

    let state_route = warp::path("state")
        .and(warp::get())
        .and_then(move || {
            let state_clone = state.clone();
            async move {
                let info = get_current_track_info(&state_clone).await;
                Ok::<_, warp::Rejection>(warp::reply::json(&info))
            }
        });

    let overlay_route = warp::any().map(move || warp::reply::html(overlay_html));
    let routes = state_route.or(overlay_route);

    println!("[OBS] Overlay server starting on http://127.0.0.1:9877/overlay");
    match warp::serve(routes).try_bind_ephemeral(([127, 0, 0, 1], 9877)) {
        Ok((_addr, server)) => server.await,
        Err(e) => eprintln!("[OBS] Overlay server failed to bind 127.0.0.1:9877: {}", e),
    }
}

// --- Spotify Auth & API ---

async fn handle_spotify_code<R: Runtime>(code: String, app: AppHandle<R>, state: Arc<Mutex<AppState>>) {
    let (client_id, verifier) = {
        let mut s = state.lock().unwrap();
        // Clear pending OAuth state once callback enters code exchange to prevent replay.
        let creds = (s.client_id.clone(), s.code_verifier.clone());
        s.oauth_state = None;
        s.code_verifier = None;
        s.save();
        creds
    };

    if let (Some(cid), Some(ver)) = (client_id, verifier) {
        let client = http_client();
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
            if res.status().is_success() {
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
                            s.auth_failure_count = 0;
                            s.save();
                        }

                        let device = spotify_get_active_device_internal(&token).await;
                        if let Some(device) = device {
                            let mut s = state.lock().unwrap();
                            s.current_device_id = Some(device);
                            s.save();
                        }

                        println!("[AUTH] Successfully exchanged PKCE code for tokens!");
                        let _ = app.emit("auth_success", ());
                        return;
                    }
                }
            }
        }
    }
}

async fn spotify_refresh_token(state: &Arc<Mutex<AppState>>, failed_token: Option<&str>) -> Result<String, String> {
    let _refresh_guard = refresh_lock().lock().await;

    let (client_id, refresh_token) = {
        let s = state.lock().unwrap();
        if let (Some(failed), Some(ref current)) = (failed_token, &s.access_token) {
            if current != failed {
                return Ok(current.clone());
            }
        }
        (s.client_id.clone(), s.refresh_token.clone())
    };

    if let (Some(cid), Some(rt)) = (client_id, refresh_token) {
        let client = http_client();
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

enum SpotifyFetch<T> {
    Ok(T),
    NoContent,
    Unauthorized,
    Error,
}

async fn spotify_get_active_device_fetch(access_token: &str) -> SpotifyFetch<Option<String>> {
    let client = http_client();
    let Ok(res) = client
        .get("https://api.spotify.com/v1/me/player/devices")
        .bearer_auth(access_token)
        .send()
        .await
    else {
        return SpotifyFetch::Error;
    };

    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        return SpotifyFetch::Unauthorized;
    }
    if !res.status().is_success() {
        return SpotifyFetch::Error;
    }

    let Ok(data) = res.json::<serde_json::Value>().await else {
        return SpotifyFetch::Error;
    };
    let Some(devices) = data["devices"].as_array() else {
        return SpotifyFetch::Ok(None);
    };

    let selected = devices
        .iter()
        .find(|d| d["is_active"].as_bool().unwrap_or(false))
        .or_else(|| devices.first())
        .and_then(|d| d["id"].as_str().map(|s| s.to_string()));
    SpotifyFetch::Ok(selected)
}

async fn spotify_get_active_device_internal(access_token: &str) -> Option<String> {
    match spotify_get_active_device_fetch(access_token).await {
        SpotifyFetch::Ok(device) => device,
        _ => None,
    }
}

async fn spotify_get_active_device(state: &Arc<Mutex<AppState>>) -> Option<String> {
    let token = state.lock().unwrap().access_token.clone()?;
    match spotify_get_active_device_fetch(&token).await {
        SpotifyFetch::Ok(device) => device,
        SpotifyFetch::Unauthorized => {
            if let Ok(new_at) = spotify_refresh_token(state, Some(&token)).await {
                spotify_get_active_device_internal(&new_at).await
            } else {
                None
            }
        }
        _ => None,
    }
}

async fn spotify_get_playback_state_fetch(access_token: &str) -> SpotifyFetch<serde_json::Value> {
    let client = http_client();
    let Ok(res) = client
        .get("https://api.spotify.com/v1/me/player")
        .bearer_auth(access_token)
        .send()
        .await
    else {
        return SpotifyFetch::Error;
    };

    match res.status() {
        reqwest::StatusCode::OK => match res.json::<serde_json::Value>().await {
            Ok(val) => SpotifyFetch::Ok(val),
            Err(_) => SpotifyFetch::Error,
        },
        reqwest::StatusCode::NO_CONTENT => SpotifyFetch::NoContent,
        reqwest::StatusCode::UNAUTHORIZED => SpotifyFetch::Unauthorized,
        _ => SpotifyFetch::Error,
    }
}

async fn spotify_get_playback_state(state: &Arc<Mutex<AppState>>) -> Option<serde_json::Value> {
    let token = state.lock().unwrap().access_token.clone()?;
    match spotify_get_playback_state_fetch(&token).await {
        SpotifyFetch::Ok(val) => Some(val),
        SpotifyFetch::Unauthorized => {
            if let Ok(new_at) = spotify_refresh_token(state, Some(&token)).await {
                match spotify_get_playback_state_fetch(&new_at).await {
                    SpotifyFetch::Ok(val) => Some(val),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

async fn send_spotify_with_retry<F, Fut>(
    state: &Arc<Mutex<AppState>>,
    action_name: &str,
    make_req: F,
) -> Result<(), String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response, String>>,
{
    let token = state.lock().unwrap().access_token.clone().ok_or("No token")?;
    let res = make_req(token.clone()).await?;

    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Ok(new_at) = spotify_refresh_token(state, Some(&token)).await {
            let retry_res = make_req(new_at).await?;
            if retry_res.status().is_success() {
                return Ok(());
            }
            return Err(format!("{} failed with status: {}", action_name, retry_res.status()));
        }
    } else if res.status().is_success() {
        return Ok(());
    }

    Err(format!("{} failed with status: {}", action_name, res.status()))
}

async fn spotify_control_internal(access_token: &str, play: bool, device_id: Option<String>) -> Result<reqwest::Response, String> {
    let client = http_client();
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
    if let SpotifyFetch::Ok(playback) = spotify_get_playback_state_fetch(&token).await {
        let is_currently_playing = playback["is_playing"].as_bool().unwrap_or(false);
        if play == is_currently_playing {
            return Ok(());
        }
    }

    send_spotify_with_retry(state, "Control", move |tok| {
        let dev = device_id.clone();
        async move { spotify_control_internal(&tok, play, dev).await }
    })
    .await
}

async fn spotify_set_volume_internal(access_token: &str, volume: u32, device_id: Option<String>) -> Result<reqwest::Response, String> {
    let client = http_client();
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
    send_spotify_with_retry(state, "Volume", move |tok| {
        let dev = device_id.clone();
        async move { spotify_set_volume_internal(&tok, volume, dev).await }
    })
    .await
}

async fn spotify_skip_internal(access_token: &str, next: bool, device_id: Option<String>) -> Result<reqwest::Response, String> {
    let client = http_client();
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
    send_spotify_with_retry(state, "Skip", move |tok| {
        let dev = device_id.clone();
        async move { spotify_skip_internal(&tok, next, dev).await }
    })
    .await
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
    smart_rewind_threshold_secs: Option<u32>,
    mic_aware_enabled: Option<bool>,
    mic_action: Option<String>,
    mini_hud_enabled: Option<bool>,
    album_art_spin: Option<bool>,
    mini_hud_mode: Option<String>,
    mini_hud_locked: Option<bool>,
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
    if let Some(srt) = smart_rewind_threshold_secs {
        s.smart_rewind_threshold_secs = srt.clamp(5, 300);
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
    if let Some(aas) = album_art_spin {
        s.album_art_spin = aas;
        let _ = app.emit("hud_spin_changed", aas);
    }
    if let Some(ref mhm) = mini_hud_mode {
        s.mini_hud_mode = normalize_hud_mode(mhm);
    }
    if let Some(mhl) = mini_hud_locked {
        s.mini_hud_locked = mhl;
    }
    if album_art_spin.is_some() || mini_hud_mode.is_some() || mini_hud_locked.is_some() {
        let _ = app.emit(
            "hud_prefs_changed",
            serde_json::json!({
                "album_art_spin": s.album_art_spin,
                "mini_hud_mode": s.mini_hud_mode,
                "mini_hud_locked": s.mini_hud_locked,
            }),
        );
    }

    s.save();
    broadcast_site_config(&s.site_actions);
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

    let mut state_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut state_bytes);
    let oauth_state = URL_SAFE_NO_PAD.encode(state_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    {
        let mut s = state.0.lock().unwrap();
        s.client_id = Some(effective_client_id.clone());
        s.code_verifier = Some(verifier);
        s.oauth_state = Some(oauth_state.clone());
        s.save();
    }

    let auth_url = format!(
        "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri=http://127.0.0.1:8921/callback&scope=user-modify-playback-state%20user-read-playback-state%20user-read-currently-playing&code_challenge_method=S256&code_challenge={}&state={}",
        effective_client_id, challenge, oauth_state
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
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Refusing to open non-HTTP(S) URL".into());
    }
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

async fn apply_manual_play_pause(state: &Arc<Mutex<AppState>>, play: Option<bool>) -> Result<(), String> {
    let target_play = match play {
        Some(p) => p,
        None => !media_is_playing(state).await,
    };

    let (device_id, was_ducked) = {
        let mut s = state.lock().unwrap();
        s.manual_pause_detected = !target_play;
        s.app_paused_spotify = false;
        (s.current_device_id.clone(), s.is_ducked)
    };

    if !target_play && was_ducked {
        smooth_unduck_volume(state).await;
    }

    media_control(state, target_play, device_id).await
}

#[tauri::command]
async fn spotify_play_pause(state: tauri::State<'_, StateWrapper>, play: Option<bool>) -> Result<(), String> {
    apply_manual_play_pause(&state.0, play).await
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
fn set_site_action<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, StateWrapper>,
    site: String,
    action: String,
) -> Result<(), String> {
    let clean = site.trim().to_lowercase();
    let clean = clean.strip_prefix("www.").unwrap_or(&clean).to_string();
    if clean.is_empty() {
        return Err("Empty domain".into());
    }
    let mut s = state.0.lock().unwrap();
    let act = parse_site_action(&action);
    s.site_actions.insert(clean, act);
    s.save();
    broadcast_site_config(&s.site_actions);
    let _ = app.emit("site_actions_updated", s.site_actions.clone());
    Ok(())
}

#[tauri::command]
fn remove_site_action<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, StateWrapper>,
    site: String,
) -> Result<(), String> {
    let clean = site.trim().to_lowercase();
    let clean = clean.strip_prefix("www.").unwrap_or(&clean).to_string();
    let mut s = state.0.lock().unwrap();
    s.site_actions.remove(&clean);
    s.save();
    broadcast_site_config(&s.site_actions);
    let _ = app.emit("site_actions_updated", s.site_actions.clone());
    Ok(())
}

#[tauri::command]
fn apply_site_preset<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, StateWrapper>,
    preset: String,
) -> Result<HashMap<String, SiteAction>, String> {
    let mut s = state.0.lock().unwrap();
    if !apply_site_preset_map(&mut s.site_actions, &preset) {
        return Err(format!("Unknown preset: {}", preset));
    }
    s.save();
    broadcast_site_config(&s.site_actions);
    let _ = app.emit("site_actions_updated", s.site_actions.clone());
    Ok(s.site_actions.clone())
}

async fn apply_snooze_change<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<Mutex<AppState>>,
    minutes: Option<u32>,
    until_restart: bool,
) {
    let (should_unduck, should_resume, device_id) = {
        let mut s = state.lock().unwrap();
        if until_restart {
            s.snooze_indefinite = true;
            s.snooze_until = None;
        } else if let Some(mins) = minutes.filter(|&m| m > 0) {
            s.snooze_indefinite = false;
            s.snooze_until = Some(current_unix_secs() + (mins as u64) * 60);
        } else {
            s.snooze_indefinite = false;
            s.snooze_until = None;
        }
        let snoozed = !is_sync_active(&s);
        (
            snoozed && s.is_ducked,
            snoozed && s.app_paused_spotify && !s.manual_pause_detected,
            s.current_device_id.clone(),
        )
    };

    if should_unduck {
        smooth_unduck_volume(state).await;
    }
    if should_resume && media_control(state, true, device_id).await.is_ok() {
        let mut s = state.lock().unwrap();
        s.app_paused_spotify = false;
        s.video_pause_instant = None;
    }
    let info = get_current_track_info(state).await;
    let _ = app.emit("hud_update", info);
}

#[tauri::command]
async fn snooze_sync<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, StateWrapper>,
    minutes: Option<u32>,
    until_restart: Option<bool>,
) -> Result<(), String> {
    apply_snooze_change(&app, &state.0, minutes, until_restart.unwrap_or(false)).await;
    Ok(())
}

#[tauri::command]
fn set_hud_prefs<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, StateWrapper>,
    mode: Option<String>,
    locked: Option<bool>,
    spin: Option<bool>,
) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    if let Some(m) = mode {
        s.mini_hud_mode = normalize_hud_mode(&m);
    }
    if let Some(l) = locked {
        s.mini_hud_locked = l;
    }
    if let Some(sp) = spin {
        s.album_art_spin = sp;
        let _ = app.emit("hud_spin_changed", sp);
    }
    s.save();
    let _ = app.emit(
        "hud_prefs_changed",
        serde_json::json!({
            "album_art_spin": s.album_art_spin,
            "mini_hud_mode": s.mini_hud_mode,
            "mini_hud_locked": s.mini_hud_locked,
        }),
    );
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
        "ignore" => SiteAction::Ignore,
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

fn build_badged_tray_icon(base_rgba: &[u8], w: u32, h: u32, dot_rgba: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = base_rgba.to_vec();
    let Some([dr, dg, db, da]) = dot_rgba else {
        return out;
    };
    let radius = (w.min(h) / 5).max(3) as i32;
    let cx = w as i32 - radius - 1;
    let cy = h as i32 - radius - 1;
    let r_sq = radius * radius;
    let border_sq = (radius + 1) * (radius + 1);
    for y in (cy - radius - 1).max(0)..(cy + radius + 2).min(h as i32) {
        for x in (cx - radius - 1).max(0)..(cx + radius + 2).min(w as i32) {
            let d2 = (x - cx) * (x - cx) + (y - cy) * (y - cy);
            let idx = ((y as u32 * w + x as u32) * 4) as usize;
            if idx + 3 >= out.len() {
                continue;
            }
            if d2 <= r_sq {
                out[idx] = dr;
                out[idx + 1] = dg;
                out[idx + 2] = db;
                out[idx + 3] = da;
            } else if d2 <= border_sq {
                out[idx] = 18;
                out[idx + 1] = 18;
                out[idx + 2] = 20;
                out[idx + 3] = 240;
            }
        }
    }
    out
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
                    let _ = apply_manual_play_pause(&state, None).await;
                });
            }
        }).build())
        .manage(state_wrapper)
        .setup(move |app| {
            // Handle autostart minimized only when both launched with --minimized and setting is enabled
            let args: Vec<String> = std::env::args().collect();
            let autostart_minimized = state.lock().unwrap().autostart_minimized;
            if args.contains(&"--minimized".to_string()) && autostart_minimized {
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
            let snooze_15_i = MenuItem::with_id(app, "snooze_15", "Snooze Sync (15 min)", true, None::<&str>).unwrap();
            let snooze_60_i = MenuItem::with_id(app, "snooze_60", "Snooze Sync (1 hour)", true, None::<&str>).unwrap();
            let snooze_restart_i = MenuItem::with_id(app, "snooze_restart", "Snooze Until Restart", true, None::<&str>).unwrap();
            let snooze_clear_i = MenuItem::with_id(app, "snooze_clear", "Resume Auto Sync", true, None::<&str>).unwrap();
            let show_hud_i = MenuItem::with_id(app, "toggle_hud", "Toggle Mini-HUD", true, None::<&str>).unwrap();
            let show_i = MenuItem::with_id(app, "show", "Show StillSound Studio", true, None::<&str>).unwrap();
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();
            let menu = Menu::with_items(
                app,
                &[
                    &show_i,
                    &show_hud_i,
                    &play_pause_i,
                    &next_i,
                    &prev_i,
                    &snooze_15_i,
                    &snooze_60_i,
                    &snooze_restart_i,
                    &snooze_clear_i,
                    &quit_i,
                ],
            )
            .unwrap();

            let base_icon = app.default_window_icon().unwrap().clone();
            let base_rgba = base_icon.rgba().to_vec();
            let base_w = base_icon.width();
            let base_h = base_icon.height();

            let tray = TrayIconBuilder::new()
                .icon(base_icon)
                .tooltip("StillSound Studio")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        let app_handle = app.clone();
                        let state = app.state::<StateWrapper>().0.clone();
                        tauri::async_runtime::spawn(async move {
                            let was_ducked = {
                                let s = state.lock().unwrap();
                                s.save();
                                s.is_ducked
                            };
                            if was_ducked {
                                smooth_unduck_volume(&state).await;
                            }
                            app_handle.exit(0);
                        });
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
                            let next_visible = !is_visible;
                            if is_visible {
                                let _ = hud.hide();
                            } else {
                                let _ = hud.show();
                            }
                            let state = app.state::<StateWrapper>().0.clone();
                            {
                                let mut s = state.lock().unwrap();
                                s.mini_hud_enabled = next_visible;
                                s.save();
                            }
                            let _ = app.emit("mini_hud_toggled", next_visible);
                        }
                    }
                    "play_pause" => {
                        let state = app.state::<StateWrapper>().0.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = apply_manual_play_pause(&state, None).await;
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
                    "snooze_15" | "snooze_60" | "snooze_restart" | "snooze_clear" => {
                        let id = event.id.as_ref().to_string();
                        let app_handle = app.clone();
                        let state = app.state::<StateWrapper>().0.clone();
                        tauri::async_runtime::spawn(async move {
                            match id.as_str() {
                                "snooze_15" => apply_snooze_change(&app_handle, &state, Some(15), false).await,
                                "snooze_60" => apply_snooze_change(&app_handle, &state, Some(60), false).await,
                                "snooze_restart" => apply_snooze_change(&app_handle, &state, None, true).await,
                                _ => apply_snooze_change(&app_handle, &state, Some(0), false).await,
                            }
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
                let mut last_badge: Option<&'static str> = None;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    {
                        let mut s = state_tray.lock().unwrap();
                        if let Some(until) = s.snooze_until {
                            if current_unix_secs() >= until {
                                s.snooze_until = None;
                            }
                        }
                    }
                    let info = get_current_track_info(&state_tray).await;
                    let is_playing = info["is_playing"].as_bool().unwrap_or(false);
                    {
                        let mut s = state_tray.lock().unwrap();
                        s.last_music_playing = is_playing;
                    }

                    let sync_eff = info["sync_effective"].as_bool().unwrap_or(true);
                    let is_ducked = info["is_ducked"].as_bool().unwrap_or(false);
                    let app_paused = info["app_paused_spotify"].as_bool().unwrap_or(false);
                    let yt_playing = info["yt_playing"].as_bool().unwrap_or(false);
                    let mic_active = info["mic_active"].as_bool().unwrap_or(false);

                    let (badge_key, badge_color, state_label): (&'static str, Option<[u8; 4]>, String) = if !sync_eff {
                        let rem = info["snooze_remaining_secs"].as_u64();
                        let lbl = match rem {
                            Some(sec) => format!("Snoozed {}m", (sec / 60).max(1)),
                            None => "Sync Paused".to_string(),
                        };
                        ("snoozed", Some([120, 120, 130, 255]), lbl)
                    } else if is_ducked {
                        ("ducked", Some([255, 170, 0, 255]), "Ducked".to_string())
                    } else if app_paused || yt_playing {
                        ("paused", Some([255, 76, 0, 255]), "Paused for Video".to_string())
                    } else if mic_active {
                        ("mic", Some([47, 204, 107, 255]), "Mic Active".to_string())
                    } else {
                        ("idle", None, String::new())
                    };

                    if last_badge != Some(badge_key) {
                        last_badge = Some(badge_key);
                        let rgba = build_badged_tray_icon(&base_rgba, base_w, base_h, badge_color);
                        let _ = tray_handle.set_icon(Some(tauri::image::Image::new_owned(rgba, base_w, base_h)));
                    }

                    let title = info["track_name"].as_str().unwrap_or("");
                    let artist = info["artist_name"].as_str().unwrap_or("");
                    let tip = if !title.is_empty() && !state_label.is_empty() {
                        format!("StillSound [{}]: {} - {}", state_label, title, artist)
                    } else if !title.is_empty() {
                        format!("StillSound: {} - {}", title, artist)
                    } else if !state_label.is_empty() {
                        format!("StillSound Studio [{}]", state_label)
                    } else {
                        "StillSound Studio".to_string()
                    };
                    let _ = tray_handle.set_tooltip(Some(tip));
                    // Emit HUD updates for the mini_hud overlay window
                    let _ = app_handle_for_sync.emit("hud_update", info);
                }
            });

            // Weekly focus recap ticker
            let state_recap = state.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    let now = current_unix_secs();
                    let mut s = state_recap.lock().unwrap();
                    if s.recap_week_start == 0 || now.saturating_sub(s.recap_week_start) >= 7 * 86400 {
                        s.recap_week_start = now;
                        s.focus_minutes_this_week = 0;
                        s.music_minutes_this_week = 0;
                        s.auto_pause_count = 0;
                        s.save();
                    }
                    if s.yt_playing {
                        s.focus_minutes_this_week += 1;
                        s.save();
                    } else if s.last_music_playing {
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

            let state_obs = state.clone();
            tauri::async_runtime::spawn(async move {
                start_obs_server(state_obs).await;
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
            remove_site_action,
            apply_site_preset,
            snooze_sync,
            set_hud_prefs,
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
            } else if let tauri::WindowEvent::Moved(pos) = event {
                if window.label() == "mini_hud"
                    && window.is_visible().unwrap_or(false)
                    && pos.x > -10000
                    && pos.y > -10000
                {
                    let app_handle = window.app_handle();
                    let state = app_handle.state::<StateWrapper>();
                    let mut s = state.0.lock().unwrap();
                    let moved_far = match s.mini_hud_pos {
                        Some((px, py)) => (px - pos.x).abs() >= 4 || (py - pos.y).abs() >= 4,
                        None => true,
                    };
                    s.mini_hud_pos = Some((pos.x, pos.y));
                    if moved_far {
                        s.save();
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_matches_exact_and_subdomains_only() {
        assert!(domain_matches("youtube.com", "youtube.com"));
        assert!(domain_matches("music.youtube.com", "youtube.com"));
        assert!(domain_matches("m.twitch.tv", "twitch.tv"));
        assert!(!domain_matches("myyoutube.com", "youtube.com"));
        assert!(!domain_matches("notnetflix.com", "netflix.com"));
    }

    #[test]
    fn resolve_site_action_prefers_exact_over_parent() {
        let mut map = HashMap::new();
        map.insert("youtube.com".to_string(), SiteAction::Pause);
        map.insert("music.youtube.com".to_string(), SiteAction::Ignore);
        map.insert("twitch.tv".to_string(), SiteAction::Duck);

        assert_eq!(
            resolve_site_action(&map, "music.youtube.com"),
            Some(SiteAction::Ignore)
        );
        assert_eq!(
            resolve_site_action(&map, "www.youtube.com"),
            Some(SiteAction::Pause)
        );
        assert_eq!(
            resolve_site_action(&map, "clips.twitch.tv"),
            Some(SiteAction::Duck)
        );
        assert_eq!(resolve_site_action(&map, "myyoutube.com"), None);
    }

    #[test]
    fn ws_origin_check_allows_extensions_and_obs_only() {
        assert!(is_allowed_ws_origin("chrome-extension://abcdef123456"));
        assert!(is_allowed_ws_origin("moz-extension://1234-5678"));
        assert!(is_allowed_ws_origin("safari-web-extension://uuid"));
        assert!(is_allowed_ws_origin("http://127.0.0.1:9877"));
        assert!(is_allowed_ws_origin("http://localhost:9877"));

        assert!(!is_allowed_ws_origin("https://evil.example"));
        assert!(!is_allowed_ws_origin("http://127.0.0.1:3000"));
        assert!(!is_allowed_ws_origin(""));
    }

    #[test]
    fn snooze_and_preset_helpers_behave_accurately() {
        let mut s = AppState::default();
        assert!(is_sync_active(&s));

        s.snooze_until = Some(current_unix_secs() + 300);
        assert!(!is_sync_active(&s));
        assert!(snooze_remaining_secs(&s).unwrap_or(0) > 200);

        s.snooze_until = Some(current_unix_secs().saturating_sub(10));
        assert!(is_sync_active(&s));
        assert_eq!(snooze_remaining_secs(&s), None);

        s.snooze_indefinite = true;
        assert!(!is_sync_active(&s));

        assert!(apply_site_preset_map(&mut s.site_actions, "study"));
        assert_eq!(s.site_actions.get("x.com"), Some(&SiteAction::Pause));
        assert!(apply_site_preset_map(&mut s.site_actions, "meeting"));
        assert_eq!(s.site_actions.get("x.com"), Some(&SiteAction::Ignore));
        assert_eq!(s.site_actions.get("zoom.us"), Some(&SiteAction::Pause));
        assert!(!apply_site_preset_map(&mut s.site_actions, "invalid"));
    }
}
