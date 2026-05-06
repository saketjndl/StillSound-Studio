const api = (typeof browser !== 'undefined') ? browser : chrome;

// DOM refs
const island = document.getElementById('island');
const ambientBg = document.getElementById('ambient-bg');
const musicSection = document.getElementById('music-section');
const albumArt = document.getElementById('album-art');
const artPlaceholder = document.getElementById('art-placeholder');
const trackName = document.getElementById('track-name');
const trackArtist = document.getElementById('track-artist');
const waveform = document.getElementById('waveform');
const appDot = document.getElementById('app-dot');
const appPill = document.getElementById('app-pill');
const ytDot = document.getElementById('yt-dot');
const ytPill = document.getElementById('yt-pill');
const btnPrev = document.getElementById('btn-prev');
const btnPlayPause = document.getElementById('btn-play-pause');
const iconPlay = document.getElementById('icon-play');
const iconPause = document.getElementById('icon-pause');
const btnNext = document.getElementById('btn-next');

let lastTrackName = '';
let isSpotifyPlaying = false;
let isOptimisticUpdate = false;
let optimisticTimeout = null;

function updatePlayPauseIcon(playing) {
    if (playing) {
        iconPlay.classList.remove('visible');
        iconPlay.classList.add('hidden');
        iconPause.classList.remove('hidden');
        iconPause.classList.add('visible');
    } else {
        iconPause.classList.remove('visible');
        iconPause.classList.add('hidden');
        iconPlay.classList.remove('hidden');
        iconPlay.classList.add('visible');
    }
}

function sendCommand(type) {
    console.log('[StillSound Popup] Sending command:', type);
    try {
        const callback = (res) => {
            const lastError = chrome.runtime.lastError;
            if (lastError) {
                console.error('[StillSound Popup] Command error:', lastError.message);
                return;
            }
        };
        const result = api.runtime.sendMessage({ type }, callback);
        if (result && typeof result.then === 'function') {
            result.catch(() => {});
        }
    } catch (e) {
        console.warn('[StillSound Popup] Error sending command:', e);
    }
}

// --- Control buttons ---
btnPlayPause.addEventListener('click', () => {
    sendCommand(isSpotifyPlaying ? 'spotify_pause' : 'spotify_play');
    // Optimistic UI update with smooth transition
    isSpotifyPlaying = !isSpotifyPlaying;
    
    isOptimisticUpdate = true;
    clearTimeout(optimisticTimeout);
    optimisticTimeout = setTimeout(() => { isOptimisticUpdate = false; }, 3000);
    
    updatePlayPauseIcon(isSpotifyPlaying);
    if (isSpotifyPlaying) {
        waveform.classList.add('active');
        musicSection.classList.add('playing');
        ambientBg.classList.add('playing');
    } else {
        waveform.classList.remove('active');
        musicSection.classList.remove('playing');
        ambientBg.classList.remove('playing');
    }
    setTimeout(() => sendCommand('request_spotify_refresh'), 500);
});

btnPrev.addEventListener('click', () => {
    sendCommand('spotify_prev');
    setTimeout(() => sendCommand('request_spotify_refresh'), 500);
});

btnNext.addEventListener('click', () => {
    sendCommand('spotify_next');
    setTimeout(() => sendCommand('request_spotify_refresh'), 500);
});

