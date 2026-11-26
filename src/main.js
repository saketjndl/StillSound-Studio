const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- refs ---
const vSetup = document.getElementById('v-setup');
const vDash = document.getElementById('v-dash');
const vSettings = document.getElementById('v-settings');
const clientId = document.getElementById('client-id');
const sSub = document.getElementById('s-sub');

// Music Island Refs
const musicSection = document.getElementById('music-section');
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

// Smart Audio & HUD refs
const duckLevelSlider = document.getElementById('duck-level-slider');
const duckLevelNum = document.getElementById('duck-level-num');
const smartRewindToggle = document.getElementById('smart-rewind-toggle');
const micAwareToggle = document.getElementById('mic-aware-toggle');
const micActionRow = document.getElementById('mic-action-row');
const micActionSelector = document.getElementById('mic-action-selector');
const miniHudToggle = document.getElementById('mini-hud-toggle');
const spinArtToggle = document.getElementById('spin-art-toggle');

const GITHUB = 'https://github.com/saketjndl/StillSound-Studio';

const SITE_GROUPS = {
    "Streaming (Auto-Pause)": ["youtube.com", "netflix.com", "primevideo.com", "hotstar.com", "disneyplus.com", "twitch.tv", "crunchyroll.com", "vimeo.com", "jiocinema.com"],
    "Social Feeds (Auto-Duck)": ["twitter.com", "x.com", "reddit.com", "instagram.com", "facebook.com", "tiktok.com"],
    "Educational": ["udemy.com", "coursera.org", "khanacademy.org", "nptel.ac.in"],
    "Conferencing (Calls)": ["meet.google.com", "zoom.us", "teams.microsoft.com"]
};

let siteActions = {};
let currentMode = 'universal';
let isSpotifyReady = false;
let currentMicAction = 'duck';
let smartRewindSeconds = 5;
let activeBridgeCount = 0;

// --- volume & ducking debounce ---
let volTimer = null;
let duckTimer = null;

// --- helpers ---

function updateBridgeStatus(count) {
    activeBridgeCount = Math.max(0, Number(count) || 0);
    const extHelp = document.getElementById('ext-help');

    if (activeBridgeCount > 0) {
        pExt.classList.add('on');
        pExt.textContent = activeBridgeCount > 1 ? `linked (${activeBridgeCount})` : 'linked';
        if (extHelp) extHelp.style.display = 'none';
    } else {
        pExt.classList.remove('on');
        pExt.textContent = 'unlinked';
        if (extHelp) extHelp.style.display = 'block';
    }
}

function updateDashForMode(mode) {
    currentMode = mode;
    const isUniv = mode === 'universal';
    
    const btnUniv = document.getElementById('mode-universal');
    const btnSpot = document.getElementById('mode-spotify');
    if (btnUniv && btnSpot) {
        btnUniv.classList.toggle('active', isUniv);
        btnSpot.classList.toggle('active', !isUniv);
    }

    const volSection = document.getElementById('vol-section');
    if (volSection) volSection.style.display = isUniv ? 'none' : 'block';

    const refreshBtn = document.getElementById('refresh-device');
    if (refreshBtn) refreshBtn.style.display = isUniv ? 'none' : 'flex';

    const mediaConnName = document.getElementById('media-conn-name');
    if (mediaConnName) mediaConnName.textContent = isUniv ? 'Media Player' : 'Spotify';

    if (isUniv) {
        pSpotify.classList.add('on');
        pSpotify.textContent = 'active';
    } else if (isSpotifyReady) {
        pSpotify.classList.add('on');
        pSpotify.textContent = 'connected';
    } else {
        pSpotify.classList.remove('on');
        pSpotify.textContent = 'waiting...';
    }
}

function goDash() {
    vSetup.style.display = 'none';
    vDash.style.display = 'flex';
    vSettings.style.display = 'none';
    winSettingsBtn.style.display = 'flex';

    if (currentMode === 'spotify') {
        invoke('get_spotify_volume').then((vol) => {
            volSlider.value = vol;
            volNum.textContent = vol + '%';
        }).catch(() => { });
    }
    updateDashForMode(currentMode);
}

function openExternalUrl(url) {
    invoke('open_url', { url }).catch(() => {
        window.__TAURI__?.shell?.open(url);
    });
}

function openGithub() {
    openExternalUrl(GITHUB);
}

