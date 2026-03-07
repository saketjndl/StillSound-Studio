const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- refs ---
const vSetup = document.getElementById('v-setup');
const vDash = document.getElementById('v-dash');
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

const GITHUB = 'https://github.com/saketjndl/StillSound-Studio';

// --- volume debounce ---
let volTimer = null;

// --- helpers ---

function goDash() {
    vSetup.style.display = 'none';
    vDash.style.display = 'flex';
    tbDot.classList.add('ok');
    pSpotify.classList.add('on');
    pSpotify.textContent = 'connected';
}

function openGithub() {
    invoke('open_url', { url: GITHUB }).catch(() => {
        window.__TAURI__?.shell?.open(GITHUB);
    });
}

// --- window controls ---
document.getElementById('win-min').onclick = () => invoke('minimize_window');
document.getElementById('win-close').onclick = () => invoke('close_window');

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

// --- sync toggle ---
syncToggle.addEventListener('change', () => {
    invoke('update_settings', {
        sync: syncToggle.checked,
        vol: parseInt(volSlider.value)
    });
});

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

        if (s.spotify_ready) goDash();
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
