const api = (typeof browser !== 'undefined') ? browser : chrome;
let currentVideo = null;

function attachVideo() {
    const video = document.querySelector('video');
    if (!video || video === currentVideo) return;

    currentVideo = video;
    console.log('[StillSound] Video detected, attaching listeners');

    video.addEventListener('play', () => {
        console.log('[StillSound] Video playing');
        api.runtime.sendMessage({ type: 'video_playing' });
    });

    video.addEventListener('pause', () => {
        console.log('[StillSound] Video paused');
        api.runtime.sendMessage({ type: 'video_paused' });
    });

    // Report current state immediately if video is already there
    if (!video.paused) {
        api.runtime.sendMessage({ type: 'video_playing' });
    }
}

// YouTube is a SPA — video element can change on navigation
const observer = new MutationObserver(attachVideo);
observer.observe(document.body, { childList: true, subtree: true });

// Periodic fallback for YouTube's SPA navigation (every 1s)
setInterval(attachVideo, 1000);

// Initial attach
attachVideo();