function openFeedback() {
    const ua = navigator.userAgent || '';
    const osName = ua.includes('Mac') ? 'macOS' : ua.includes('Linux') ? 'Linux' : 'Windows';
    const body = [
        '### Environment',
        `- App Version: v${appVersion}`,
        `- OS: ${osName}`,
        `- Sync Mode: ${currentMode}`,
        `- Browser Extensions Linked: ${activeBridgeCount}`,
        '',
        '### Feedback / Bug Description',
        'Describe what happened or what you would like improved:'
    ].join('\n');
    const url = `${GITHUB}/issues/new?title=${encodeURIComponent(`[v${appVersion}] `)}&body=${encodeURIComponent(body)}`;
    openExternalUrl(url);
}

// --- window controls (Tauri Window API + IPC fallback) ---
window.__minApp = async (e) => {
    if (e) e.stopPropagation();
    try {
        await invoke('minimize_window');
    } catch (err) {
        window.__TAURI__?.window?.getCurrentWindow?.()?.minimize?.();
    }
};

window.__closeApp = async (e) => {
    if (e) e.stopPropagation();
    try {
        await invoke('close_window');
    } catch (err) {
        window.__TAURI__?.window?.getCurrentWindow?.()?.close?.();
    }
};

const minBtn = document.getElementById('win-min');
const closeBtn = document.getElementById('win-close');
if (minBtn) minBtn.onclick = window.__minApp;
if (closeBtn) closeBtn.onclick = window.__closeApp;

function switchView(toSettings) {
    const activeView = vSetup.style.display !== 'none' ? vSetup : vDash;
    if (toSettings) {
        activeView.classList.add('fade-out');
        setTimeout(() => {
            activeView.style.display = 'none';
            activeView.classList.remove('fade-out');
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
            const targetView = (currentMode === 'spotify' && !isSpotifyReady) ? vSetup : vDash;
            targetView.style.display = 'flex';
            targetView.classList.add('fade-in');
            setTimeout(() => {
                targetView.classList.remove('fade-in');
                if (targetView === vDash) updateDashForMode(currentMode);
            }, 300);
        }, 300);
        winSettingsBtn.style.display = 'flex';
    }
}

winSettingsBtn.addEventListener('click', () => switchView(true));
settingsBack.addEventListener('click', () => switchView(false));

// --- Switch to Universal Mode from Setup Screen ---
const btnSwitchUniversal = document.getElementById('btn-switch-universal');
if (btnSwitchUniversal) {
    btnSwitchUniversal.addEventListener('click', async () => {
        await invoke('set_sync_mode', { mode: 'universal' }).catch(console.error);
        currentMode = 'universal';
        updateDashForMode('universal');
        goDash();
    });
}

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
document.getElementById('feedback-setup')?.addEventListener('click', (e) => {
    e.preventDefault();
    openFeedback();
});
document.getElementById('feedback-dash')?.addEventListener('click', (e) => {
    e.preventDefault();
    openFeedback();
});
document.getElementById('open-feedback')?.addEventListener('click', openFeedback);

// --- extension download link ---
document.getElementById('open-ext-download').addEventListener('click', () => {
    openExternalUrl(`${GITHUB}/releases`);
});
document.getElementById('open-ext-dash').addEventListener('click', () => {
    openExternalUrl(`${GITHUB}/releases`);
});

// --- advanced setup toggle ---
const advancedToggle = document.getElementById('advanced-toggle');
const advancedSection = document.getElementById('advanced-section');

advancedToggle.addEventListener('click', () => {
    const isOpen = advancedSection.style.display !== 'none';
    advancedSection.style.display = isOpen ? 'none' : 'block';
    advancedToggle.classList.toggle('open', !isOpen);
});

