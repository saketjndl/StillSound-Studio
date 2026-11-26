/// Microphone / Call Activity Monitor
/// Detects active microphone capture sessions (Zoom, Discord, Google Meet, MS Teams, etc.)
/// and notifies the sync engine to duck or pause music.

use crate::AppState;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Runtime};

#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::core::Interface;
    use windows::Win32::Foundation::{S_FALSE, S_OK};
    use windows::Win32::Media::Audio::{
        eCapture, eCommunications, eConsole, AudioSessionStateActive, IAudioSessionControl2,
        IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    struct ComGuard(bool);

    impl ComGuard {
        fn init() -> Self {
            let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            Self(hr == S_OK || hr == S_FALSE)
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    pub fn is_mic_active() -> bool {
        let _com = ComGuard::init();
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                    Ok(e) => e,
                    Err(_) => return false,
                };

            let endpoints = [eCommunications, eConsole];
            for flow in endpoints {
                if let Ok(device) = enumerator.GetDefaultAudioEndpoint(eCapture, flow) {
                    if let Ok(session_manager) =
                        device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
                    {
                        if let Ok(session_enum) = session_manager.GetSessionEnumerator() {
                            if let Ok(count) = session_enum.GetCount() {
                                for i in 0..count {
                                    if let Ok(control) = session_enum.GetSession(i) {
                                        if let Ok(state) = control.GetState() {
                                            if state == AudioSessionStateActive {
                                                if let Ok(control2) =
                                                    control.cast::<IAudioSessionControl2>()
                                                {
                                                    if let Ok(pid) = control2.GetProcessId() {
                                                        if pid > 4 {
                                                            return true;
                                                        }
                                                    }
                                                } else {
                                                    return true;
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
            false
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

    let mut consecutive_active: u8 = 0;
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
            consecutive_active = 0;
            last_raw_active = false;
            if last_reported_active {
                last_reported_active = false;
                {
                    let mut s = state.lock().unwrap();
                    s.mic_active = false;
                }
                let _ = app.emit("mic_status", serde_json::json!({ "active": false }));
                crate::handle_mic_event(false, &app, &state).await;
            }
            continue;
        }

        // Run COM check on blocking thread pool to keep Tokio runtime responsive
        let raw_active = tokio::task::spawn_blocking(check_mic_active)
            .await
            .unwrap_or(false);

        let now = Instant::now();
        if raw_active {
            consecutive_active = consecutive_active.saturating_add(1);
            last_active_time = now;
        } else {
            consecutive_active = 0;
        }

        // Require 2 consecutive active samples (~1.2s) to activate so wake-up WASAPI blips don't trigger ghost ducks,
        // while keeping the 1.8s grace window after active speech/PTT ends.
        let debounced_active = if last_reported_active {
            raw_active || (last_raw_active && now.duration_since(last_active_time) < debounce_duration)
        } else {
            consecutive_active >= 2
        };
        last_raw_active = raw_active;

        if debounced_active != last_reported_active {
            last_reported_active = debounced_active;

            {
                let mut s = state.lock().unwrap();
                s.mic_active = debounced_active;
            }

            println!("[MIC] Microphone active state changed: {}", debounced_active);
            let _ = app.emit("mic_status", serde_json::json!({ "active": debounced_active }));

            crate::handle_mic_event(debounced_active, &app, &state).await;
        }
    }
}