function updateUI(res) {
    if (!res) return;

    // --- Connection status ---
    if (res.connected) {
        island.classList.add('connected');
        appDot.classList.add('green');
        appPill.textContent = 'connected';
        appPill.className = 'status-pill ok';
    } else {
        island.classList.remove('connected');
        appDot.classList.remove('green', 'orange');
        appPill.textContent = 'offline';
        appPill.className = 'status-pill off';
    }

    // --- YouTube status ---
    if (res.ytPlaying) {
        ytDot.classList.add('orange');
        ytDot.classList.remove('green');
        ytPill.textContent = 'playing';
        ytPill.className = 'status-pill active';
    } else if (res.ytDetected) {
        ytDot.classList.remove('orange');
        ytDot.classList.add('green');
        ytPill.textContent = 'paused';
        ytPill.className = 'status-pill off';
    } else {
        ytDot.classList.remove('orange', 'green');
        ytPill.textContent = 'idle';
        ytPill.className = 'status-pill off';
    }

    // --- Spotify track info ---
    if (res.spotify) {
        const s = res.spotify;

        if (s.track_name && s.track_name.length > 0) {
            trackName.textContent = s.track_name;

            // Build artist line & device info
            let sub = s.artist_name || '';
            if (s.album_name && s.album_name !== s.track_name) {
                sub += sub ? ' · ' + s.album_name : s.album_name;
            }
            
            if (res.ytPlaying) {
                trackArtist.textContent = sub ? `${sub} (Auto-paused by video)` : 'Auto-paused by video';
            } else if (!s.is_playing && res.ytDetected) {
                trackArtist.textContent = sub ? `${sub} (Video paused)` : 'Video paused';
            } else if (s.device_name) {
                if (s.is_playing) {
                    trackArtist.textContent = sub ? `${sub} • Playing on ${s.device_name}` : `Playing on ${s.device_name}`;
                } else {
                    trackArtist.textContent = sub ? `${sub} • Connected to ${s.device_name}` : `Connected to ${s.device_name}`;
                }
            } else {
                trackArtist.textContent = sub || 'Unknown';
            }

            // Album art
            if (s.album_art) {
                if (albumArt.src !== s.album_art) {
                    albumArt.onload = () => {
                        albumArt.classList.add('loaded');
                    };
                    albumArt.src = s.album_art;
                    ambientBg.src = s.album_art;
                }
            } else {
                albumArt.classList.remove('loaded');
                albumArt.src = '';
                ambientBg.src = '';
            }

            // Play state — update icon smoothly
            if (!isOptimisticUpdate && s.is_playing !== isSpotifyPlaying) {
                isSpotifyPlaying = s.is_playing;
                updatePlayPauseIcon(isSpotifyPlaying);
                
                if (s.is_playing) {
                    waveform.classList.add('active');
                    musicSection.classList.add('playing');
                    ambientBg.classList.add('playing');
                } else {
                    waveform.classList.remove('active');
                    musicSection.classList.remove('playing');
                    ambientBg.classList.remove('playing');
                }
            }

            // Trigger animation on track change
            if (s.track_name !== lastTrackName && lastTrackName !== '') {
                trackName.style.animation = 'artFadeIn 0.3s ease both';
                trackArtist.style.animation = 'artFadeIn 0.3s ease both 0.05s';
                setTimeout(() => {
                    trackName.style.animation = '';
                    trackArtist.style.animation = '';
                }, 400);
            }
            lastTrackName = s.track_name;
        } else {
            // No track info
            trackName.textContent = 'Not Playing';
            trackArtist.textContent = 'Open Spotify to see your music';
            albumArt.classList.remove('loaded');
            albumArt.src = '';
            ambientBg.src = '';
            waveform.classList.remove('active');
            musicSection.classList.remove('playing');
            ambientBg.classList.remove('playing');
            isSpotifyPlaying = false;
            updatePlayPauseIcon(false);
        }
    }
}

function getStatus() {
    try {
        const message = { type: 'get_status' };

        const callback = (res) => {
            const lastError = chrome.runtime.lastError;
            if (lastError) return;
            if (res) updateUI(res);
        };

        const result = api.runtime.sendMessage(message, callback);

        // If it returns a promise (Firefox browser namespace), handle it
        if (result && typeof result.then === 'function') {
            result.then(updateUI).catch(() => {});
        }
    } catch (e) {
        console.warn('[StillSound] Error getting status:', e);
    }
}

// Request a fresh spotify state on popup open
try {
    api.runtime.sendMessage({ type: 'request_spotify_refresh' });
} catch (e) { /* ignore */ }

getStatus();
setInterval(getStatus, 1000);