// --- connect to spotify ---
document.getElementById('btn-connect').addEventListener('click', async () => {
    const customId = clientId.value.trim();
    const effectiveId = customId || null;

    const btn = document.getElementById('btn-connect');
    const label = btn.querySelector('span');
    const spin = document.getElementById('spin');

    try {
        label.textContent = 'Waiting for Spotify...';
        spin.style.display = 'block';
        btn.disabled = true;

        await invoke('start_auth', { clientId: effectiveId });
    } catch (e) {
        console.error('Spotify auth error:', e);
        label.textContent = 'Enter Client ID below ↓';
        spin.style.display = 'none';
        btn.disabled = false;
        
        // Auto-expand advanced section if custom Client ID is needed
        if (advancedSection) {
            advancedSection.style.display = 'block';
            advancedToggle.classList.add('open');
            clientId.focus();
        }
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
        if (currentMode === 'spotify') {
            pSpotify.classList.add('on');
            pSpotify.textContent = 'connected';
        }
    } catch {
        btn.textContent = 'No device — open Spotify';
        if (currentMode === 'spotify') {
            pSpotify.classList.remove('on');
            pSpotify.textContent = 'no device';
        }
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
        setTimeout(() => isDraggingVolume = false, 500);
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

// --- Ducking level slider ---
duckLevelSlider.addEventListener('input', (e) => {
    const v = parseInt(e.target.value);
    duckLevelNum.textContent = v + '%';

    clearTimeout(duckTimer);
    duckTimer = setTimeout(() => {
        saveSettings();
    }, 250);
});

// --- Mic action segmented control ---
micAwareToggle.addEventListener('change', () => {
    if (micActionRow) {
        micActionRow.style.display = micAwareToggle.checked ? 'flex' : 'none';
    }
    saveSettings();
});

if (micActionSelector) {
    micActionSelector.querySelectorAll('.seg-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            micActionSelector.querySelectorAll('.seg-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentMicAction = btn.dataset.action || 'duck';
            saveSettings();
        });
    });
}

// --- Mini-HUD toggle ---
miniHudToggle.addEventListener('change', (e) => {
    invoke('toggle_mini_hud', { show: e.target.checked }).catch(console.error);
    saveSettings();
});
spinArtToggle?.addEventListener('change', saveSettings);

// --- Smart Rewind toggle ---
smartRewindToggle.addEventListener('change', saveSettings);

let appVersion = '3.0.1';
let latestUpdateUrl = '';

function isNewerVersion(latest, current) {
    const a = String(latest || '').replace(/^v/i, '').split('.').map(n => parseInt(n, 10) || 0);
    const b = String(current || '').replace(/^v/i, '').split('.').map(n => parseInt(n, 10) || 0);
    const len = Math.max(a.length, b.length, 3);
    for (let i = 0; i < len; i++) {
        const av = a[i] || 0;
        const bv = b[i] || 0;
        if (av > bv) return true;
        if (av < bv) return false;
    }
    return false;
}

function pickPlatformAsset(assets = [], fallbackUrl = GITHUB) {
    const ua = navigator.userAgent || '';
    let match = null;
    if (ua.includes('Mac')) {
        match = assets.find(a => a.name.endsWith('.dmg'));
    } else if (ua.includes('Linux')) {
        match = assets.find(a => a.name.endsWith('.AppImage')) || assets.find(a => a.name.endsWith('.deb'));
    } else {
        match = assets.find(a => a.name.endsWith('.exe')) || assets.find(a => a.name.endsWith('.msi'));
    }
    return match ? match.browser_download_url : fallbackUrl;
}

function showUpdateNotice(latestVersion, downloadUrl) {
    const statusText = document.getElementById('update-status');
    const dashBanner = document.getElementById('dash-update-banner');
    const dashTitle = document.getElementById('dash-update-title');
    if (statusText) {
        statusText.innerHTML = `Update available: <strong>v${latestVersion}</strong>`;
        statusText.style.color = 'var(--orange)';
    }
    latestUpdateUrl = downloadUrl || `${GITHUB}/releases/latest`;
    if (dashBanner && dashTitle && localStorage.getItem('stillsound_dismissed_update') !== latestVersion) {
        dashTitle.textContent = `StillSound v${latestVersion} is available`;
        dashBanner.style.display = 'flex';
    }
}

document.getElementById('dash-update-link')?.addEventListener('click', () => {
    if (latestUpdateUrl) openExternalUrl(latestUpdateUrl);
});
document.getElementById('dash-update-dismiss')?.addEventListener('click', (e) => {
    e.stopPropagation();
    const dashBanner = document.getElementById('dash-update-banner');
    if (dashBanner) dashBanner.style.display = 'none';
    const match = (document.getElementById('dash-update-title')?.textContent || '').match(/v(\d+\.\d+\.\d+)/);
    if (match) localStorage.setItem('stillsound_dismissed_update', match[1]);
});

