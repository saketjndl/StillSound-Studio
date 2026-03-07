const dot = document.getElementById('dot');
const appPill = document.getElementById('app-pill');
const ytPill = document.getElementById('yt-pill');

chrome.runtime.sendMessage({ type: 'get_status' }, (res) => {
    if (chrome.runtime.lastError || !res) return;

    if (res.connected) {
        dot.classList.add('on');
        appPill.textContent = 'connected';
        appPill.className = 'pill ok';
    } else {
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
});
