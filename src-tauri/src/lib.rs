use tauri::{AppHandle, Runtime, Emitter, Manager};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::menu::{Menu, MenuItem};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use futures_util::StreamExt;
use tokio_tungstenite::accept_async;
use tokio::net::TcpListener;
use reqwest::Client;
use std::collections::HashMap;
use rand::RngCore;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::fs;
use std::path::PathBuf;

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
    #[serde(skip)]
    pub active_bridges: std::collections::HashMap<String, bool>,
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
            client_id: None,
            code_verifier: None,
            access_token: None,
            refresh_token: None,
            app_paused_spotify: false,
            manual_pause_detected: false,
            active_bridges: std::collections::HashMap::new(),
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
            serde_json::from_str(&content).unwrap_or_default()
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

// --- Spotify Controller ---

// (Deleted old duplicate functions)


// --- WebSocket Sync Engine (Extension Bridge) ---

async fn start_extension_bridge<R: Runtime>(app: AppHandle<R>, state: Arc<Mutex<AppState>>) {
    let addr = "127.0.0.1:9876";
    let listener = TcpListener::bind(&addr).await.expect("Failed to bind WS port 9876");
    println!("[STILLSOUND] Native WS Console listening on {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let app_clone = app.clone();
        let state_clone = state.clone();
        
        tauri::async_runtime::spawn(async move {
            let bridge_id = (rand::random::<u64>()).to_string();
            let mut ws_stream = accept_async(stream).await.expect("Error during WS handshake");
            println!("[STILLSOUND] Extension Linked: {}", bridge_id);
            app_clone.emit("bridge_linked", ()).unwrap();

            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if msg.is_text() {
                        let text = msg.to_text().unwrap();
                        let event: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
                        
                        if let Some(event_type) = event["type"].as_str() {
                            handle_extension_event(event_type, Some(bridge_id.clone()), &app_clone, &state_clone).await;
                        }
                    }
                }
            }
            
            // Cleanup on disconnect
            println!("[STILLSOUND] Extension Unlinked: {}", bridge_id);
            handle_extension_event("disconnect", Some(bridge_id), &app_clone, &state_clone).await;
        });
    }
}

async fn handle_extension_event<R: Runtime>(event: &str, bridge_id: Option<String>, app: &AppHandle<R>, state: &Arc<Mutex<AppState>>) {
    let (yt_playing, mut device_id, sync_enabled) = {
        let mut s = state.lock().unwrap();
        
        if let Some(bid) = bridge_id {
            match event {
                "video_playing" => { s.active_bridges.insert(bid, true); }
                "video_paused" => { s.active_bridges.insert(bid, false); }
                "disconnect" => { s.active_bridges.remove(&bid); }
                _ => {}
            }
        }

        // Aggregate state: Playing if ANY bridge reports playing
        let aggregate_playing = s.active_bridges.values().any(|&v| v);
        s.yt_playing = aggregate_playing;

        (s.yt_playing, s.current_device_id.clone(), s.sync_enabled)
    };

    app.emit("sync_event", if yt_playing { "yt_playing" } else { "yt_paused" }).unwrap();

    if !sync_enabled { return; }

    // Use state-aware device discovery
    if device_id.is_none() {
        if let Some(found) = spotify_get_active_device(state).await {
            device_id = Some(found.clone());
            let mut s = state.lock().unwrap();
            s.current_device_id = Some(found);
            s.save();
        }
    }

    if yt_playing {
        // Use state-aware playback check
        if let Some(playback) = spotify_get_playback_state(state).await {
            let spotify_is_playing = playback["is_playing"].as_bool().unwrap_or(false);
            
            if spotify_is_playing {
                // Use state-aware control
                if let Ok(_) = spotify_control(state, false, device_id).await {
                    let mut s = state.lock().unwrap();
                    s.app_paused_spotify = true;
                    s.manual_pause_detected = false;
                    println!("[STILLSOUND] Paused Spotify because YouTube started");
                }
            } else {
                let mut s = state.lock().unwrap();
                if !s.app_paused_spotify {
                    s.manual_pause_detected = true;
                    println!("[STILLSOUND] Respecting manual Spotify pause");
                }
            }
        }
    } else {
        let (was_paused_by_app, manual_override) = {
            let s = state.lock().unwrap();
            (s.app_paused_spotify, s.manual_pause_detected)
        };

        if was_paused_by_app && !manual_override {
            // Use state-aware control
            if let Ok(_) = spotify_control(state, true, device_id).await {
                let mut s = state.lock().unwrap();
                s.app_paused_spotify = false;
                println!("[STILLSOUND] Resumed Spotify (auto)");
            }
        } else {
            let mut s = state.lock().unwrap();
            s.app_paused_spotify = false;
            println!("[STILLSOUND] Staying paused (user intent)");
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
                s.save();
                println!("[AUTH] Token refreshed successfully!");
                return Ok(new_at);
            }
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
    
    // Find first active device or just the first one
    devices.iter().find(|d| d["is_active"].as_bool().unwrap_or(false))
        .or(devices.get(0))
        .and_then(|d| d["id"].as_str().map(|s| s.to_string()))
}

async fn spotify_get_active_device(state: &Arc<Mutex<AppState>>) -> Option<String> {
    let token = state.lock().unwrap().access_token.clone()?;
    
    if let Some(device) = spotify_get_active_device_internal(&token).await {
        return Some(device);
    }

    // Try refresh
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

    // Try refresh on failure (might be a 401)
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
    
    // 1. SMART CHECK
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

// --- Tauri Commands ---

#[tauri::command]
fn update_settings(
    app: tauri::AppHandle,
    state: tauri::State<StateWrapper>, 
    sync: bool, 
    vol: u32,
    autostart: bool,
    autostart_minimized: bool,
    minimize_to_tray: bool
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    
    let mut s = state.0.lock().unwrap();
    
    // Handle autostart plugin
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
    s.save();
    Ok(())
}

#[tauri::command]
async fn set_volume(state: tauri::State<'_, StateWrapper>, vol: u32) -> Result<(), String> {
    let device_id = {
        let mut s = state.0.lock().unwrap();
        s.volume = vol;
        s.save();
        s.current_device_id.clone()
    };

    spotify_set_volume(&state.0, vol, device_id).await
}

#[tauri::command]
async fn start_auth(state: tauri::State<'_, StateWrapper>, client_id: String) -> Result<(), String> {
    let mut random_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    {
        let mut s = state.0.lock().unwrap();
        s.client_id = Some(client_id.clone());
        s.code_verifier = Some(verifier);
        s.save();
    }

    let auth_url = format!(
        "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri=http://127.0.0.1:8921/callback&scope=user-modify-playback-state%20user-read-playback-state%20user-read-currently-playing&code_challenge_method=S256&code_challenge={}",
        client_id, challenge
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
fn close_window(window: tauri::Window) {
    window.close().unwrap();
}

#[tauri::command]
fn minimize_window(window: tauri::Window) {
    window.minimize().unwrap();
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
    // Return saved volume as fallback
    let s = state.0.lock().unwrap();
    Ok(s.volume)
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
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
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }))
        .manage(state_wrapper)
        .setup(move |app| {
            // Handle autostart minimized
            let args: Vec<String> = std::env::args().collect();
            if args.contains(&"--minimized".to_string()) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            // Tray Menu
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();
            let show_i = MenuItem::with_id(app, "show", "Show StillSound", true, None::<&str>).unwrap();
            let menu = Menu::with_items(app, &[&show_i, &quit_i]).unwrap();

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("StillSound Studio")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
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
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app).unwrap();

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
            open_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
