// StillSound Background Bridge v3.0
const api = (typeof browser !== 'undefined') ? browser : chrome;
const PORTS = [9876, 9878, 9880, 9882];
let portIndex = 0;
let socket = null;
let reconnectInterval = 3000;
let isConnected = false;

function updateBadge(connected) {
    try {
        if (connected) {
            api.action.setBadgeText({ text: '' });
        } else {
            api.action.setBadgeText({ text: 'OFF' });
            api.action.setBadgeBackgroundColor({ color: '#E53935' });
        }
    } catch (e) {}
}
updateBadge(false);

// Aggregation state: tabId -> { isPlaying: boolean, hostname: string, action: "pause"|"duck"|"ignore" }
const tabStates = new Map();
let aggregateState = 'idle'; // 'pause' | 'duck' | 'idle'
let debounceTimer = null;

// Spotify track info (received from Tauri app via WebSocket)
let spotifyInfo = {
    is_playing: false,
    track_name: '',
    artist_name: '',
    album_art: '',
    album_name: '',
    is_ducked: false,
    ducking_level: 20
};
let spotifyPollTimer = null;

// Site action matrix (configured from Tauri desktop app)
let siteActions = {};

// Match a hostname against the site action matrix
function getSiteAction(hostname) {
    if (!hostname) return 'pause';
    hostname = hostname.replace(/^www\./, '');
    
    // Exact match
    if (siteActions.hasOwnProperty(hostname)) return siteActions[hostname];
    
    // Try parent domain (e.g. m.youtube.com → youtube.com)
    const parts = hostname.split('.');
    if (parts.length > 2) {
        const parent = parts.slice(1).join('.');
        if (siteActions.hasOwnProperty(parent)) return siteActions[parent];
    }
    
    // Default fallback based on domain type
    const socialDomains = ['twitter.com', 'x.com', 'reddit.com', 'instagram.com', 'facebook.com', 'tiktok.com'];
    if (socialDomains.some(d => hostname.includes(d))) {
        return 'duck';
    }
    return 'pause';
}

function connect() {
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
        return;
    }

    const currentUrl = `ws://127.0.0.1:${PORTS[portIndex]}`;

    try {
        console.log('[StillSound] Attempting connection to', currentUrl);
        socket = new WebSocket(currentUrl);
    } catch (e) {
        console.error('[StillSound] Connection error:', e);
        isConnected = false;
        updateBadge(false);
        rotatePortAndRetry();
        return;
    }

    socket.onopen = () => {
        console.log('[StillSound] WebSocket connected successfully on port', PORTS[portIndex]);
        isConnected = true;
        updateBadge(true);
        reconnectInterval = 3000;
        
        // Sync active state on reconnect if anything is currently playing
        if (aggregateState !== 'idle') {
            sendActivePlaybackState();
        }
        startSpotifyPoll();
    };

    socket.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);
            if (data.type === 'spotify_state') {
                spotifyInfo = {
                    is_playing: data.is_playing || false,
                    track_name: data.track_name || '',
                    artist_name: data.artist_name || '',
                    album_art: data.album_art || '',
                    album_name: data.album_name || '',
                    is_ducked: data.is_ducked || false,
                    ducking_level: data.ducking_level || 20
                };
            } else if (data.type === 'site_config') {
                if (data.site_actions) {
                    siteActions = data.site_actions;
                    console.log('[StillSound] Received site action matrix:', Object.keys(siteActions).length, 'sites');
                } else if (data.sites) {
                    // Legacy backwards compatibility
                    siteActions = {};
                    for (const [s, enabled] of Object.entries(data.sites)) {
                        siteActions[s] = enabled ? 'pause' : 'ignore';
                    }
                }
                // Instantly re-evaluate all currently open tabs with the updated site actions
                for (const [tabId, tabState] of tabStates.entries()) {
                    tabState.action = getSiteAction(tabState.hostname);
                }
                updateAggregateState();
            }
        } catch (e) {
            console.warn('[StillSound] Failed to parse WS message:', e);
        }
    };

    socket.onclose = (event) => {
        console.log('[StillSound] WebSocket closed. Code:', event.code, 'Reason:', event.reason);
        isConnected = false;
        updateBadge(false);
        socket = null;
        stopSpotifyPoll();
        rotatePortAndRetry();
    };

    socket.onerror = (err) => {
        console.error('[StillSound] WebSocket error occurred:', err);
        isConnected = false;
        updateBadge(false);
    };
}

function rotatePortAndRetry() {
    portIndex = (portIndex + 1) % PORTS.length;
    setTimeout(connect, reconnectInterval);
    if (reconnectInterval < 10000) reconnectInterval += 1500;
}

function startSpotifyPoll() {
    stopSpotifyPoll();
    spotifyPollTimer = setInterval(() => {
        if (socket && socket.readyState === WebSocket.OPEN) {
            socket.send(JSON.stringify({ type: 'get_spotify_info' }));
        }
    }, 2000);
}

function stopSpotifyPoll() {
    if (spotifyPollTimer) {
        clearInterval(spotifyPollTimer);
        spotifyPollTimer = null;
    }
}

