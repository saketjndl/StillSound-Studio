(() => {
    if (window.__stillsoundContentInitialized) return;
    window.__stillsoundContentInitialized = true;

    const extRuntime = typeof browser !== 'undefined' ? browser : chrome;
    const isFirefox = (() => {
        try {
            if (extRuntime?.runtime?.getURL) {
                return extRuntime.runtime.getURL('').startsWith('moz-extension://');
            }
        } catch (e) {}
        return /Firefox\//i.test(navigator.userAgent || '');
    })();
    const api = isFirefox ? browser : chrome;
    const attachedVideos = new WeakSet();

    const PLAY_CONFIRM_MS = 350;
    const PAUSE_DEBOUNCE_MS = 1600;
    const WAKE_COOLDOWN_MS = 2500;
    const SLEEP_GAP_MS = 4000;

    let isCurrentlyPlaying = false;
    let playConfirmTimer = null;
    let pauseTimeout = null;
    let wakeCooldownUntil = 0;
    let lastTickAt = Date.now();
    let mutationScanScheduled = false;

    function sendRuntimeMessage(payload) {
        try {
            if (isFirefox) {
                const res = browser.runtime.sendMessage(payload);
                if (res && typeof res.then === 'function') {
                    res.catch(() => {});
                }
            } else {
                chrome.runtime.sendMessage(payload, () => {
                    void chrome.runtime?.lastError;
                });
            }
        } catch (e) {}
    }

    function findVideosInRoot(root = document, includeShadow = false) {
        if (!root || typeof root.querySelectorAll !== 'function') return [];
        let videos = Array.from(root.querySelectorAll('video'));
        if (includeShadow) {
            const allElements = root.querySelectorAll('*');
            for (const el of allElements) {
                if (el.shadowRoot) {
                    videos = videos.concat(findVideosInRoot(el.shadowRoot, true));
                }
            }
        }
        return videos;
    }

    function isValidVideo(v, requireReady = false) {
        if (v.paused || v.ended || v.seeking) return false;
        if (v.muted || v.volume === 0) return false; // Ignore muted/silent autoplay previews & background loops
        if (requireReady && v.readyState < 2) return false; // Ignore unloaded/re-initializing players on wake
        if (!Number.isNaN(v.duration) && v.duration > 0 && v.duration < 3) return false; // Ignore short UI sound effects (< 3s)

        const rect = v.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) return false; // Ignore invisible ad-tracking pixels

        return true;
    }

    function clearPlayConfirm() {
        if (playConfirmTimer) {
            clearTimeout(playConfirmTimer);
            playConfirmTimer = null;
        }
    }

    function clearPauseTimeout() {
        if (pauseTimeout) {
            clearTimeout(pauseTimeout);
            pauseTimeout = null;
        }
    }

    function checkPlaybackState() {
        const now = Date.now();

        // Detect OS sleep / tab freeze wake-up gaps and absorb transient decoder glitches
        if (now - lastTickAt > SLEEP_GAP_MS) {
            wakeCooldownUntil = now + WAKE_COOLDOWN_MS;
            clearPlayConfirm();
        }
        lastTickAt = now;

        const videos = findVideosInRoot(document, false);
        const requireReady = !isCurrentlyPlaying;
        const isPlaying = videos.some((v) => isValidVideo(v, requireReady));

        if (isPlaying) {
            clearPauseTimeout();

            if (!isCurrentlyPlaying && !playConfirmTimer) {
                const remainingWake = Math.max(0, wakeCooldownUntil - now);
                const delay = Math.max(PLAY_CONFIRM_MS, remainingWake);

                playConfirmTimer = setTimeout(() => {
                    playConfirmTimer = null;
                    const currentVideos = findVideosInRoot(document, false);
                    if (currentVideos.some((v) => isValidVideo(v, true))) {
                        isCurrentlyPlaying = true;
                        console.log('[StillSound] Video started playing on', location.hostname);
                        sendRuntimeMessage({ type: 'video_playing', hostname: location.hostname });
                    }
                }, delay);
            }
        } else {
            clearPlayConfirm();

            if (isCurrentlyPlaying && !pauseTimeout) {
                // Debounce pause events to prevent jitter on ad transitions or buffering spikes
                pauseTimeout = setTimeout(() => {
                    pauseTimeout = null;
                    const currentVideos = findVideosInRoot(document, false);
                    if (!currentVideos.some((v) => isValidVideo(v, false))) {
                        isCurrentlyPlaying = false;
                        console.log('[StillSound] All videos paused on', location.hostname);
                        sendRuntimeMessage({ type: 'video_paused', hostname: location.hostname });
                    }
                }, PAUSE_DEBOUNCE_MS);
            }
        }
    }

    function attachVideos(videos) {
        let attachedAny = false;
        videos.forEach((video) => {
            if (attachedVideos.has(video)) return;

            attachedVideos.add(video);
            attachedAny = true;
            console.log('[StillSound] New video detected on', location.hostname);

            video.addEventListener('play', checkPlaybackState);
            video.addEventListener('playing', checkPlaybackState);
            video.addEventListener('pause', checkPlaybackState);
            video.addEventListener('ended', checkPlaybackState);
            video.addEventListener('waiting', checkPlaybackState);
            video.addEventListener('seeked', checkPlaybackState);
            video.addEventListener('volumechange', checkPlaybackState);
        });

        if (attachedAny) {
            checkPlaybackState();
        }
    }

    // Handle Page Lifecycle freeze/resume events (supported in Chromium)
    document.addEventListener('resume', () => {
        wakeCooldownUntil = Date.now() + WAKE_COOLDOWN_MS;
        clearPlayConfirm();
        lastTickAt = Date.now();
        setTimeout(checkPlaybackState, WAKE_COOLDOWN_MS);
    });

    window.addEventListener('pagehide', () => {
        clearPlayConfirm();
        clearPauseTimeout();
        if (isCurrentlyPlaying) {
            isCurrentlyPlaying = false;
            sendRuntimeMessage({ type: 'video_paused', hostname: location.hostname });
        }
    });

    // Watch dynamically added nodes without scanning the entire DOM on every mutation
    const targetNode = document.body || document.documentElement;
    if (targetNode) {
        const pendingNodes = new Set();
        const observer = new MutationObserver((mutations) => {
            for (const m of mutations) {
                for (const node of m.addedNodes) {
                    if (node.nodeType === Node.ELEMENT_NODE) {
                        pendingNodes.add(node);
                    }
                }
            }
            if (pendingNodes.size === 0 || mutationScanScheduled) return;

            mutationScanScheduled = true;
            setTimeout(() => {
                mutationScanScheduled = false;
                const nodes = Array.from(pendingNodes);
                pendingNodes.clear();
                const found = [];
                for (const node of nodes) {
                    if (node.tagName === 'VIDEO') {
                        found.push(node);
                    }
                    if (typeof node.querySelectorAll === 'function') {
                        found.push(...findVideosInRoot(node, true));
                    }
                }
                if (found.length > 0) {
                    attachVideos(found);
                }
            }, 150);
        });
        observer.observe(targetNode, { childList: true, subtree: true });
    }

    // Periodic fallback for shadow DOMs and SPA state checks
    setInterval(() => {
        attachVideos(findVideosInRoot(document, false));
        checkPlaybackState();
    }, 1000);

    // Occasional deep shadow-DOM scan for custom video players
    setInterval(() => {
        attachVideos(findVideosInRoot(document, true));
    }, 5000);

    // Respond to background sync requests after reconnect
    api.runtime.onMessage?.addListener((message) => {
        if (message?.type === 'sync_playback_state') {
            const now = Date.now();
            if (now - lastTickAt > SLEEP_GAP_MS) {
                wakeCooldownUntil = now + WAKE_COOLDOWN_MS;
                lastTickAt = now;
            }
            if (now < wakeCooldownUntil) {
                // Re-confirm after wake cooldown instead of replaying stale state
                isCurrentlyPlaying = false;
                clearPlayConfirm();
                clearPauseTimeout();
                checkPlaybackState();
                return;
            }
            checkPlaybackState();
            const stillPlayingNow = findVideosInRoot(document, false).some((v) => isValidVideo(v, false));
            if (isCurrentlyPlaying && stillPlayingNow) {
                sendRuntimeMessage({ type: 'video_playing', hostname: location.hostname });
            }
        }
    });

    // Initial scan
    attachVideos(findVideosInRoot(document, true));
    checkPlaybackState();
})();
