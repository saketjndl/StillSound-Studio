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

function connect() {
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
        return;
    }

    try {
        socket = new WebSocket(SERVER_URL);
    } catch (e) {
        isConnected = false;
        setTimeout(connect, reconnectInterval);
        return;
    }

    socket.onopen = () => {
        console.log('[StillSound] Connected to app');
        isConnected = true;
        reconnectInterval = 3000;
        // Sync current state on connect
        sendToApp(aggregatePlaying ? 'video_playing' : 'video_paused');
    };

    socket.onclose = () => {
        console.log('[StillSound] Disconnected, reconnecting...');
        isConnected = false;
        socket = null;
        setTimeout(connect, reconnectInterval);
        if (reconnectInterval < 15000) reconnectInterval += 2000;
    };

    socket.onerror = () => {
        isConnected = false;
    };
}

function updateAggregateState() {
    const isAnyPlaying = Array.from(tabStates.values()).some(p => p === true);

    if (isAnyPlaying !== aggregatePlaying) {
        // Debounce the transition to avoid flicker during autoplay/buffering/seeking
        // We only debounce the "pause" (resume music) to be safe.
        // Starting a video (pausing music) should be instant.
        if (isAnyPlaying) {
            clearTimeout(debounceTimer);
            aggregatePlaying = true;
            sendToApp('video_playing');
        } else {
            clearTimeout(debounceTimer);
            debounceTimer = setTimeout(() => {
                aggregatePlaying = false;
                sendToApp('video_paused');
            }, 1000); // 1s buffer for autoplay/refresh
        }
    }
}

function sendToApp(type) {
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ type }));
    }
}

// Keep service worker alive
api.alarms.create('keepalive', { periodInMinutes: 0.4 });
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
        sendResponse({
            connected: isConnected,
            ytPlaying: aggregatePlaying,
            ytDetected: tabStates.size > 0
        });
        return;
    }

    if (!sender.tab) return;
    const tabId = sender.tab.id;

    if (message.type === 'video_playing') {
        tabStates.set(tabId, true);
        updateAggregateState();
    } else if (message.type === 'video_paused') {
        tabStates.set(tabId, false);
        updateAggregateState();
    }
});

connect();
