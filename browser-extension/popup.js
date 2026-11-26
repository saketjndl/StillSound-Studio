const isFirefox = typeof browser !== 'undefined';
const api = isFirefox ? browser : chrome;
const SOCIAL_DOMAINS = ['twitter.com', 'x.com', 'reddit.com', 'instagram.com', 'facebook.com', 'tiktok.com'];

// DOM refs
const island = document.getElementById('island');
const ambientBg = document.getElementById('ambient-bg');
const musicSection = document.getElementById('music-section');
const albumArt = document.getElementById('album-art');
const trackName = document.getElementById('track-name');
const trackArtist = document.getElementById('track-artist');
const waveform = document.getElementById('waveform');
const appPill = document.getElementById('app-pill');
const ytPill = document.getElementById('yt-pill');
const btnPrev = document.getElementById('btn-prev');
const btnPlayPause = document.getElementById('btn-play-pause');
const iconPlay = document.getElementById('icon-play');
const iconPause = document.getElementById('icon-pause');
const btnNext = document.getElementById('btn-next');
const onboardingBanner = document.getElementById('onboarding-banner');
const controls = document.getElementById('controls');
const permBanner = document.getElementById('perm-banner');
const btnGrantPerms = document.getElementById('btn-grant-perms');

let lastTrackName = '';
let isSpotifyPlaying = false;
let isOptimisticUpdate = false;
let optimisticTimeout = null;
let pollCount = 0;
let currentTabHostname = '';

const siteSeg = document.getElementById('site-seg');
if (siteSeg) {
    siteSeg.querySelectorAll('.site-seg-btn').forEach((btn) => {
        btn.addEventListener('click', () => {
            if (!currentTabHostname) return;
            const action = btn.dataset.action;
            siteSeg.querySelectorAll('.site-seg-btn').forEach((b) => {
                b.classList.toggle('active', b.dataset.action === action);
            });
            sendRuntimeMessage({
                type: 'set_site_action',
                site: currentTabHostname,
                action
            }, () => getStatus());
        });
    });
}

function matchesDomain(hostname, domain) {
    return hostname === domain || hostname.endsWith('.' + domain);
}

function resolveSiteAction(hostname, siteActions = {}) {
    hostname = hostname.toLowerCase().replace(/^www\./, '');
    if (Object.prototype.hasOwnProperty.call(siteActions, hostname)) {
        return siteActions[hostname];
    }
    const parts = hostname.split('.');
    for (let i = 1; i < parts.length - 1; i++) {
        const parent = parts.slice(i).join('.');
        if (Object.prototype.hasOwnProperty.call(siteActions, parent)) {
            return siteActions[parent];
        }
    }
    if (SOCIAL_DOMAINS.some((d) => matchesDomain(hostname, d))) {
        return 'duck';
    }
    return 'pause';
}

function sendRuntimeMessage(payload, onReply) {
    try {
        if (isFirefox) {
            const p = browser.runtime.sendMessage(payload);
            if (p && typeof p.then === 'function') {
                p.then((res) => {
                    if (onReply && res !== undefined) onReply(res);
                }).catch(() => {});
            }
        } else {
            chrome.runtime.sendMessage(payload, (res) => {
                if (chrome.runtime?.lastError) return;
                if (onReply && res !== undefined) onReply(res);
            });
        }
    } catch (e) {
        console.warn('[StillSound Popup] Error sending message:', e);
    }
}

function queryActiveTab(callback) {
    try {
        if (isFirefox) {
            browser.tabs.query({ active: true, currentWindow: true })
                .then((tabs) => callback(tabs || []))
                .catch(() => {});
        } else {
            chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
                if (chrome.runtime?.lastError) return;
                callback(tabs || []);
            });
        }
    } catch (e) {}
}

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
    sendRuntimeMessage({ type });
}

