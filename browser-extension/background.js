// StillSound Background Bridge
const SERVER_URL = 'ws://127.0.0.1:9876';
let socket = null;
let reconnectInterval = 3000;
let isConnected = false;
let ytPlaying = false;
let ytDetected = false;

function connect() {
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
        return; // Already connected or connecting
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

// Keep service worker alive — Manifest V3 service workers sleep after 30s of inactivity.
// This alarm fires every 25 seconds to prevent that, keeping the WebSocket alive.
chrome.alarms.create('keepalive', { periodInMinutes: 0.4 });
chrome.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === 'keepalive') {
        if (!socket || socket.readyState !== WebSocket.OPEN) {
            connect();
        }
    }
});

// Handle messages from content script AND popup
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    // Popup asking for status
    if (message.type === 'get_status') {
        sendResponse({ connected: isConnected, ytPlaying, ytDetected });
        return; // Synchronous response, no need for return true
    }

    // Track YouTube state from content script
    if (message.type === 'video_playing') {
        ytPlaying = true;
        ytDetected = true;
    } else if (message.type === 'video_paused') {
        ytPlaying = false;
        ytDetected = true;
    }

    // Forward to native app via WebSocket
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(message));
    }
});

connect();
