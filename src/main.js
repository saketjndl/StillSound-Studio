const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- refs ---
const vSetup = document.getElementById('v-setup');
const vDash = document.getElementById('v-dash');
const vSettings = document.getElementById('v-settings');
const tbDot = document.getElementById('tb-dot');
const clientId = document.getElementById('client-id');
const sDot = document.getElementById('s-dot');
const sSub = document.getElementById('s-sub');

// Music Island Refs
const ambientBg = document.getElementById('ambient-bg');
const albumArt = document.getElementById('album-art');
const trackName = document.getElementById('track-name');
const trackArtist = document.getElementById('track-artist');
const btnPlayPause = document.getElementById('btn-play-pause');
const iconPlay = document.getElementById('icon-play');
const iconPause = document.getElementById('icon-pause');
const btnNext = document.getElementById('btn-next');
const btnPrev = document.getElementById('btn-prev');

const pSpotify = document.getElementById('p-spotify');
const pExt = document.getElementById('p-ext');
const volSlider = document.getElementById('vol-slider');
const volNum = document.getElementById('vol-num');
const syncToggle = document.getElementById('sync-toggle');

const winSettingsBtn = document.getElementById('win-settings');
const settingsBack = document.getElementById('settings-back');
const autostartToggle = document.getElementById('autostart-toggle');
const autostartMinimizedToggle = document.getElementById('autostart-minimized-toggle');
const minimizeToTrayToggle = document.getElementById('minimize-to-tray-toggle');

const GITHUB = 'https://github.com/saketjndl/StillSound-Studio';

const SITE_GROUPS = {
    "Streaming": ["youtube.com", "netflix.com", "primevideo.com", "hotstar.com", "disneyplus.com", "twitch.tv", "crunchyroll.com", "vimeo.com", "jiocinema.com"],
    "Social Media": ["twitter.com", "x.com", "reddit.com", "instagram.com", "facebook.com"],
    "Educational": ["udemy.com", "coursera.org", "khanacademy.org", "nptel.ac.in"],
    "Conferencing": ["meet.google.com", "zoom.us", "teams.microsoft.com"]
};

let enabledSites = {};

// --- volume debounce ---
let volTimer = null;

// --- helpers ---

function goDash() {
    vSetup.style.display = 'none';
    vDash.style.display = 'flex';
    vSettings.style.display = 'none';
    winSettingsBtn.style.display = 'flex';
    tbDot.classList.add('ok');
    pSpotify.classList.add('on');
    pSpotify.textContent = 'connected';

    // Sync volume slider with Spotify's actual volume
    invoke('get_spotify_volume').then((vol) => {
        volSlider.value = vol;
        volNum.textContent = vol + '%';
    }).catch(() => { });
}

function saveSettings() {
    invoke('update_settings', {
        sync: syncToggle.checked,
        vol: parseInt(volSlider.value),
        autostart: autostartToggle.checked,
        autostartMinimized: autostartMinimizedToggle.checked,
        minimizeToTray: minimizeToTrayToggle.checked,
        enabledSites: enabledSites
    }).catch(console.error);
}

function openGithub() {
    invoke('open_url', { url: GITHUB }).catch(() => {
        window.__TAURI__?.shell?.open(GITHUB);
    });
}

// --- window controls ---
document.getElementById('win-min').onclick = () => invoke('minimize_window');
document.getElementById('win-close').onclick = () => invoke('close_window');

function switchView(toSettings) {
    if (toSettings) {
        vDash.classList.add('fade-out');
        setTimeout(() => {
            vDash.style.display = 'none';
            vDash.classList.remove('fade-out');
            vSettings.style.display = 'flex';
            vSettings.classList.add('fade-in');
            setTimeout(() => vSettings.classList.remove('fade-in'), 300);
        }, 300);
        winSettingsBtn.style.display = 'none';
    } else {
        vSettings.classList.add('fade-out');
        setTimeout(() => {
            vSettings.style.display = 'none';
            vSettings.classList.remove('fade-out');
            vDash.style.display = 'flex';
            vDash.classList.add('fade-in');
            setTimeout(() => vDash.classList.remove('fade-in'), 300);
        }, 300);
        winSettingsBtn.style.display = 'block';
    }
}

winSettingsBtn.addEventListener('click', () => switchView(true));
settingsBack.addEventListener('click', () => switchView(false));