function showWhatsNewIfNeeded(version) {
    const seen = localStorage.getItem('stillsound_seen_version');
    const banner = document.getElementById('whats-new-banner');
    const titleEl = document.getElementById('whats-new-title');
    if (!banner || !titleEl) return;
    if (seen && seen !== version) {
        titleEl.textContent = `Updated to StillSound v${version}`;
        banner.style.display = 'flex';
    } else if (!seen) {
        localStorage.setItem('stillsound_seen_version', version);
    }
}

document.getElementById('whats-new-link')?.addEventListener('click', () => {
    localStorage.setItem('stillsound_seen_version', appVersion);
    const banner = document.getElementById('whats-new-banner');
    if (banner) banner.style.display = 'none';
    openExternalUrl(`${GITHUB}/releases/tag/v${appVersion}`);
});
document.getElementById('whats-new-dismiss')?.addEventListener('click', (e) => {
    e.stopPropagation();
    localStorage.setItem('stillsound_seen_version', appVersion);
    const banner = document.getElementById('whats-new-banner');
    if (banner) banner.style.display = 'none';
});

// --- update check (throttled every 12h unless manual) ---
async function checkUpdates(manual = false) {
    const statusText = document.getElementById('update-status');
    const checkBtn = document.getElementById('check-updates');

    if (!manual) {
        try {
            const cached = JSON.parse(localStorage.getItem('stillsound_update_cache') || 'null');
            if (cached && (Date.now() - cached.checkedAt) < 12 * 60 * 60 * 1000) {
                if (isNewerVersion(cached.version, appVersion)) {
                    showUpdateNotice(cached.version, cached.url);
                }
                return;
            }
        } catch (e) {}
    }

    if (manual) {
        checkBtn.textContent = 'Checking...';
        checkBtn.disabled = true;
    }

    try {
        const res = await fetch('https://api.github.com/repos/saketjndl/StillSound-Studio/releases/latest');
        if (!res.ok) throw new Error('API down');
        const data = await res.json();
        const latestVersion = (data.tag_name || '').replace(/^v/i, '');
        const assetUrl = pickPlatformAsset(data.assets || [], data.html_url);

        try {
            localStorage.setItem('stillsound_update_cache', JSON.stringify({
                version: latestVersion,
                url: assetUrl,
                checkedAt: Date.now()
            }));
        } catch (e) {}

        if (isNewerVersion(latestVersion, appVersion)) {
            showUpdateNotice(latestVersion, assetUrl);
            if (manual && assetUrl) {
                openExternalUrl(assetUrl);
            }
        } else {
            statusText.textContent = 'You are on the latest version';
            statusText.style.color = '';
        }
    } catch (e) {
        console.error('Update check failed:', e);
        if (manual) statusText.textContent = 'Update check failed';
    } finally {
        checkBtn.textContent = 'Check';
        checkBtn.disabled = false;
    }
}

// --- settings toggles ---
document.getElementById('mode-universal').addEventListener('click', () => {
    updateDashForMode('universal');
    invoke('set_sync_mode', { mode: 'universal' }).catch(console.error);
});
document.getElementById('mode-spotify').addEventListener('click', () => {
    updateDashForMode('spotify');
    invoke('set_sync_mode', { mode: 'spotify' }).catch(console.error);
});
syncToggle.addEventListener('change', saveSettings);
autostartToggle.addEventListener('change', saveSettings);
autostartMinimizedToggle.addEventListener('change', saveSettings);
minimizeToTrayToggle.addEventListener('change', saveSettings);
document.getElementById('check-updates').onclick = () => checkUpdates(true);

// --- Save settings helper ---
async function saveSettings() {
    try {
        await invoke('update_settings', {
            sync: syncToggle.checked,
            vol: parseInt(volSlider.value) || 50,
            autostart: autostartToggle.checked,
            autostartMinimized: autostartMinimizedToggle.checked,
            minimizeToTray: minimizeToTrayToggle.checked,
            siteActions: siteActions,
            duckingLevel: parseInt(duckLevelSlider.value) || 20,
            smartRewindEnabled: smartRewindToggle.checked,
            smartRewindSeconds: smartRewindSeconds,
            micAwareEnabled: micAwareToggle.checked,
            micAction: currentMicAction,
            miniHudEnabled: miniHudToggle.checked,
            albumArtSpin: spinArtToggle ? spinArtToggle.checked : true
        });
        console.log('[STILLSOUND] Settings saved successfully:', { siteActions, duckingLevel: duckLevelSlider.value });
    } catch (e) {
        console.error('[STILLSOUND] Failed to save settings:', e);
    }
}

