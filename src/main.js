const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- Views & Navigation ---
const appTabs = document.getElementById('app-tabs');
const vSetup = document.getElementById('v-setup');
const vDash = document.getElementById('v-dash');
const vRules = document.getElementById('v-rules');
const vSettings = document.getElementById('v-settings');
const winSettingsBtn = document.getElementById('win-settings');

// --- Setup Refs ---
const clientId = document.getElementById('client-id');
const setupStepExt = document.getElementById('setup-step-ext');
const setupExtPill = document.getElementById('setup-ext-pill');
const setupStepPlayer = document.getElementById('setup-step-player');
const setupPlayerPill = document.getElementById('setup-player-pill');
const advancedToggle = document.getElementById('advanced-toggle');
const advancedSection = document.getElementById('advanced-section');

// --- Dashboard Music Island Refs ---
const musicSection = document.getElementById('music-section');
const ambientBg = document.getElementById('ambient-bg');
const albumArt = document.getElementById('album-art');
const trackName = document.getElementById('track-name');
const trackArtist = document.getElementById('track-artist');
const liveStatePill = document.getElementById('live-state-pill');
const sSub = document.getElementById('s-sub');
const btnPlayPause = document.getElementById('btn-play-pause');
const iconPlay = document.getElementById('icon-play');
const iconPause = document.getElementById('icon-pause');
const btnNext = document.getElementById('btn-next');
const btnPrev = document.getElementById('btn-prev');

// --- Console Controls & Snooze Refs ---
const syncToggle = document.getElementById('sync-toggle');
const syncStatusDesc = document.getElementById('sync-status-desc');
const snoozeChips = document.getElementById('snooze-chips');
const snoozeResumeBtn = document.getElementById('snooze-resume-btn');
const dashModePills = document.getElementById('dash-mode-pills');
const volSlider = document.getElementById('vol-slider');
const volNum = document.getElementById('vol-num');
const dashDuckSlider = document.getElementById('dash-duck-slider');
const dashDuckNum = document.getElementById('dash-duck-num');
const pSpotify = document.getElementById('p-spotify');
const pExt = document.getElementById('p-ext');
const extClientsLabel = document.getElementById('ext-clients-label');

// --- Rules Refs ---
const presetBar = document.getElementById('preset-bar');
const customDomainInput = document.getElementById('custom-domain-input');
const btnAddDomain = document.getElementById('btn-add-domain');
const siteSearchInput = document.getElementById('site-search');
const sitesList = document.getElementById('sites-list');

// --- Settings Refs ---
const duckLevelSlider = document.getElementById('duck-level-slider');
const duckLevelNum = document.getElementById('duck-level-num');
const smartRewindToggle = document.getElementById('smart-rewind-toggle');
const rewindConfigRow = document.getElementById('rewind-config-row');
const rewindSecsSelector = document.getElementById('rewind-secs-selector');
const rewindThreshSelector = document.getElementById('rewind-thresh-selector');
const micAwareToggle = document.getElementById('mic-aware-toggle');
const micActionRow = document.getElementById('mic-action-row');
const micActionSelector = document.getElementById('mic-action-selector');
const miniHudToggle = document.getElementById('mini-hud-toggle');
const hudModeSelector = document.getElementById('hud-mode-selector');
const spinArtToggle = document.getElementById('spin-art-toggle');
const hudLockToggle = document.getElementById('hud-lock-toggle');
const autostartToggle = document.getElementById('autostart-toggle');
const autostartMinimizedToggle = document.getElementById('autostart-minimized-toggle');
const minimizeToTrayToggle = document.getElementById('minimize-to-tray-toggle');

const GITHUB = 'https://github.com/saketjndl/StillSound-Studio';

const SITE_GROUPS = {
    "Streaming (Auto-Pause)": ["youtube.com", "netflix.com", "primevideo.com", "hotstar.com", "disneyplus.com", "twitch.tv", "crunchyroll.com", "vimeo.com", "jiocinema.com"],
    "Social Feeds (Auto-Duck)": ["twitter.com", "x.com", "reddit.com", "instagram.com", "facebook.com", "tiktok.com"],
    "Educational": ["udemy.com", "coursera.org", "khanacademy.org", "nptel.ac.in"],
    "Conferencing (Calls)": ["meet.google.com", "zoom.us", "teams.microsoft.com"]
};

const KNOWN_BUILTINS = new Set(Object.values(SITE_GROUPS).flat());