// --- spotify dashboard link ---
document.getElementById('open-dash').addEventListener('click', () => {
    invoke('open_url', { url: 'https://developer.spotify.com/dashboard' }).catch(() => {
        window.__TAURI__?.shell?.open('https://developer.spotify.com/dashboard');
    });
});

// --- copy redirect uri ---
document.getElementById('copy-uri').addEventListener('click', function () {
    navigator.clipboard.writeText('http://127.0.0.1:8921/callback').then(() => {
        this.textContent = 'Copied!';
        setTimeout(() => { this.textContent = 'Copy'; }, 1500);
    });
});

// --- github star links ---
document.getElementById('star-setup').addEventListener('click', (e) => {
    e.preventDefault();
    openGithub();
});
document.getElementById('star-dash').addEventListener('click', (e) => {
    e.preventDefault();
    openGithub();
});

// --- extension download link ---
document.getElementById('open-ext-download').addEventListener('click', () => {
    invoke('open_url', { url: GITHUB }).catch(() => {
        window.__TAURI__?.shell?.open(GITHUB);
    });
});
document.getElementById('open-ext-dash').addEventListener('click', () => {
    invoke('open_url', { url: GITHUB }).catch(() => {
        window.__TAURI__?.shell?.open(GITHUB);
    });
});

// --- connect ---
document.getElementById('btn-connect').addEventListener('click', async () => {
    const id = clientId.value.trim();
    if (!id) {
        clientId.classList.add('shake');
        clientId.focus();
        setTimeout(() => clientId.classList.remove('shake'), 500);
        return;
    }

    const btn = document.getElementById('btn-connect');
    const label = btn.querySelector('span');
    const spin = document.getElementById('spin');

    try {
        label.textContent = 'Waiting for Spotify...';
        spin.style.display = 'block';
        btn.disabled = true;
        tbDot.className = 'tb-dot live';

        await invoke('start_auth', { clientId: id });
    } catch (e) {
        label.textContent = 'Failed — try again';
        spin.style.display = 'none';
        btn.disabled = false;
        tbDot.className = 'tb-dot';
        console.error(e);
    }
});

// --- refresh device ---
document.getElementById('refresh-device').addEventListener('click', async () => {
    const btn = document.getElementById('refresh-device');
    const orig = btn.innerHTML;
    btn.textContent = 'Searching...';

    try {
        await invoke('refresh_spotify_device');
        btn.textContent = '✓ Device found';
        pSpotify.classList.add('on');
        pSpotify.textContent = 'connected';
    } catch {
        btn.textContent = 'No device — open Spotify';
        pSpotify.classList.remove('on');
        pSpotify.textContent = 'no device';
    }

    setTimeout(() => { btn.innerHTML = orig; }, 2000);
});

// --- volume (debounced & synced) ---
let isDraggingVolume = false;

['mousedown', 'touchstart'].forEach(evt => 
    volSlider.addEventListener(evt, () => isDraggingVolume = true)
);

['mouseup', 'mouseleave', 'touchend'].forEach(evt => 
    volSlider.addEventListener(evt, () => {
        setTimeout(() => isDraggingVolume = false, 500); // Small delay to prevent jitter
    })
);

volSlider.addEventListener('input', (e) => {
    const v = parseInt(e.target.value);
    volNum.textContent = v + '%';

    clearTimeout(volTimer);
    volTimer = setTimeout(() => {
        invoke('set_volume', { vol: v }).catch(() => { });
    }, 250);
});

let appVersion = '2.0.0'; // Fallback

// --- update check ---
async function checkUpdates(manual = false) {
    const statusText = document.getElementById('update-status');
    const checkBtn = document.getElementById('check-updates');

    if (manual) {
        checkBtn.textContent = 'Checking...';
        checkBtn.disabled = true;
    }

    try {
        const res = await fetch('https://api.github.com/repos/saketjndl/StillSound-Studio/releases/latest');
        if (!res.ok) throw new Error('API down');
        const data = await res.json();
        const latestVersion = data.tag_name.replace('v', '');

        if (latestVersion !== appVersion) {
            statusText.innerHTML = `Update available: <strong>v${latestVersion}</strong>`;
            statusText.style.color = 'var(--orange)';
            
            // Auto-open on manual click, or show download button
            if (manual) {
                // Find installer in assets if possible
                const installer = data.assets.find(a => a.name.endsWith('.exe'));
                const url = installer ? installer.browser_download_url : data.html_url;
                invoke('open_url', { url });
            }
        } else {
            statusText.textContent = 'You are on the latest version';
            statusText.style.color = '';
        }
    } catch (e) {
        console.error('Update check failed:', e);
        if (manual) statusText.textContent = 'Update check failed';
    } finally {
        checkBtn.textContent = manual ? 'Check' : 'Check';
        checkBtn.disabled = false;
    }
}