// --- render site action matrix ---
function renderSiteMatrix() {
    const sitesList = document.getElementById('sites-list');
    if (!sitesList) return;
    sitesList.innerHTML = '';

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
            row.className = 'site-item';
            
            const label = document.createElement('span');
            label.className = 'site-name';
            label.textContent = site;
            
            const actionGroup = document.createElement('div');
            actionGroup.className = 'site-actions-group';

            const defaultAction = group.includes("Social") ? "duck" : "pause";
            const currentAction = siteActions[site] || defaultAction;

            const actions = [
                { key: 'pause', label: 'Pause' },
                { key: 'duck', label: 'Duck' },
                { key: 'ignore', label: 'Ignore' }
            ];

            actions.forEach(act => {
                const btn = document.createElement('button');
                btn.className = `site-act-btn ${currentAction === act.key ? 'active' : ''}`;
                btn.dataset.action = act.key;
                btn.textContent = act.label;

                btn.addEventListener('click', async () => {
                    actionGroup.querySelectorAll('.site-act-btn').forEach(b => b.classList.remove('active'));
                    btn.classList.add('active');
                    siteActions[site] = act.key;
                    await invoke('set_site_action', { site: site, action: act.key }).catch(console.error);
                    saveSettings();
                });

                actionGroup.appendChild(btn);
            });
            
            row.appendChild(label);
            row.appendChild(actionGroup);
            sitesList.appendChild(row);
        });
    }
}

