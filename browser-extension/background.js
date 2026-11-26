// StillSound Background Bridge v3.0
const isFirefox = typeof browser !== 'undefined';
const api = isFirefox ? browser : chrome;
const PORTS = [9876, 9878, 9880, 9882];
const SOCIAL_DOMAINS = ['twitter.com', 'x.com', 'reddit.com', 'instagram.com', 'facebook.com', 'tiktok.com'];
const SESSION_KEY = 'stillsound_session_v1';

let portIndex = 0;
let socket = null;
let reconnectTimer = null;
let reconnectInterval = 3000;
let isConnected = false;

// Aggregation state: "tabId:frameId" -> { tabId, frameId, isPlaying, hostname, action }
const tabStates = new Map();
let aggregateState = 'idle'; // 'pause' | 'duck' | 'idle'
let debounceTimer = null;

// Track info received from desktop app via WebSocket
let spotifyInfo = {
    is_playing: false,
    track_name: '',
    artist_name: '',
    album_art: '',
    album_name: '',
    is_ducked: false,
    ducking_level: 20
};

// Site action matrix (configured from Tauri desktop app)
let siteActions = {};

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

function matchesDomain(hostname, domain) {
    return hostname === domain || hostname.endsWith('.' + domain);
}

// Match a hostname against the site action matrix
function getSiteAction(hostname) {
    if (!hostname) return 'pause';
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

function queryTabs(queryInfo) {
    return new Promise((resolve) => {
        try {
            if (isFirefox) {
                browser.tabs.query(queryInfo).then(resolve).catch(() => resolve([]));
            } else {
                chrome.tabs.query(queryInfo, (tabs) => {
                    if (chrome.runtime?.lastError) resolve([]);
                    else resolve(tabs || []);
                });
            }
        } catch (e) {
            resolve([]);
        }
    });
}

function sendTabMessage(tabId, payload) {
    try {
        if (isFirefox) {
            const p = browser.tabs.sendMessage(tabId, payload);
            if (p && typeof p.then === 'function') p.catch(() => {});
        } else {
            chrome.tabs.sendMessage(tabId, payload, () => {
                void chrome.runtime?.lastError;
            });
        }
    } catch (e) {}
}

function persistSessionState() {
    try {
        const sessionStore = api.storage?.session;
        if (!sessionStore) return;
        const payload = {
            [SESSION_KEY]: {
                tabStates: Array.from(tabStates.entries()),
                aggregateState,
                siteActions
            }
        };
        const res = sessionStore.set(payload);
        if (res && typeof res.then === 'function') res.catch(() => {});
    } catch (e) {}
}

const sessionReady = (async () => {
    try {
        const sessionStore = api.storage?.session;
        if (!sessionStore) return;
        const data = await new Promise((resolve) => {
            if (isFirefox) {
                sessionStore.get(SESSION_KEY).then(resolve).catch(() => resolve({}));
            } else {
                sessionStore.get(SESSION_KEY, (items) => {
                    if (chrome.runtime?.lastError) resolve({});
                    else resolve(items || {});
                });
            }
        });
        const saved = data?.[SESSION_KEY];
        if (saved) {
            if (Array.isArray(saved.tabStates) && tabStates.size === 0) {
                for (const [k, v] of saved.tabStates) {
                    tabStates.set(k, v);
                }
            }
            if (saved.siteActions && Object.keys(siteActions).length === 0) {
                siteActions = saved.siteActions;
            }
            if (typeof saved.aggregateState === 'string' && aggregateState === 'idle') {
                aggregateState = saved.aggregateState;
            }
        }
    } catch (e) {}
})();

async function pruneStaleTabs() {
    if (tabStates.size === 0) return false;
    const openTabs = await queryTabs({});
    const liveTabIds = new Set(
        openTabs.filter((t) => !t.discarded).map((t) => t.id)
    );
    let changed = false;
    for (const [key, entry] of tabStates.entries()) {
        if (!liveTabIds.has(entry.tabId)) {
            tabStates.delete(key);
            changed = true;
        }
    }
    if (changed) persistSessionState();
    return changed;
}

function removeTabEntries(tabId) {
    let changed = false;
    for (const [key, entry] of tabStates.entries()) {
        if (entry.tabId === tabId) {
            tabStates.delete(key);
            changed = true;
        }
    }
    if (changed) {
        updateAggregateState();
        persistSessionState();
    }
}

function clearReconnectTimer() {
    if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
    }
}

