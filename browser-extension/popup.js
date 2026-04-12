const api = (typeof browser !== 'undefined') ? browser : chrome;
const dot = document.getElementById('dot');
const appPill = document.getElementById('app-pill');
const ytPill = document.getElementById('yt-pill');

function updateUI(res) {
    if (!res) return;

    if (res.connected) {
        dot.classList.add('on');
        appPill.textContent = 'connected';
        appPill.className = 'pill ok';
    } else {
        dot.classList.remove('on');
        appPill.textContent = 'offline';
        appPill.className = 'pill off';
    }

    if (res.ytPlaying) {
        ytPill.textContent = 'playing';
        ytPill.className = 'pill active';
    } else if (res.ytDetected) {
        ytPill.textContent = 'paused';
        ytPill.className = 'pill off';
    } else {
        ytPill.textContent = 'idle';
        ytPill.className = 'pill off';
    }
}

function getStatus() {
    try {
        const message = { type: 'get_status' };
        
        // Use chrome namespace directly for callback compatibility in both browsers
        // or handle the promise return if using browser namespace.
        // Firefox's chrome.runtime.sendMessage supports callbacks.
        const callback = (res) => {
            const lastError = chrome.runtime.lastError;
            if (lastError) return;
            if (res) updateUI(res);
        };

        const result = api.runtime.sendMessage(message, callback);
        
        // If it returns a promise (Firefox browser namespace), handle it
        if (result && typeof result.then === 'function') {
            result.then(updateUI).catch(() => {});
        }
    } catch (e) {
        console.warn('[StillSound] Error getting status:', e);
    }
}

getStatus();
setInterval(getStatus, 1000);