let siteActions = {};
let currentTab = 'setup';
let currentMode = 'universal';
let isSpotifyReady = false;
let currentMicAction = 'duck';
let smartRewindSeconds = 5;
let smartRewindThresholdSecs = 30;
let currentHudMode = 'pinned';
let activeBridgeCount = 0;
let activeBridgeClients = [];
let appVersion = '3.0.1';
let latestUpdateUrl = '';

let volTimer = null;
let duckTimer = null;

// --- Helpers ---

function updateSliderFill(slider) {
    if (!slider) return;
    const min = Number(slider.min || 0);
    const max = Number(slider.max || 100);
    const val = Number(slider.value || min);
    const pct = max > min ? ((val - min) / (max - min)) * 100 : 0;
    slider.style.setProperty('--progress', `${Math.max(0, Math.min(100, pct))}%`);
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
        `- Browser Extensions Linked: ${activeBridgeCount} (${activeBridgeClients.join(', ') || 'none'})`,
        '',
        '### Feedback / Bug Description',
        'Describe what happened or what you would like improved:'
    ].join('\n');
    const url = `${GITHUB}/issues/new?title=${encodeURIComponent(`[v${appVersion}] `)}&body=${encodeURIComponent(body)}`;
    openExternalUrl(url);
}

function normalizeDomain(raw) {
    let val = String(raw || '').trim().toLowerCase();
    val = val.replace(/^https?:\/\//, '');
    val = val.split('/')[0].split('?')[0].split(':')[0];
    val = val.replace(/^www\./, '');
    return /^[a-z0-9.-]+\.[a-z]{2,}$/.test(val) ? val : '';
}

function switchTab(tab) {
    const valid = ['setup', 'console', 'rules', 'settings'];
    if (!valid.includes(tab)) tab = 'console';
    currentTab = tab;

    vSetup.style.display = tab === 'setup' ? 'flex' : 'none';
    vDash.style.display = tab === 'console' ? 'flex' : 'none';
    vRules.style.display = tab === 'rules' ? 'flex' : 'none';
    vSettings.style.display = tab === 'settings' ? 'flex' : 'none';

    const showTabs = tab !== 'setup';
    if (appTabs) {
        appTabs.style.display = showTabs ? 'grid' : 'none';
        appTabs.querySelectorAll('.tab-btn').forEach((btn) => {
            btn.classList.toggle('active', btn.dataset.tab === tab);
        });
    }

    if (tab === 'console') {
        updateDashForMode(currentMode);
    }
}

function updateBridgeStatus(count, clients = []) {
    activeBridgeCount = Math.max(0, Number(count) || 0);
    if (Array.isArray(clients) && clients.length > 0) {
        activeBridgeClients = clients;
    } else if (activeBridgeCount === 0) {
        activeBridgeClients = [];
    }

    const extHelp = document.getElementById('ext-help');
    if (activeBridgeCount > 0) {
        pExt.classList.add('on');
        pExt.textContent = activeBridgeCount > 1 ? `linked (${activeBridgeCount})` : 'linked';
        if (extClientsLabel) {
            extClientsLabel.textContent = activeBridgeClients.length > 0 ? `(${activeBridgeClients.join(' · ')})` : '';
        }
        if (extHelp) extHelp.style.display = 'none';
        if (setupStepExt) setupStepExt.classList.add('on');
        if (setupExtPill) {
            setupExtPill.classList.add('on');
            setupExtPill.textContent = activeBridgeClients[0] || 'linked';
        }
    } else {
        pExt.classList.remove('on');
        pExt.textContent = 'unlinked';
        if (extClientsLabel) extClientsLabel.textContent = '';
        if (extHelp) extHelp.style.display = 'block';
        if (setupStepExt) setupStepExt.classList.remove('on');
        if (setupExtPill) {
            setupExtPill.classList.remove('on');
            setupExtPill.textContent = 'waiting';
        }
    }
}

function syncDuckSliders(val) {
    const clean = Math.max(5, Math.min(60, Number(val) || 20));
    if (duckLevelSlider) {
        duckLevelSlider.value = clean;
        updateSliderFill(duckLevelSlider);
    }
    if (dashDuckSlider) {
        dashDuckSlider.value = clean;
        updateSliderFill(dashDuckSlider);
    }
    if (duckLevelNum) duckLevelNum.textContent = clean + '%';
    if (dashDuckNum) dashDuckNum.textContent = clean + '%';
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

    if (dashModePills) {
        dashModePills.querySelectorAll('.seg-btn').forEach((btn) => {
            btn.classList.toggle('active', btn.dataset.mode === mode);
        });
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
        pSpotify.textContent = 'connect needed';
    }
}

async function setSyncModeAndNavigate(mode) {
    if (mode === 'spotify' && !isSpotifyReady) {
        switchTab('setup');
        return;
    }
    try {
        await invoke('set_sync_mode', { mode });
        updateDashForMode(mode);
        if (mode === 'spotify' && isSpotifyReady) {
            invoke('get_spotify_volume').then((vol) => {
                volSlider.value = vol;
                volNum.textContent = vol + '%';
                updateSliderFill(volSlider);
            }).catch(() => {});
        }
        updateSettingsSummaries();
    } catch (e) {
        if (String(e).includes('AUTH_REQUIRED')) {
            isSpotifyReady = false;
            updateDashForMode('universal');
            switchTab('setup');
        } else {
            console.error(e);
        }
    }
}

function goDash() {
    switchTab('console');
    if (currentMode === 'spotify') {
        invoke('get_spotify_volume').then((vol) => {
            volSlider.value = vol;
            volNum.textContent = vol + '%';
            updateSliderFill(volSlider);
        }).catch(() => {});
    }
}

// --- Window Controls ---
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

document.getElementById('win-min')?.addEventListener('click', window.__minApp);
document.getElementById('win-close')?.addEventListener('click', window.__closeApp);

winSettingsBtn?.addEventListener('click', () => {
    if (currentTab === 'settings') {
        const backTab = (!isSpotifyReady && currentMode === 'spotify') ? 'setup' : 'console';
        switchTab(backTab);
    } else {
        switchTab('settings');
    }
});

if (appTabs) {
    appTabs.querySelectorAll('.tab-btn').forEach((btn) => {
        btn.addEventListener('click', () => switchTab(btn.dataset.tab));
    });
}

// --- Setup Screen Interactions ---
document.getElementById('btn-switch-universal')?.addEventListener('click', async () => {
    await invoke('set_sync_mode', { mode: 'universal' }).catch(console.error);
    currentMode = 'universal';
    goDash();
});

document.getElementById('open-dash')?.addEventListener('click', () => {
    openExternalUrl('https://developer.spotify.com/dashboard');
});

function wireCopyButton(btnId, textToCopy) {
    const btn = document.getElementById(btnId);
    if (!btn) return;
    btn.addEventListener('click', () => {
        navigator.clipboard.writeText(textToCopy).then(() => {
            btn.textContent = 'Copied!';
            setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
        }).catch(() => {});
    });
}
wireCopyButton('copy-uri', 'http://127.0.0.1:8921/callback');
wireCopyButton('copy-obs-uri', 'http://127.0.0.1:9877/overlay');

document.getElementById('star-setup')?.addEventListener('click', (e) => { e.preventDefault(); openGithub(); });
document.getElementById('star-dash')?.addEventListener('click', (e) => { e.preventDefault(); openGithub(); });
document.getElementById('feedback-setup')?.addEventListener('click', (e) => { e.preventDefault(); openFeedback(); });
document.getElementById('feedback-dash')?.addEventListener('click', (e) => { e.preventDefault(); openFeedback(); });
document.getElementById('open-feedback')?.addEventListener('click', openFeedback);
document.getElementById('open-ext-download')?.addEventListener('click', () => openExternalUrl(`${GITHUB}/releases`));
document.getElementById('open-ext-dash')?.addEventListener('click', () => openExternalUrl(`${GITHUB}/releases`));

advancedToggle?.addEventListener('click', () => {
    const isOpen = advancedSection.style.display !== 'none';
    advancedSection.style.display = isOpen ? 'none' : 'block';
    advancedToggle.classList.toggle('open', !isOpen);
});

document.getElementById('btn-connect')?.addEventListener('click', async () => {
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
        if (advancedSection) {
            advancedSection.style.display = 'block';
            advancedToggle.classList.add('open');
            clientId.focus();
        }
    }
});