function sendActivePlaybackState() {
    // Determine highest priority active action: 'pause' > 'duck'
    const activeTabs = Array.from(tabStates.values()).filter(t => t.isPlaying);
    const hasPause = activeTabs.some(t => t.action === 'pause');
    const hasDuck = activeTabs.some(t => t.action === 'duck');

    if (hasPause) {
        const pauseTab = activeTabs.find(t => t.action === 'pause');
        sendToApp({ type: 'video_playing', hostname: pauseTab?.hostname || '', action: 'pause' });
    } else if (hasDuck) {
        const duckTab = activeTabs.find(t => t.action === 'duck');
        sendToApp({ type: 'video_playing', hostname: duckTab?.hostname || '', action: 'duck' });
    }
}

function updateAggregateState() {
    const activeTabs = Array.from(tabStates.values()).filter(t => t.isPlaying);
    const hasPause = activeTabs.some(t => t.action === 'pause');
    const hasDuck = activeTabs.some(t => t.action === 'duck');

    let newAggregateState = 'idle';
    if (hasPause) {
        newAggregateState = 'pause';
    } else if (hasDuck) {
        newAggregateState = 'duck';
    }

    if (newAggregateState !== aggregateState) {
        clearTimeout(debounceTimer);
        if (newAggregateState !== 'idle') {
            aggregateState = newAggregateState;
            sendActivePlaybackState();
        } else {
            // Debounce pause transition to prevent stuttering across consecutive videos or ads
            debounceTimer = setTimeout(() => {
                aggregateState = 'idle';
                sendToApp({ type: 'video_paused' });
            }, 800);
        }
    }
}

function sendToApp(payload) {
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(payload));
    }
}

// Keep service worker alive
api.alarms.create('keepalive', { periodInMinutes: 1.0 });
api.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === 'keepalive') {
        if (!socket || socket.readyState !== WebSocket.OPEN) {
            connect();
        }
    }
});

// Clean up when a tab is closed
api.tabs.onRemoved.addListener((tabId) => {
    if (tabStates.has(tabId)) {
        tabStates.delete(tabId);
        updateAggregateState();
    }
});

// Handle messages from content script AND popup
api.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.type === 'get_status') {
        if (!isConnected) {
            connect();
        }
        sendResponse({
            connected: isConnected,
            ytPlaying: aggregateState === 'pause',
            isDucked: aggregateState === 'duck',
            ytDetected: tabStates.size > 0,
            spotify: spotifyInfo
        });
        return;
    }

    if (message.type === 'request_spotify_refresh') {
        if (socket && socket.readyState === WebSocket.OPEN) {
            socket.send(JSON.stringify({ type: 'get_spotify_info' }));
        }
        sendResponse({ ok: true });
        return;
    }

    // Controls from popup
    if (['spotify_play', 'spotify_pause', 'spotify_next', 'spotify_prev'].includes(message.type)) {
        if (socket && socket.readyState === WebSocket.OPEN) {
            socket.send(JSON.stringify({ type: message.type }));
        }
        sendResponse({ ok: true });
        return;
    }

    if (!sender.tab) return;
    const tabId = sender.tab.id;
    const hostname = message.hostname || '';
    const action = getSiteAction(hostname);

    if (action === 'ignore') {
        if (tabStates.has(tabId)) {
            tabStates.delete(tabId);
            updateAggregateState();
        }
        return;
    }

    if (message.type === 'video_playing') {
        tabStates.set(tabId, { isPlaying: true, hostname, action });
        updateAggregateState();
    } else if (message.type === 'video_paused') {
        if (tabStates.has(tabId)) {
            tabStates.set(tabId, { isPlaying: false, hostname, action });
            updateAggregateState();
        }
    }
});

// Automatically re-inject content script into open tabs when extension is installed/reloaded
api.runtime.onInstalled?.addListener(() => {
    const MATCH_PATTERNS = [
        "*://*.youtube.com/*", "*://*.netflix.com/*", "*://*.primevideo.com/*",
        "*://*.hotstar.com/*", "*://*.disneyplus.com/*", "*://*.twitch.tv/*",
        "*://*.crunchyroll.com/*", "*://*.vimeo.com/*", "*://*.jiocinema.com/*",
        "*://*.twitter.com/*", "*://*.x.com/*", "*://*.reddit.com/*",
        "*://*.instagram.com/*", "*://*.facebook.com/*", "*://*.tiktok.com/*",
        "*://*.udemy.com/*", "*://*.coursera.org/*", "*://*.khanacademy.org/*",
        "*://*.nptel.ac.in/*", "*://nptel.ac.in/*", "*://meet.google.com/*",
        "*://*.zoom.us/*", "*://teams.microsoft.com/*"
    ];
    if (api.tabs && api.scripting) {
        api.tabs.query({ url: MATCH_PATTERNS }, (tabs) => {
            if (!tabs) return;
            tabs.forEach(tab => {
                api.scripting.executeScript({
                    target: { tabId: tab.id },
                    files: ['content.js']
                }).catch(() => {});
            });
        });
    }
});

connect();
