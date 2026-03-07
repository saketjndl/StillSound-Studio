// StillSound Background Bridge
const SERVER_URL = 'ws://127.0.0.1:9876';
let socket = null;
let reconnectInterval = 3000;
let isConnected = false;
let ytPlaying = false;
let ytDetected = false;

function connect() {
    socket = new WebSocket(SERVER_URL);

    socket.onopen = () => {
        console.log('[StillSound] Connected to app');
        isConnected = true;
        reconnectInterval = 3000;
    };

    socket.onclose = () => {
        isConnected = false;
        setTimeout(connect, reconnectInterval);
        if (reconnectInterval < 30000) reconnectInterval += 5000;
    };

    socket.onerror = () => {
        // Server not available — will reconnect via onclose
    };
}

// Forward content script messages to the native app
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    // Handle status request from popup
    if (message.type === 'get_status') {
        sendResponse({ connected: isConnected, ytPlaying, ytDetected });
        return true;
    }

    // Track YouTube state
    if (message.type === 'video_playing') {
        ytPlaying = true;
        ytDetected = true;
    } else if (message.type === 'video_paused') {
        ytPlaying = false;
        ytDetected = true;
    }

    // Forward to native app
    if (socket && socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify(message));
    }
});

connect();