document.getElementById('refresh-device')?.addEventListener('click', async () => {
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

// --- Volume & Ducking Controls ---
let isDraggingVolume = false;
['mousedown', 'touchstart'].forEach(evt => volSlider?.addEventListener(evt, () => { isDraggingVolume = true; }));
['mouseup', 'mouseleave', 'touchend'].forEach(evt => volSlider?.addEventListener(evt, () => {
    setTimeout(() => { isDraggingVolume = false; }, 500);
}));

volSlider?.addEventListener('input', (e) => {
    const v = parseInt(e.target.value, 10);
    volNum.textContent = v + '%';
    updateSliderFill(volSlider);
    clearTimeout(volTimer);
    volTimer = setTimeout(() => {
        invoke('set_volume', { vol: v }).catch(() => {});
    }, 250);
});

function handleDuckInput(val) {
    syncDuckSliders(val);
    clearTimeout(duckTimer);
    duckTimer = setTimeout(saveSettings, 250);
}
duckLevelSlider?.addEventListener('input', (e) => handleDuckInput(e.target.value));
dashDuckSlider?.addEventListener('input', (e) => handleDuckInput(e.target.value));

// --- Snooze Controls ---
if (snoozeChips) {
    snoozeChips.querySelectorAll('.chip-btn').forEach((btn) => {
        btn.addEventListener('click', async () => {
            if (btn.id === 'snooze-resume-btn') {
                await invoke('snooze_sync', { minutes: 0, untilRestart: false }).catch(console.error);
            } else if (btn.dataset.restart === 'true') {
                await invoke('snooze_sync', { minutes: null, untilRestart: true }).catch(console.error);
            } else {
                const mins = parseInt(btn.dataset.mins || '15', 10);
                await invoke('snooze_sync', { minutes: mins, untilRestart: false }).catch(console.error);
            }
            fetchTrackInfo();
        });
    });
}

// --- Smart Audio & HUD Settings ---
if (dashModePills) {
    dashModePills.querySelectorAll('.seg-btn').forEach((btn) => {
        btn.addEventListener('click', () => setSyncModeAndNavigate(btn.dataset.mode));
    });
}
document.getElementById('mode-universal')?.addEventListener('click', () => setSyncModeAndNavigate('universal'));
document.getElementById('mode-spotify')?.addEventListener('click', () => setSyncModeAndNavigate('spotify'));

function updateRewindUiVisibility() {
    if (rewindConfigRow) {
        rewindConfigRow.style.display = smartRewindToggle.checked ? 'flex' : 'none';
    }
}

smartRewindToggle?.addEventListener('change', () => {
    updateRewindUiVisibility();
    saveSettings();
});

if (rewindSecsSelector) {
    rewindSecsSelector.querySelectorAll('.seg-btn').forEach((btn) => {
        btn.addEventListener('click', () => {
            rewindSecsSelector.querySelectorAll('.seg-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            smartRewindSeconds = parseInt(btn.dataset.secs || '5', 10);
            saveSettings();
        });
    });
}

if (rewindThreshSelector) {
    rewindThreshSelector.querySelectorAll('.seg-btn').forEach((btn) => {
        btn.addEventListener('click', () => {
            rewindThreshSelector.querySelectorAll('.seg-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            smartRewindThresholdSecs = parseInt(btn.dataset.thresh || '30', 10);
            saveSettings();
        });
    });
}

micAwareToggle?.addEventListener('change', () => {
    if (micActionRow) {
        micActionRow.style.display = micAwareToggle.checked ? 'flex' : 'none';
    }
    saveSettings();
});

if (micActionSelector) {
    micActionSelector.querySelectorAll('.seg-btn').forEach((btn) => {
        btn.addEventListener('click', () => {
            micActionSelector.querySelectorAll('.seg-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentMicAction = btn.dataset.action || 'duck';
            saveSettings();
        });
    });
}

miniHudToggle?.addEventListener('change', (e) => {
    invoke('toggle_mini_hud', { show: e.target.checked }).catch(console.error);
    saveSettings();
});
spinArtToggle?.addEventListener('change', saveSettings);
hudLockToggle?.addEventListener('change', saveSettings);

if (hudModeSelector) {
    hudModeSelector.querySelectorAll('.seg-btn').forEach((btn) => {
        btn.addEventListener('click', () => {
            hudModeSelector.querySelectorAll('.seg-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentHudMode = btn.dataset.hudMode || 'pinned';
            saveSettings();
        });
    });
}

syncToggle?.addEventListener('change', () => {
    if (syncToggle.checked) {
        invoke('snooze_sync', { minutes: 0, untilRestart: false }).catch(() => {});
    }
    saveSettings();
});
autostartToggle?.addEventListener('change', saveSettings);
autostartMinimizedToggle?.addEventListener('change', saveSettings);
minimizeToTrayToggle?.addEventListener('change', saveSettings);

// --- Updates & Banners ---
function isNewerVersion(latest, current) {
    const a = String(latest || '').replace(/^v/i, '').split('.').map(n => parseInt(n, 10) || 0);
    const b = String(current || '').replace(/^v/i, '').split('.').map(n => parseInt(n, 10) || 0);
    const len = Math.max(a.length, b.length, 3);
    for (let i = 0; i < len; i++) {
        if ((a[i] || 0) > (b[i] || 0)) return true;
        if ((a[i] || 0) < (b[i] || 0)) return false;
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
document.getElementById('check-updates')?.addEventListener('click', () => checkUpdates(true));

// --- Save Settings ---
function updateSettingsSummaries() {
    const audioEl = document.getElementById('acc-audio-summary');
    const hudEl = document.getElementById('acc-hud-summary');
    const sysEl = document.getElementById('acc-sys-summary');

    if (audioEl) {
        const duckTxt = `Duck ${duckLevelSlider?.value || 20}%`;
        const rewTxt = smartRewindToggle?.checked ? `Rewind ${smartRewindSeconds}s` : 'No Rewind';
        audioEl.textContent = `${duckTxt} · ${rewTxt}`;
    }
    if (hudEl) {
        const onTxt = miniHudToggle?.checked ? 'On' : 'Off';
        const modeTxt = currentHudMode === 'peek' ? 'Peek' : currentHudMode === 'autohide' ? 'Hide' : 'Pinned';
        hudEl.textContent = `${onTxt} · ${modeTxt}`;
    }
    if (sysEl) {
        sysEl.textContent = `${currentMode === 'spotify' ? 'Spotify' : 'Universal'} · v${appVersion}`;
    }
}

function updateHudModeExplain() {
    const el = document.getElementById('hud-mode-explain');
    if (!el) return;
    if (currentHudMode === 'peek') {
        el.textContent = 'Peek: hidden while idle; smoothly glides into view for ~2.8s when pausing, ducking, or switching songs.';
    } else if (currentHudMode === 'autohide') {
        el.textContent = 'Hide: stays tucked off the top edge until you hover the reveal strip.';
    } else {
        el.textContent = 'Pinned: stays visible cleanly on screen and softens gently when idle.';
    }
}

document.getElementById('reset-recap-btn')?.addEventListener('click', async () => {
    await invoke('reset_weekly_recap').catch(console.error);
    fetchTrackInfo();
});

async function saveSettings() {
    updateSettingsSummaries();
    updateHudModeExplain();
    try {
        await invoke('update_settings', {
            sync: syncToggle.checked,
            vol: parseInt(volSlider.value, 10) || 50,
            autostart: autostartToggle.checked,
            autostartMinimized: autostartMinimizedToggle.checked,
            minimizeToTray: minimizeToTrayToggle.checked,
            siteActions: siteActions,
            duckingLevel: parseInt(duckLevelSlider.value, 10) || 20,
            smartRewindEnabled: smartRewindToggle.checked,
            smartRewindSeconds: smartRewindSeconds,
            smartRewindThresholdSecs: smartRewindThresholdSecs,
            micAwareEnabled: micAwareToggle.checked,
            micAction: currentMicAction,
            miniHudEnabled: miniHudToggle.checked,
            albumArtSpin: spinArtToggle ? spinArtToggle.checked : true,
            miniHudMode: currentHudMode,
            miniHudLocked: hudLockToggle ? hudLockToggle.checked : false
        });
    } catch (e) {
        console.error('[STILLSOUND] Failed to save settings:', e);
    }
}

// --- Site Rules Matrix + Presets + Custom Domains ---
function buildSiteRow(site, defaultAction, isCustom) {
    const row = document.createElement('div');
    row.className = 'site-item';

    const leftWrap = document.createElement('div');
    leftWrap.style.display = 'flex';
    leftWrap.style.alignItems = 'center';
    leftWrap.style.gap = '6px';
    leftWrap.style.minWidth = '0';

    const label = document.createElement('span');
    label.className = 'site-name';
    label.textContent = site;
    leftWrap.appendChild(label);

    if (isCustom) {
        const del = document.createElement('button');
        del.className = 'site-del-btn';
        del.type = 'button';
        del.title = 'Remove custom domain';
        del.textContent = '×';
        del.addEventListener('click', async () => {
            delete siteActions[site];
            renderSiteMatrix();
            await invoke('remove_site_action', { site }).catch(console.error);
        });
        leftWrap.appendChild(del);
    }

    const actionGroup = document.createElement('div');
    actionGroup.className = 'site-actions-group';
    const currentAction = siteActions[site] || defaultAction;

    [
        { key: 'pause', label: 'Pause' },
        { key: 'duck', label: 'Duck' },
        { key: 'ignore', label: 'Ignore' }
    ].forEach((act) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = `site-act-btn ${currentAction === act.key ? 'active' : ''}`;
        btn.dataset.action = act.key;
        btn.textContent = act.label;
        btn.addEventListener('click', async () => {
            actionGroup.querySelectorAll('.site-act-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            siteActions[site] = act.key;
            await invoke('set_site_action', { site, action: act.key }).catch(console.error);
        });
        actionGroup.appendChild(btn);
    });

    row.appendChild(leftWrap);
    row.appendChild(actionGroup);
    return row;
}

function renderSiteMatrix() {
    if (!sitesList) return;
    const filter = (siteSearchInput?.value || '').trim().toLowerCase();
    sitesList.innerHTML = '';

    const appendGroup = (title, items, defaultActionForGroup, isCustom) => {
        const filtered = items.filter(s => !filter || s.includes(filter));
        if (filtered.length === 0) return;

        const hdr = document.createElement('div');
        hdr.style.cssText = 'font-size:10px;text-transform:uppercase;letter-spacing:0.05em;color:var(--faint);margin-bottom:6px;';
        hdr.style.marginTop = sitesList.children.length > 0 ? '14px' : '2px';
        hdr.textContent = title;
        sitesList.appendChild(hdr);

        filtered.forEach((site) => {
            sitesList.appendChild(buildSiteRow(site, defaultActionForGroup, isCustom));
        });
    };

    const customSites = Object.keys(siteActions)
        .filter(s => !KNOWN_BUILTINS.has(s))
        .sort();
    appendGroup('Custom Domains', customSites, 'pause', true);

    for (const [group, sites] of Object.entries(SITE_GROUPS)) {
        const defaultAction = group.includes('Social') ? 'duck' : 'pause';
        appendGroup(group, sites, defaultAction, false);
    }
}

siteSearchInput?.addEventListener('input', renderSiteMatrix);

async function addCustomDomain() {
    const clean = normalizeDomain(customDomainInput?.value);
    if (!clean) {
        customDomainInput?.classList.add('shake');
        setTimeout(() => customDomainInput?.classList.remove('shake'), 400);
        return;
    }
    siteActions[clean] = siteActions[clean] || 'pause';
    if (customDomainInput) customDomainInput.value = '';
    renderSiteMatrix();
    await invoke('set_site_action', { site: clean, action: siteActions[clean] }).catch(console.error);
}

btnAddDomain?.addEventListener('click', addCustomDomain);
customDomainInput?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') addCustomDomain();
});

if (presetBar) {
    presetBar.querySelectorAll('.chip-btn').forEach((btn) => {
        btn.addEventListener('click', async () => {
            presetBar.querySelectorAll('.chip-btn').forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            try {
                const updated = await invoke('apply_site_preset', { preset: btn.dataset.preset });
                if (updated) {
                    siteActions = updated;
                    renderSiteMatrix();
                }
            } catch (e) {
                console.error(e);
            }
        });
    });
}

// --- Init ---
window.addEventListener('DOMContentLoaded', async () => {
    try {
        const s = await invoke('get_initial_state');

        if (s.client_id && s.client_id !== 'PLACEHOLDER_STILLSOUND_CLIENT_ID') {
            clientId.value = s.client_id;
        }
        if (s.volume != null) {
            volSlider.value = s.volume;
            volNum.textContent = s.volume + '%';
            updateSliderFill(volSlider);
        }
        syncToggle.checked = s.sync_enabled !== false;
        autostartToggle.checked = !!s.autostart;
        autostartMinimizedToggle.checked = !!s.autostart_minimized;
        minimizeToTrayToggle.checked = !!s.minimize_to_tray;

        syncDuckSliders(s.ducking_level != null ? s.ducking_level : 20);

        smartRewindToggle.checked = s.smart_rewind_enabled !== false;
        smartRewindSeconds = s.smart_rewind_seconds || 5;
        smartRewindThresholdSecs = s.smart_rewind_threshold_secs || 30;
        rewindSecsSelector?.querySelectorAll('.seg-btn').forEach((b) => {
            b.classList.toggle('active', parseInt(b.dataset.secs, 10) === smartRewindSeconds);
        });
        rewindThreshSelector?.querySelectorAll('.seg-btn').forEach((b) => {
            b.classList.toggle('active', parseInt(b.dataset.thresh, 10) === smartRewindThresholdSecs);
        });
        updateRewindUiVisibility();

        micAwareToggle.checked = s.mic_aware_enabled !== false;
        if (micActionRow) micActionRow.style.display = micAwareToggle.checked ? 'flex' : 'none';
        currentMicAction = s.mic_action || 'duck';
        micActionSelector?.querySelectorAll('.seg-btn').forEach((b) => {
            b.classList.toggle('active', b.dataset.action === currentMicAction);
        });

        miniHudToggle.checked = !!s.mini_hud_enabled;
        if (spinArtToggle) spinArtToggle.checked = s.album_art_spin !== false;
        if (hudLockToggle) hudLockToggle.checked = !!s.mini_hud_locked;
        currentHudMode = s.mini_hud_mode || 'pinned';
        hudModeSelector?.querySelectorAll('.seg-btn').forEach((b) => {
            b.classList.toggle('active', b.dataset.hudMode === currentHudMode);
        });

        if (s.site_actions) siteActions = s.site_actions;
        renderSiteMatrix();

        updateBridgeStatus(s.active_bridges ? Object.keys(s.active_bridges).length : 0);

        if (s.spotify_ready) isSpotifyReady = true;
        currentMode = s.sync_mode || 'universal';
        updateDashForMode(currentMode);

        if (isSpotifyReady || currentMode === 'universal') {
            goDash();
        } else {
            switchTab('setup');
        }

        const version = await invoke('get_app_version');
        appVersion = version;
        document.querySelectorAll('.footer-ver').forEach((el) => {
            el.textContent = (el.textContent.includes('Studio') ? 'StillSound Studio v' : 'v') + version;
        });

        updateSettingsSummaries();
        updateHudModeExplain();
        showWhatsNewIfNeeded(version);
        checkUpdates(false);
        setInterval(fetchTrackInfo, 1500);
        fetchTrackInfo();
    } catch (e) {
        console.error('init error:', e);
    }
});

// --- Dynamic Island & Live Sync Polling ---
let currentTrackName = '';
let isPlaying = false;
let lastVideoState = 'idle';
let isOptimisticUpdate = false;
let optimisticTimeout = null;

function setLiveBadge(text, cls) {
    if (!liveStatePill) return;
    if (!text) {
        liveStatePill.style.display = 'none';
        liveStatePill.className = 'state-chip';
        liveStatePill.textContent = '';
        return;
    }
    liveStatePill.style.display = 'inline-block';
    liveStatePill.className = `state-chip ${cls || ''}`.trim();
    liveStatePill.textContent = text;
}

function updateSnoozeUi(info) {
    const secs = info.snooze_remaining_secs;
    const indef = !!info.snooze_indefinite;
    const effective = info.sync_effective !== false;

    if (snoozeResumeBtn) {
        snoozeResumeBtn.style.display = (secs || indef) ? 'inline-block' : 'none';
    }

    if (snoozeChips) {
        snoozeChips.querySelectorAll('.chip-btn').forEach((b) => {
            if (b.dataset.restart === 'true') {
                b.classList.toggle('active', indef);
            } else if (b.dataset.mins) {
                const m = parseInt(b.dataset.mins, 10);
                const approxMatch = secs && !indef && (m === 15 ? secs <= 900 : secs > 900);
                b.classList.toggle('active', !!approxMatch);
            }
        });
    }

    if (syncStatusDesc) {
        if (!syncToggle.checked) {
            syncStatusDesc.textContent = 'Turned off manually';
        } else if (indef) {
            syncStatusDesc.textContent = 'Snoozed until restart';
        } else if (secs) {
            const mins = Math.floor(secs / 60);
            const remS = secs % 60;
            syncStatusDesc.textContent = mins > 0 ? `Snoozed · ${mins}m ${remS}s left` : `Snoozed · ${remS}s left`;
        } else {
            syncStatusDesc.textContent = 'Monitoring videos & calls';
        }
    }
    return effective;
}

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
                albumArt.removeAttribute('src');
                ambientBg.removeAttribute('src');
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

        if (setupStepPlayer && setupPlayerPill) {
            const hasTrack = !!(info.track_name && info.track_name.length > 0);
            setupStepPlayer.classList.toggle('on', hasTrack);
            setupPlayerPill.classList.toggle('on', hasTrack);
            setupPlayerPill.textContent = hasTrack ? (info.device_name || 'detected') : 'standby';
        }

        if (typeof info.spotify_ready === 'boolean') {
            isSpotifyReady = info.spotify_ready;
        }
        if (typeof info.sync_mode === 'string' && info.sync_mode !== currentMode) {
            updateDashForMode(info.sync_mode);
            updateSettingsSummaries();
        }

        if (typeof info.active_bridge_count === 'number') {
            updateBridgeStatus(info.active_bridge_count, info.bridge_clients || []);
        }

        if (!info.yt_playing && !info.app_paused_spotify && lastVideoState === 'playing') {
            lastVideoState = 'idle';
        }

        const syncEffective = updateSnoozeUi(info);
        musicSection.classList.remove('is-playing', 'paused', 'ducked', 'snoozed');

        const hostLabel = info.active_video_host ? ` · ${info.active_video_host}` : '';
        if (!syncEffective) {
            musicSection.classList.add('snoozed');
            setLiveBadge('SNOOZED', 'snoozed');
            sSub.textContent = 'Auto-sync temporarily paused';
        } else if (info.is_ducked) {
            musicSection.classList.add('ducked');
            setLiveBadge(`DUCK ${info.ducking_level || 20}%`, 'ducked');
            sSub.textContent = info.active_video_host ? `Feed video${hostLabel}` : 'Lowered for feed video';
        } else if (info.mic_active) {
            musicSection.classList.add('is-playing');
            setLiveBadge('MIC ACTIVE', 'mic');
            sSub.textContent = 'Call / microphone detected';
        } else if (lastVideoState === 'playing' || info.app_paused_spotify) {
            musicSection.classList.add('paused');
            setLiveBadge('PAUSED', 'paused');
            sSub.textContent = info.active_video_host ? `Watching${hostLabel}` : 'Auto-paused by video';
        } else if (lastVideoState === 'paused') {
            if (info.is_playing) musicSection.classList.add('is-playing');
            setLiveBadge('');
            sSub.textContent = info.is_playing ? 'Video paused — enjoy your music' : 'Video paused';
        } else {
            if (info.is_playing) musicSection.classList.add('is-playing');
            setLiveBadge('');
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

        if (info.volume_percent !== null && !isDraggingVolume) {
            if (parseInt(volSlider.value, 10) !== info.volume_percent) {
                volSlider.value = info.volume_percent;
                volNum.textContent = info.volume_percent + '%';
                updateSliderFill(volSlider);
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
    } catch (e) {}
}

// --- Media Controls ---
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

btnPlayPause?.addEventListener('click', async () => {
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

btnNext?.addEventListener('click', async () => {
    debounceButton(btnNext, 500);
    await invoke('spotify_skip_track', { next: true }).catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

btnPrev?.addEventListener('click', async () => {
    debounceButton(btnPrev, 500);
    await invoke('spotify_skip_track', { next: false }).catch(console.error);
    setTimeout(fetchTrackInfo, 500);
});

// --- Tauri Live Events ---
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
            } catch {}
        }
    };
    tryRefresh(3);
});

listen('bridge_linked', (ev) => {
    const count = typeof ev.payload === 'number' ? ev.payload : Math.max(1, activeBridgeCount + 1);
    updateBridgeStatus(count);
    fetchTrackInfo();
});

listen('bridge_unlinked', (ev) => {
    const count = typeof ev.payload === 'number' ? ev.payload : 0;
    updateBridgeStatus(count);
    fetchTrackInfo();
});

listen('sync_event', (ev) => {
    if (ev.payload === 'yt_playing') {
        lastVideoState = 'playing';
        fetchTrackInfo();
    } else if (ev.payload === 'yt_paused') {
        lastVideoState = 'paused';
        fetchTrackInfo();
    }
});

listen('site_actions_updated', (ev) => {
    if (ev.payload && typeof ev.payload === 'object') {
        siteActions = ev.payload;
        renderSiteMatrix();
    }
});

listen('mini_hud_toggled', (ev) => {
    if (miniHudToggle) miniHudToggle.checked = !!ev.payload;
});

listen('hud_spin_changed', (ev) => {
    if (spinArtToggle) spinArtToggle.checked = !!ev.payload;
});

listen('hud_prefs_changed', (ev) => {
    const p = ev.payload || {};
    if (typeof p.album_art_spin === 'boolean' && spinArtToggle) {
        spinArtToggle.checked = p.album_art_spin;
    }
    if (typeof p.mini_hud_locked === 'boolean' && hudLockToggle) {
        hudLockToggle.checked = p.mini_hud_locked;
    }
    if (typeof p.mini_hud_mode === 'string' && hudModeSelector) {
        currentHudMode = p.mini_hud_mode;
        hudModeSelector.querySelectorAll('.seg-btn').forEach((b) => {
            b.classList.toggle('active', b.dataset.hudMode === currentHudMode);
        });
        updateSettingsSummaries();
        updateHudModeExplain();
    }
});

listen('mic_status', () => {
    fetchTrackInfo();
});
