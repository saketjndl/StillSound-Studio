// StillSound Content Script
let videoElement = null;

function findVideo() {
    videoElement = document.querySelector('video');
    if (videoElement) {
        console.log('[STILLSOUND] Video Detector Active');

        videoElement.onplay = () => chrome.runtime.sendMessage({ type: 'video_playing' });
        videoElement.onpause = () => chrome.runtime.sendMessage({ type: 'video_paused' });
    }
}

// Watch for video element
const observer = new MutationObserver(() => {
    if (!videoElement) findVideo();
});

observer.observe(document.body, { childList: true, subtree: true });
findVideo();
