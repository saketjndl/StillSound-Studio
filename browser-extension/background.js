// StillSound Background Bridge
const api = (typeof browser !== 'undefined') ? browser : chrome;
const SERVER_URL = 'ws://127.0.0.1:9876';
let socket = null;
let reconnectInterval = 3000;
let isConnected = false;

// Aggregation state
const tabStates = new Map(); // tabId -> isPlaying
let aggregatePlaying = false;
let debounceTimer = null;

// Spotify track info (received from Tauri app via WebSocket)
let spotifyInfo = {
    is_playing: false,
    track_name: '',
    artist_name: '',
    album_art: '',
    album_name: ''
};
let spotifyPollTimer = null;

// Site filtering — default all enabled, overridden by Tauri config
let enabledSites = {};

// Match a hostname against the enabled sites list
function isSiteEnabled(hostname) {
    if (!hostname) return true; // legacy messages without hostname
    hostname = hostname.replace(/^www\./, '');
    // Exact match
    if (enabledSites.hasOwnProperty(hostname)) return enabledSites[hostname] !== false;
    // Try parent domain (e.g. m.youtube.com → youtube.com)
    const parts = hostname.split('.');
    if (parts.length > 2) {
        const parent = parts.slice(1).join('.');
        if (enabledSites.hasOwnProperty(parent)) return enabledSites[parent] !== false;
    }
    // Unknown site — allow by default (content script only runs on registered sites)
    return true;
}

function connect() {
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
        return;
    }

    try {
        console.log('[StillSound] Attempting connection to', SERVER_URL);
        socket = new WebSocket(SERVER_URL);
    } catch (e) {
        console.error('[StillSound] Connection error:', e);
        isConnected = false;
        setTimeout(connect, reconnectInterval);
        return;
    }

    socket.onopen = () => {
        console.log('[StillSound] WebSocket connected successfully');
        isConnected = true;
        reconnectInterval = 3000;
        // Sync current state on connect
        sendToApp(aggregatePlaying ? 'video_playing' : 'video_paused');
        // Start polling for spotify info
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
                    album_name: data.album_name || ''
                };
            } else if (data.type === 'site_config') {
                // Receive enabled sites list from Tauri app
                if (data.sites) {
                    enabledSites = data.sites;
                    console.log('[StillSound] Received site config:', Object.keys(enabledSites).length, 'sites');
                }
            }
        } catch (e) {
            console.warn('[StillSound] Failed to parse WS message:', e);
        }
    };

    socket.onclose = (event) => {
        console.log('[StillSound] WebSocket closed. Code:', event.code, 'Reason:', event.reason);
        isConnected = false;
        socket = null;
        stopSpotifyPoll();
        setTimeout(connect, reconnectInterval);
        if (reconnectInterval < 15000) reconnectInterval += 2000;
    };

    socket.onerror = (err) => {
        console.error('[StillSound] WebSocket error occurred:', err);
        isConnected = false;
    };
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

function updateAggregateState() {
    const isAnyPlaying = Array.from(tabStates.values()).some(p => p === true);

    if (isAnyPlaying !== aggregatePlaying) {
        if (isAnyPlaying) {
            clearTimeout(debounceTimer);
            aggregatePlaying = true;
            sendToApp('video_playing');
        } else {
            clearTimeout(debounceTimer);
            debounceTimer = setTimeout(() => {
                aggregatePlaying = false;
                sendToApp('video_paused');
            }, 1000);
        }
    }
}

function sendToApp(type) {
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ type }));
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
            ytPlaying: aggregatePlaying,
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

    // Spotify control commands from popup
    if (['spotify_play', 'spotify_pause', 'spotify_next', 'spotify_prev'].includes(message.type)) {
        if (socket && socket.readyState === WebSocket.OPEN) {
            socket.send(JSON.stringify({ type: message.type }));
        }
        sendResponse({ ok: true });
        return;
    }

    if (!sender.tab) return;
    const tabId = sender.tab.id;

    if (message.type === 'video_playing') {
        // Check if site is enabled before tracking
        if (!isSiteEnabled(message.hostname)) {
            console.log('[StillSound] Site disabled, ignoring:', message.hostname);
            return;
        }
        tabStates.set(tabId, true);
        updateAggregateState();
    } else if (message.type === 'video_paused') {
        if (!isSiteEnabled(message.hostname)) return;
        tabStates.set(tabId, false);
        updateAggregateState();
    }
});

connect();