function connect() {
    clearReconnectTimer();
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

    socket.onopen = async () => {
        console.log('[StillSound] WebSocket connected successfully on port', PORTS[portIndex]);
        isConnected = true;
        updateBadge(true);
        reconnectInterval = 3000;

        await sessionReady;
        await pruneStaleTabs();

        // Clear pre-reconnect active states so stale tabs cannot replay ghost video_playing on wake;
        // live tabs will re-report via sync_playback_state below.
        if (debounceTimer) {
            clearTimeout(debounceTimer);
            debounceTimer = null;
        }
        for (const entry of tabStates.values()) {
            entry.isPlaying = false;
        }
        aggregateState = 'idle';
        persistSessionState();

        // Request live status verification from any open matching tabs after reconnect/wake
        const matches = api.runtime.getManifest().content_scripts?.[0]?.matches || [];
        if (matches.length > 0) {
            const matchingTabs = await queryTabs({ url: matches });
            for (const tab of matchingTabs) {
                if (!tab.discarded && tab.id != null) {
                    sendTabMessage(tab.id, { type: 'sync_playback_state' });
                }
            }
        }
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
                    siteActions = {};
                    for (const [s, enabled] of Object.entries(data.sites)) {
                        siteActions[s] = enabled ? 'pause' : 'ignore';
                    }
                }
                for (const tabState of tabStates.values()) {
                    tabState.action = getSiteAction(tabState.hostname);
                }
                persistSessionState();
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
        rotatePortAndRetry();
    };

    socket.onerror = (err) => {
        console.error('[StillSound] WebSocket error occurred:', err);
        isConnected = false;
        updateBadge(false);
    };
}

function rotatePortAndRetry() {
    clearReconnectTimer();
    portIndex = (portIndex + 1) % PORTS.length;
    reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        connect();
    }, reconnectInterval);
    if (reconnectInterval < 10000) reconnectInterval += 1500;
}

function sendActivePlaybackState() {
    const activeTabs = Array.from(tabStates.values()).filter((t) => t.isPlaying);
    const pauseTab = activeTabs.find((t) => t.action === 'pause');
    const duckTab = activeTabs.find((t) => t.action === 'duck');

    if (pauseTab) {
        sendToApp({ type: 'video_playing', hostname: pauseTab.hostname || '', action: 'pause' });
    } else if (duckTab) {
        sendToApp({ type: 'video_playing', hostname: duckTab.hostname || '', action: 'duck' });
    }
}

function updateAggregateState(forceSync = false) {
    const activeTabs = Array.from(tabStates.values()).filter((t) => t.isPlaying);
    const hasPause = activeTabs.some((t) => t.action === 'pause');
    const hasDuck = activeTabs.some((t) => t.action === 'duck');

    let newAggregateState = 'idle';
    if (hasPause) {
        newAggregateState = 'pause';
    } else if (hasDuck) {
        newAggregateState = 'duck';
    }

    if (newAggregateState !== 'idle') {
        if (debounceTimer) {
            clearTimeout(debounceTimer);
            debounceTimer = null;
        }
        if (newAggregateState !== aggregateState || forceSync) {
            aggregateState = newAggregateState;
            persistSessionState();
            sendActivePlaybackState();
        }
    } else {
        if (aggregateState !== 'idle' && !debounceTimer) {
            debounceTimer = setTimeout(() => {
                debounceTimer = null;
                aggregateState = 'idle';
                persistSessionState();
                sendToApp({ type: 'video_paused' });
            }, 600);
        }
    }
}