// --- init ---
window.addEventListener('DOMContentLoaded', async () => {
    try {
        const s = await invoke('get_initial_state');

        if (s.client_id && s.client_id !== 'PLACEHOLDER_STILLSOUND_CLIENT_ID') {
            clientId.value = s.client_id;
        }
        if (s.volume != null) {
            volSlider.value = s.volume;
            volNum.textContent = s.volume + '%';
        }
        syncToggle.checked = s.sync_enabled !== false;
        autostartToggle.checked = !!s.autostart;
        autostartMinimizedToggle.checked = !!s.autostart_minimized;
        minimizeToTrayToggle.checked = !!s.minimize_to_tray;

        // Smart features init
        if (s.ducking_level != null) {
            duckLevelSlider.value = s.ducking_level;
            duckLevelNum.textContent = s.ducking_level + '%';
        }
        smartRewindToggle.checked = s.smart_rewind_enabled !== false;
        if (s.smart_rewind_seconds != null) {
            smartRewindSeconds = s.smart_rewind_seconds;
        }
        micAwareToggle.checked = s.mic_aware_enabled !== false;
        if (micActionRow) {
            micActionRow.style.display = micAwareToggle.checked ? 'flex' : 'none';
        }
        currentMicAction = s.mic_action || 'duck';
        if (micActionSelector) {
            micActionSelector.querySelectorAll('.seg-btn').forEach(b => {
                b.classList.toggle('active', b.dataset.action === currentMicAction);
            });
        }
        miniHudToggle.checked = !!s.mini_hud_enabled;
        if (spinArtToggle) {
            spinArtToggle.checked = s.album_art_spin !== false;
        }
        
        if (s.site_actions) {
            siteActions = s.site_actions;
        }

        renderSiteMatrix();

        updateBridgeStatus(s.active_bridges ? Object.keys(s.active_bridges).length : 0);

        if (s.spotify_ready) isSpotifyReady = true;
        currentMode = s.sync_mode || 'universal';
        updateDashForMode(currentMode);

        if (isSpotifyReady || currentMode === 'universal') {
            goDash();
        } else {
            winSettingsBtn.style.display = 'flex';
        }

        const version = await invoke('get_app_version');
        appVersion = version;
        document.querySelectorAll('.footer-ver').forEach(el => {
            el.textContent = (el.textContent.includes('Studio') ? 'StillSound Studio v' : 'v') + version;
        });

        showWhatsNewIfNeeded(version);
        checkUpdates(false);
        
        setInterval(fetchTrackInfo, 1500);
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
            currentTrackName = '';
            trackName.textContent = 'Not Playing';
            trackArtist.textContent = currentMode === 'universal'
                ? 'Open Spotify or your media player'
                : 'Open Spotify to see your music';
            albumArt.classList.remove('loaded');
            albumArt.removeAttribute('src');
            ambientBg.removeAttribute('src');
        }

        if (typeof info.active_bridge_count === 'number' && info.active_bridge_count !== activeBridgeCount) {
            updateBridgeStatus(info.active_bridge_count);
        }

        if (!info.yt_playing && !info.app_paused_spotify && lastVideoState === 'playing') {
            lastVideoState = 'idle';
        }
        
        // Dynamic status row with Ducking & Mic awareness
        if (info.is_ducked) {
            musicSection.classList.add('ducked');
            sSub.textContent = `Ducked (${info.ducking_level || 20}%) · Feed Video Active`;
        } else if (info.mic_active) {
            musicSection.classList.remove('ducked');
            sSub.textContent = `Mic Active · Meeting in Progress`;
        } else if (lastVideoState === 'playing' || info.app_paused_spotify) {
            musicSection.classList.remove('ducked');
            sSub.textContent = 'Auto-paused by video';
        } else if (lastVideoState === 'paused') {
            musicSection.classList.remove('ducked');
            sSub.textContent = info.is_playing ? 'Video paused — enjoy your music' : 'Video paused';
        } else {
            musicSection.classList.remove('ducked');
            if (info.is_playing) {
                sSub.textContent = info.device_name ? `Playing on ${info.device_name}` : 'Music playing';
            } else {
                sSub.textContent = info.device_name ? `Connected to ${info.device_name}` : 'Studio ready';
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
        
        // Sync Volume if not dragging
        if (info.volume_percent !== null && !isDraggingVolume) {
            if (parseInt(volSlider.value) !== info.volume_percent) {
                volSlider.value = info.volume_percent;
                volNum.textContent = info.volume_percent + '%';
            }
        }
        
        try {
            const recap = await invoke('get_weekly_recap');
            const elFocus = document.getElementById('recap-focus');
            const elMusic = document.getElementById('recap-music');
            const elSaved = document.getElementById('recap-saved');
            if (elFocus && recap) {
                const fMins = recap.focus_minutes || 0;
                const fHrs = Math.floor(fMins / 60);
                elFocus.textContent = fHrs > 0 ? `${fHrs}h ${fMins % 60}m` : `${fMins}m`;
                
                const mMins = recap.music_minutes || 0;
                const mHrs = Math.floor(mMins / 60);
                elMusic.textContent = mHrs > 0 ? `${mHrs}h ${mMins % 60}m` : `${mMins}m`;
                
                elSaved.textContent = recap.auto_pause_count || 0;
            }
        } catch (err) {}
    } catch (e) {
        // silently fail on backend errors
    }
}

// Media Controls
function debounceButton(btn, delay = 500) {
    if (!btn) return;
    btn.disabled = true;
    btn.style.pointerEvents = 'none';
    btn.style.opacity = '0.6';
    setTimeout(() => {
        btn.disabled = false;
        btn.style.pointerEvents = 'auto';
        btn.style.opacity = '1';
    }, delay);
}

btnPlayPause.addEventListener('click', async () => {
    debounceButton(btnPlayPause, 500);
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
    await invoke('spotify_play_pause', { play: isPlaying }).catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

btnNext.addEventListener('click', async () => {
    debounceButton(btnNext, 500);
    await invoke('spotify_skip_track', { next: true }).catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

btnPrev.addEventListener('click', async () => {
    debounceButton(btnPrev, 500);
    await invoke('spotify_skip_track', { next: false }).catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

// --- events ---

listen('auth_success', () => {
    isSpotifyReady = true;
    goDash();

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

listen('bridge_linked', (ev) => {
    const count = typeof ev.payload === 'number' ? ev.payload : Math.max(1, activeBridgeCount + 1);
    updateBridgeStatus(count);
});

listen('bridge_unlinked', (ev) => {
    const count = typeof ev.payload === 'number' ? ev.payload : 0;
    updateBridgeStatus(count);
});

listen('sync_event', (ev) => {
    if (ev.payload === 'yt_playing') {
        lastVideoState = 'playing';
        sSub.textContent = 'Auto-paused by video';
        fetchTrackInfo();
    } else if (ev.payload === 'yt_paused') {
        lastVideoState = 'paused';
        fetchTrackInfo();
    }
});

listen('mini_hud_toggled', (ev) => {
    if (miniHudToggle) {
        miniHudToggle.checked = !!ev.payload;
    }
});

listen('hud_spin_changed', (ev) => {
    if (spinArtToggle) {
        spinArtToggle.checked = !!ev.payload;
    }
});

listen('mic_status', (ev) => {
    fetchTrackInfo();
});
