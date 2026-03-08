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
    chrome.runtime.sendMessage({ type: 'get_status' }, (res) => {
        if (chrome.runtime.lastError) return;
        updateUI(res);
    });
}

// Check immediately, then retry after a beat (service worker might still be waking up)
getStatus();
setTimeout(getStatus, 800);