// --- Control buttons ---
btnPlayPause.addEventListener('click', () => {
    sendCommand(isSpotifyPlaying ? 'spotify_pause' : 'spotify_play');
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

    document.body.classList.toggle('no-gradient', res.extGradientEnabled === false);

    // --- Connection status ---
    if (res.connected) {
        island.classList.add('connected');
        appPill.textContent = 'connected';
        appPill.className = 'status-pill ok';
        if (onboardingBanner) onboardingBanner.style.display = 'none';
        if (musicSection) musicSection.style.display = 'flex';
        if (controls) controls.style.display = 'flex';
    } else {
        island.classList.remove('connected');
        appPill.textContent = 'disconnected';
        appPill.className = 'status-pill off';
        if (onboardingBanner) onboardingBanner.style.display = 'flex';
        if (musicSection) musicSection.style.display = 'none';
        if (controls) controls.style.display = 'none';
    }

    // --- Video status ---
    if (res.ytPlaying) {
        ytPill.textContent = 'pausing music';
        ytPill.className = 'status-pill active';
        ytPill.style.background = '';
    } else if (res.isDucked || res.spotify?.is_ducked) {
        ytPill.textContent = `ducking (${res.spotify?.ducking_level || 20}%)`;
        ytPill.className = 'status-pill active';
        ytPill.style.background = '#ffaa00';
    } else if (res.ytDetected) {
        ytPill.textContent = 'ready (video paused)';
        ytPill.className = 'status-pill ok';
        ytPill.style.background = '';
    } else {
        ytPill.textContent = 'monitoring videos';
        ytPill.className = 'status-pill off';
        ytPill.style.background = '';
    }

    // --- Active Tab Site Action Display ---
    queryActiveTab((tabs) => {
        if (tabs[0] && tabs[0].url) {
            try {
                const url = new URL(tabs[0].url);
                if (url.protocol !== 'http:' && url.protocol !== 'https:') return;

                const hostname = url.hostname.replace(/^www\./, '');
                currentTabHostname = hostname;
                const siteRow = document.getElementById('site-action-row');
                const siteLabel = document.getElementById('current-site-label');
                if (siteRow && hostname) {
                    siteRow.style.display = 'flex';
                    siteLabel.textContent = hostname;
                    const action = resolveSiteAction(hostname, res.siteActions || {});
                    if (siteSeg) {
                        siteSeg.querySelectorAll('.site-seg-btn').forEach((btn) => {
                            btn.classList.toggle('active', btn.dataset.action === action);
                        });
                    }
                }
            } catch (e) {}
        }
    });

    // --- Track info ---
    if (res.spotify) {
        const s = res.spotify;

        if (s.track_name && s.track_name.length > 0) {
            trackName.textContent = s.track_name;

            let sub = s.artist_name || '';
            if (s.album_name && s.album_name !== s.track_name) {
                sub += sub ? ' · ' + s.album_name : s.album_name;
            }

            if (res.isDucked || s.is_ducked) {
                trackArtist.textContent = sub ? `${sub} • Ducked (${s.ducking_level || 20}%) for Feed Video` : 'Ducked for Feed Video';
            } else if (res.ytPlaying) {
                trackArtist.textContent = sub ? `${sub} • Paused while watching video` : 'Paused while watching video';
            } else if (!s.is_playing && res.ytDetected) {
                trackArtist.textContent = sub ? `${sub} • Paused` : 'Paused';
            } else if (s.device_name) {
                if (s.is_playing) {
                    trackArtist.textContent = sub ? `${sub} • Playing on ${s.device_name}` : `Playing on ${s.device_name}`;
                } else {
                    trackArtist.textContent = sub ? `${sub} • Connected to ${s.device_name}` : `Connected to ${s.device_name}`;
                }
            } else {
                trackArtist.textContent = sub || 'Unknown';
            }

            if (s.album_art && s.album_art.length > 0) {
                if (albumArt.getAttribute('src') !== s.album_art) {
                    albumArt.src = s.album_art;
                    ambientBg.src = s.album_art;
                }
                albumArt.classList.add('loaded');
            } else {
                albumArt.classList.remove('loaded');
                albumArt.removeAttribute('src');
                ambientBg.removeAttribute('src');
            }

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
            lastTrackName = '';
            trackName.textContent = 'No Active Music Session';
            trackArtist.textContent = 'Play music on Spotify, Apple Music, or VLC';
            albumArt.classList.remove('loaded');
            albumArt.removeAttribute('src');
            ambientBg.removeAttribute('src');
            waveform.classList.remove('active');
            musicSection.classList.remove('playing');
            ambientBg.classList.remove('playing');
            isSpotifyPlaying = false;
            updatePlayPauseIcon(false);
        }
    }
}

async function checkHostPermissions() {
    if (!permBanner || !api.permissions?.contains) return;
    try {
        const hasPerms = await api.permissions.contains({
            origins: ['*://*.youtube.com/*', '*://127.0.0.1/*']
        });
        permBanner.style.display = hasPerms ? 'none' : 'flex';
    } catch (e) {}
}

if (btnGrantPerms) {
    btnGrantPerms.addEventListener('click', async () => {
        if (!api.permissions?.request) return;
        try {
            const manifest = api.runtime.getManifest();
            const origins = manifest.host_permissions || [];
            const granted = await api.permissions.request({ origins });
            if (granted) {
                permBanner.style.display = 'none';
                const matches = manifest.content_scripts?.[0]?.matches || [];
                if (matches.length > 0 && api.tabs?.query && api.scripting?.executeScript) {
                    const tabs = await api.tabs.query({ url: matches });
                    for (const tab of tabs) {
                        if (tab.id != null && !tab.discarded) {
                            api.scripting.executeScript({
                                target: { tabId: tab.id, allFrames: true },
                                files: ['content.js']
                            }).catch(() => {});
                        }
                    }
                }
            }
        } catch (e) {}
    });
}

function getStatus() {
    pollCount += 1;
    if (pollCount % 2 === 1) {
        sendRuntimeMessage({ type: 'request_spotify_refresh' });
    }
    sendRuntimeMessage({ type: 'get_status' }, updateUI);
}

checkHostPermissions();
getStatus();
setInterval(getStatus, 1000);
