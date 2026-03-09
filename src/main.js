const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- refs ---
const vSetup = document.getElementById('v-setup');
const vDash = document.getElementById('v-dash');
const vSettings = document.getElementById('v-settings');
const tbDot = document.getElementById('tb-dot');
const clientId = document.getElementById('client-id');
const sDot = document.getElementById('s-dot');
const sTitle = document.getElementById('s-title');
const sSub = document.getElementById('s-sub');
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
        minimizeToTray: minimizeToTrayToggle.checked
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
winSettingsBtn.onclick = () => {
    vDash.style.display = 'none';
    vSettings.style.display = 'flex';
};
settingsBack.onclick = () => {
    vSettings.style.display = 'none';
    vDash.style.display = 'flex';
};

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

// --- volume (debounced) ---
volSlider.addEventListener('input', (e) => {
    const v = parseInt(e.target.value);
    volNum.textContent = v + '%';

    clearTimeout(volTimer);
    volTimer = setTimeout(() => {
        invoke('set_volume', { vol: v }).catch(() => { });
    }, 250);
});

const CURRENT_VERSION = '1.1.0';

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

        if (latestVersion !== CURRENT_VERSION) {
            statusText.textContent = `New version available: v${latestVersion}`;
            statusText.style.color = 'var(--orange)';
            if (manual) {
                invoke('open_url', { url: data.html_url });
            }
        } else {
            statusText.textContent = 'You are on the latest version';
            statusText.style.color = '';
        }
    } catch (e) {
        statusText.textContent = 'Update check failed';
    } finally {
        checkBtn.textContent = 'Check';
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

        if (s.spotify_ready) goDash();

        // Auto check updates on start
        setTimeout(() => checkUpdates(false), 2000);
    } catch (e) {
        console.error('init error:', e);
    }
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
        sDot.className = 'status-dot-big video';
        sTitle.textContent = 'Watching YouTube';
        sSub.textContent = 'Spotify paused';
    } else if (ev.payload === 'yt_paused') {
        sDot.className = 'status-dot-big music';
        sTitle.textContent = 'Spotify playing';
        sSub.textContent = 'YouTube paused — enjoy your music';
    }
});