// --- settings toggles ---
syncToggle.addEventListener('change', saveSettings);
autostartToggle.addEventListener('change', saveSettings);
autostartMinimizedToggle.addEventListener('change', saveSettings);
minimizeToTrayToggle.addEventListener('change', saveSettings);
document.getElementById('check-updates').onclick = () => checkUpdates(true);

// --- init ---
window.addEventListener('DOMContentLoaded', async () => {
    try {
        const s = await invoke('get_initial_state');

        if (s.client_id) clientId.value = s.client_id;
        if (s.volume != null) {
            volSlider.value = s.volume;
            volNum.textContent = s.volume + '%';
        }
        syncToggle.checked = s.sync_enabled !== false;
        autostartToggle.checked = !!s.autostart;
        autostartMinimizedToggle.checked = !!s.autostart_minimized;
        minimizeToTrayToggle.checked = !!s.minimize_to_tray;
        
        if (s.enabled_sites) {
            enabledSites = s.enabled_sites;
        }

        if (s.active_bridges && Object.keys(s.active_bridges).length > 0) {
            pExt.classList.add('on');
            pExt.textContent = 'linked';
            const extHelp = document.getElementById('ext-help');
            if (extHelp) extHelp.style.display = 'none';
        }

        // Render site toggles
        const sitesList = document.getElementById('sites-list');
        if (sitesList) {
            for (const [group, sites] of Object.entries(SITE_GROUPS)) {
                const groupHeader = document.createElement('div');
                groupHeader.style.fontSize = '10px';
                groupHeader.style.textTransform = 'uppercase';
                groupHeader.style.letterSpacing = '0.05em';
                groupHeader.style.color = 'rgba(255, 255, 255, 0.4)';
                groupHeader.style.marginTop = sitesList.children.length > 0 ? '16px' : '0';
                groupHeader.style.marginBottom = '6px';
                groupHeader.textContent = group;
                sitesList.appendChild(groupHeader);

                sites.forEach(site => {
                    const row = document.createElement('div');
                    row.className = 'ctrl-row';
                    row.style.padding = '4px 0';
                    
                    const label = document.createElement('span');
                    label.className = 'ctrl-label';
                    label.style.fontSize = '12px';
                    label.textContent = site;
                    
                    const toggleLabel = document.createElement('label');
                    toggleLabel.className = 'toggle';
                    const checkbox = document.createElement('input');
                    checkbox.type = 'checkbox';
                    // Default to true if not present
                    checkbox.checked = enabledSites[site] !== false;
                    
                    checkbox.addEventListener('change', (e) => {
                        enabledSites[site] = e.target.checked;
                        saveSettings();
                    });
                    
                    const track = document.createElement('span');
                    track.className = 'toggle-track';
                    
                    toggleLabel.appendChild(checkbox);
                    toggleLabel.appendChild(track);
                    
                    row.appendChild(label);
                    row.appendChild(toggleLabel);
                    sitesList.appendChild(row);
                });
            }
        }

        if (s.spotify_ready) goDash();

        // Get actual version from app
        const version = await invoke('get_app_version');
        appVersion = version;
        document.querySelectorAll('.footer-ver').forEach(el => {
            el.textContent = (el.textContent.includes('Studio') ? 'StillSound Studio v' : 'v') + version;
        });

        // Auto check updates on start
        checkUpdates(false);
        
        // Start polling track info
        setInterval(fetchTrackInfo, 2000);
        fetchTrackInfo();
        
    } catch (e) {
        console.error('init error:', e);
    }
});

// --- Dynamic Island Logic ---

let currentTrackName = "";
let isPlaying = false;
let lastVideoState = 'idle';
let isOptimisticUpdate = false;
let optimisticTimeout = null;

