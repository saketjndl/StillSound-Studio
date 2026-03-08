// StillSound Content Script — YouTube video sync
let currentVideo = null;

function attachToVideo(video) {
    if (video === currentVideo) return;
    currentVideo = video;

    video.addEventListener('play', () => {
        chrome.runtime.sendMessage({ type: 'video_playing' });
    });

    video.addEventListener('pause', () => {
        chrome.runtime.sendMessage({ type: 'video_paused' });
    });

    // Send current state immediately
    if (!video.paused) {
        chrome.runtime.sendMessage({ type: 'video_playing' });
    }
}

function findAndAttach() {
    const video = document.querySelector('video');
    if (video && video !== currentVideo) {
        attachToVideo(video);
    }
}

// YouTube is a SPA — the video element can change on navigation.
// Use MutationObserver + periodic check to always stay attached.
const observer = new MutationObserver(findAndAttach);
observer.observe(document.body, { childList: true, subtree: true });

// Periodic fallback for SPA navigations that don't trigger mutations
setInterval(findAndAttach, 2000);

findAndAttach();
