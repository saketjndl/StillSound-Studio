const api = (typeof browser !== 'undefined') ? browser : chrome;
const attachedVideos = new WeakSet();
let isCurrentlyPlaying = false;
let pauseTimeout = null;

function checkPlaybackState() {
    // Check if ANY video on the page is currently playing
    const videos = Array.from(document.querySelectorAll('video'));
    const isPlaying = videos.some(v => !v.paused && !v.ended && v.readyState > 0);
    
    if (isPlaying) {
        if (pauseTimeout) {
            clearTimeout(pauseTimeout);
            pauseTimeout = null;
        }
        if (!isCurrentlyPlaying) {
            isCurrentlyPlaying = true;
            console.log('[StillSound] Video started playing on', location.hostname);
            api.runtime.sendMessage({ type: 'video_playing', hostname: location.hostname });
        }
    } else {
        if (isCurrentlyPlaying && !pauseTimeout) {
            // Debounce the pause event to prevent jitter on scrolling feeds
            pauseTimeout = setTimeout(() => {
                isCurrentlyPlaying = false;
                pauseTimeout = null;
                console.log('[StillSound] All videos paused on', location.hostname);
                api.runtime.sendMessage({ type: 'video_paused', hostname: location.hostname });
            }, 1200); // 1.2s grace period for consecutive/looping videos
        }
    }
}

function attachVideo() {
    const videos = document.querySelectorAll('video');
    
    videos.forEach(video => {
        if (attachedVideos.has(video)) return;
        
        attachedVideos.add(video);
        console.log('[StillSound] New video detected on', location.hostname);

        // Standard media events
        video.addEventListener('play', checkPlaybackState);
        video.addEventListener('playing', checkPlaybackState);
        video.addEventListener('pause', checkPlaybackState);
        video.addEventListener('ended', checkPlaybackState);
        video.addEventListener('waiting', checkPlaybackState);
        video.addEventListener('seeked', checkPlaybackState);
    });
    
    // Also check state in case a video is already playing
    checkPlaybackState();
}

// Watch for dynamically added videos (like scrolling on Twitter/Reddit)
const observer = new MutationObserver(attachVideo);
observer.observe(document.body, { childList: true, subtree: true });

// Periodic fallback to catch anything missed (e.g., SPAs, shadow DOMs)
setInterval(attachVideo, 1000);
// Periodic state check fallback
setInterval(checkPlaybackState, 1000);

// Initial attach
attachVideo();