async function fetchTrackInfo() {
    try {
        const info = await invoke('get_track_info');
        if (!info || info.type !== 'spotify_state') return;
        
        if (info.track_name && info.track_name !== currentTrackName) {
            currentTrackName = info.track_name;
            trackName.textContent = info.track_name;
            trackArtist.textContent = info.artist_name || 'Unknown Artist';
            if (info.album_art) {
                albumArt.src = info.album_art;
                ambientBg.src = info.album_art;
                albumArt.onload = () => albumArt.classList.add('loaded');
            } else {
                albumArt.classList.remove('loaded');
                ambientBg.src = "";
            }
        }
        
        if (!info.track_name) {
            trackName.textContent = 'Not Playing';
            trackArtist.textContent = 'Open Spotify to see your music';
            albumArt.classList.remove('loaded');
            ambientBg.src = "";
        }
        
        // Update device info and correct wording
        if (lastVideoState === 'playing') {
            sDot.className = 'status-dot-small video';
            sSub.textContent = 'Auto-paused by video';
        } else if (lastVideoState === 'paused') {
            sDot.className = 'status-dot-small music';
            sSub.textContent = info.is_playing ? 'Video paused — enjoy your music' : 'Video paused';
        } else {
            if (info.is_playing) {
                sDot.className = 'status-dot-small music';
                sSub.textContent = info.device_name ? `Playing on ${info.device_name}` : 'Spotify playing';
            } else {
                sDot.className = 'status-dot-small';
                sSub.textContent = info.device_name ? `Connected to ${info.device_name}` : 'Spotify ready';
            }
        }
        
        if (!isOptimisticUpdate && info.is_playing !== isPlaying) {
            isPlaying = info.is_playing;
            if (isPlaying) {
                iconPlay.classList.replace('visible', 'hidden');
                iconPause.classList.replace('hidden', 'visible');
                ambientBg.classList.add('playing');
            } else {
                iconPause.classList.replace('visible', 'hidden');
                iconPlay.classList.replace('hidden', 'visible');
                ambientBg.classList.remove('playing');
            }
        }
        
        // Sync Volume
        if (info.volume_percent !== null && !isDraggingVolume) {
            if (parseInt(volSlider.value) !== info.volume_percent) {
                volSlider.value = info.volume_percent;
                volNum.textContent = info.volume_percent + '%';
            }
        }
    } catch (e) {
        // silently fail on backend errors
    }
}

// Media Controls
btnPlayPause.addEventListener('click', async () => {
    // Optimistic UI update
    isPlaying = !isPlaying;
    
    isOptimisticUpdate = true;
    clearTimeout(optimisticTimeout);
    optimisticTimeout = setTimeout(() => { isOptimisticUpdate = false; }, 3000);
    
    if (isPlaying) {
        iconPlay.classList.replace('visible', 'hidden');
        iconPause.classList.replace('hidden', 'visible');
        ambientBg.classList.add('playing');
    } else {
        iconPause.classList.replace('visible', 'hidden');
        iconPlay.classList.replace('hidden', 'visible');
        ambientBg.classList.remove('playing');
    }
    await invoke('spotify_play_pause').catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

btnNext.addEventListener('click', async () => {
    await invoke('spotify_skip_track', { next: true }).catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

btnPrev.addEventListener('click', async () => {
    await invoke('spotify_skip_track', { next: false }).catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

// --- events ---

listen('auth_success', () => {
    goDash();

    // auto-find device with retry
    const tryRefresh = async (attempts) => {
        for (let i = 0; i < attempts; i++) {
            await new Promise(r => setTimeout(r, 1500));
            try {
                await invoke('refresh_spotify_device');
                pSpotify.classList.add('on');
                pSpotify.textContent = 'connected';
                return;
            } catch { /* retry */ }
        }
    };
    tryRefresh(3);
});

listen('bridge_linked', () => {
    pExt.classList.add('on');
    pExt.textContent = 'linked';
    // Hide extension setup help once connected
    const extHelp = document.getElementById('ext-help');
    if (extHelp) extHelp.style.display = 'none';
});

listen('sync_event', (ev) => {
    if (ev.payload === 'yt_playing') {
        lastVideoState = 'playing';
        sDot.className = 'status-dot-small video';
        sSub.textContent = 'Auto-paused by video';
        fetchTrackInfo();
    } else if (ev.payload === 'yt_paused') {
        lastVideoState = 'paused';
        // The text will be evaluated accurately by fetchTrackInfo() based on isPlaying
        fetchTrackInfo();
    }
});
