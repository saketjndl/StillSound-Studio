const api = (typeof browser !== 'undefined') ? browser : chrome;
const attachedVideos = new WeakSet();
let isCurrentlyPlaying = false;
let pauseTimeout = null;

function findAllVideos(root = document) {
    let videos = Array.from(root.querySelectorAll('video'));
    const allElements = root.querySelectorAll('*');
    for (const el of allElements) {
        if (el.shadowRoot) {
            videos = videos.concat(findAllVideos(el.shadowRoot));
        }
    }
    return videos;
}

function isValidVideo(v) {
    if (v.paused || v.ended) return false;
    if (v.muted || v.volume === 0) return false; // Ignore muted/silent autoplay previews & background loops
    if (!isNaN(v.duration) && v.duration < 3) return false; // Ignore short UI sound effects (< 3s)
    
    const rect = v.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return false; // Ignore invisible ad-tracking pixels
    
    return true;
}

function checkPlaybackState() {
    // Check if ANY valid video on the page is currently playing (even if buffering/loading chunks)
    const videos = findAllVideos(document);
    const isPlaying = videos.some(isValidVideo);
    
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
            // Debounce the pause event to prevent jitter on ad transitions or buffering spikes
            pauseTimeout = setTimeout(() => {
                isCurrentlyPlaying = false;
                pauseTimeout = null;
                console.log('[StillSound] All videos paused on', location.hostname);
                api.runtime.sendMessage({ type: 'video_paused', hostname: location.hostname });
            }, 2500); // 2.5s grace period for consecutive/looping videos or ad transitions
        }
    }
}

function attachVideo() {
    const videos = findAllVideos(document);
    
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
        video.addEventListener('volumechange', checkPlaybackState);
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
