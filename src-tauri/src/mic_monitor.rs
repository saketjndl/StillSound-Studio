/// Microphone / Call Activity Monitor
/// Detects active microphone capture sessions (Zoom, Discord, Google Meet, MS Teams, etc.)
/// and notifies the sync engine to duck or pause music.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::AppState;
use tauri::{AppHandle, Emitter, Runtime};

#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::Media::Audio::{
        MMDeviceEnumerator, IMMDeviceEnumerator, eCapture, eCommunications, eConsole,
        IAudioSessionManager2, IAudioSessionControl2, AudioSessionStateActive
    };

    pub fn is_mic_active() -> bool {
        unsafe {
            // Initialize COM for this thread
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let enumerator: Result<IMMDeviceEnumerator, _> = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL);
            let enumerator = match enumerator {
                Ok(e) => e,
                Err(_) => {
                    CoUninitialize();
                    return false;
                }
            };

            // Check both communications capture endpoint and default console capture endpoint
            let endpoints = [eCommunications, eConsole];
            let mut active = false;

            for flow in endpoints {
                if let Ok(device) = enumerator.GetDefaultAudioEndpoint(eCapture, flow) {
                    if let Ok(session_manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
                        if let Ok(session_enum) = session_manager.GetSessionEnumerator() {
                            if let Ok(count) = session_enum.GetCount() {
                                for i in 0..count {
                                    if let Ok(control) = session_enum.GetSession(i) {
                                        if let Ok(state) = control.GetState() {
                                            if state == AudioSessionStateActive {
                                                // Check if session belongs to an active process (not a dead or system ghost session)
                                                if let Ok(control2) = control.cast::<IAudioSessionControl2>() {
                                                    if let Ok(pid) = control2.GetProcessId() {
                                                        if pid > 4 {
                                                            active = true;
                                                            break;
                                                        }
                                                    }
                                                } else {
                                                    active = true;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if active {
                    break;
                }
            }

            CoUninitialize();
            active
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod non_windows_impl {
    pub fn is_mic_active() -> bool {
        false
    }
}

pub fn check_mic_active() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows_impl::is_mic_active()
    }
    #[cfg(not(target_os = "windows"))]
    {
        non_windows_impl::is_mic_active()
    }
}

pub async fn start_mic_monitor<R: Runtime>(app: AppHandle<R>, state: Arc<Mutex<AppState>>) {
    println!("[MIC] Starting Microphone & Call Activity Monitor service...");

    let mut last_raw_active = false;
    let mut last_reported_active = false;
    let mut last_active_time = Instant::now();
    let debounce_duration = Duration::from_millis(1800); // 1.8s grace period for Push-to-Talk and speech pauses

    loop {
        tokio::time::sleep(Duration::from_millis(600)).await;

        let is_enabled = {
            let s = state.lock().unwrap();
            s.mic_aware_enabled
        };

        if !is_enabled {
            if last_reported_active {
                last_reported_active = false;
                let mut s = state.lock().unwrap();
                s.mic_active = false;
                let _ = app.emit("mic_status", serde_json::json!({ "active": false }));
            }
            continue;
        }

        // Run COM check on blocking thread pool to keep Tokio runtime responsive
        let raw_active = tokio::task::spawn_blocking(check_mic_active).await.unwrap_or(false);

        let now = Instant::now();
        if raw_active {
            last_active_time = now;
        }

        // Debounced mic active evaluation
        let debounced_active = raw_active || (last_raw_active && now.duration_since(last_active_time) < debounce_duration);
        last_raw_active = raw_active;

        if debounced_active != last_reported_active {
            last_reported_active = debounced_active;
            
            {
                let mut s = state.lock().unwrap();
                s.mic_active = debounced_active;
            }

            println!("[MIC] Microphone active state changed: {}", debounced_active);
            let _ = app.emit("mic_status", serde_json::json!({ "active": debounced_active }));

            // Trigger sync handler for mic event
            crate::handle_mic_event(debounced_active, &app, &state).await;
        }
    }
}