function sendToApp(payload) {
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(payload));
    }
}

// Keep service worker alive & ensure connection recovers after sleep
api.alarms.create('keepalive', { periodInMinutes: 1.0 });
api.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === 'keepalive') {
        if (!socket || socket.readyState !== WebSocket.OPEN) {
            connect();
        }
    }
});

// Clean up when a tab closes, navigates, reloads, is discarded, or replaced
api.tabs.onRemoved.addListener((tabId) => {
    removeTabEntries(tabId);
});

api.tabs.onUpdated.addListener((tabId, changeInfo) => {
    if (changeInfo.discarded === true || changeInfo.status === 'loading' || changeInfo.url) {
        removeTabEntries(tabId);
    }
});

api.tabs.onReplaced?.addListener((_addedTabId, removedTabId) => {
    removeTabEntries(removedTabId);
});

// Handle messages from content script AND popup
api.runtime.onMessage.addListener((message, sender, sendResponse) => {
    const handleAsync = async () => {
        await sessionReady;

        if (message.type === 'get_status') {
            if (!isConnected) {
                connect();
            }
            return {
                connected: isConnected,
                ytPlaying: aggregateState === 'pause',
                isDucked: aggregateState === 'duck',
                ytDetected: Array.from(tabStates.values()).some((t) => t.action !== 'ignore'),
                spotify: spotifyInfo,
                siteActions
            };
        }

        if (message.type === 'request_spotify_refresh') {
            if (socket && socket.readyState === WebSocket.OPEN) {
                socket.send(JSON.stringify({ type: 'get_spotify_info' }));
            }
            return { ok: true };
        }

        if (['spotify_play', 'spotify_pause', 'spotify_next', 'spotify_prev'].includes(message.type)) {
            if (socket && socket.readyState === WebSocket.OPEN) {
                socket.send(JSON.stringify({ type: message.type }));
            }
            return { ok: true };
        }

        if (!sender.tab) return null;
        const tabId = sender.tab.id;
        const frameId = sender.frameId ?? 0;
        const key = `${tabId}:${frameId}`;
        const hostname = message.hostname || '';
        const action = getSiteAction(hostname);

        if (action === 'ignore') {
            if (tabStates.has(key)) {
                tabStates.delete(key);
                persistSessionState();
                updateAggregateState();
            }
            return null;
        }

        if (message.type === 'video_playing') {
            tabStates.set(key, { tabId, frameId, isPlaying: true, hostname, action });
            persistSessionState();
            updateAggregateState();
        } else if (message.type === 'video_paused') {
            if (tabStates.has(key)) {
                tabStates.set(key, { tabId, frameId, isPlaying: false, hostname, action });
                persistSessionState();
                updateAggregateState();
            }
        }
        return null;
    };

    if (isFirefox) {
        return handleAsync();
    }

    handleAsync().then((resp) => {
        if (resp !== null && resp !== undefined) {
            try { sendResponse(resp); } catch (e) {}
        }
    });
    return message.type === 'get_status' ||
        message.type === 'request_spotify_refresh' ||
        ['spotify_play', 'spotify_pause', 'spotify_next', 'spotify_prev'].includes(message.type);
});

// Automatically re-inject content script into open tabs when extension is installed/reloaded
api.runtime.onInstalled?.addListener(async () => {
    const matches = api.runtime.getManifest().content_scripts?.[0]?.matches || [];
    if (matches.length === 0 || !api.tabs || !api.scripting) return;

    const tabs = await queryTabs({ url: matches });
    for (const tab of tabs) {
        if (tab.id == null || tab.discarded) continue;
        try {
            const p = api.scripting.executeScript({
                target: { tabId: tab.id, allFrames: true },
                files: ['content.js']
            });
            if (p && typeof p.then === 'function') p.catch(() => {});
        } catch (e) {}
    }
});

sessionReady.then(() => {
    connect();
});
