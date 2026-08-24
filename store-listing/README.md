# StillSound Studio — Store Listing & Packaging Guide

This directory contains everything you need when you decide to publish the StillSound browser extension to the **Google Chrome Web Store**, **Mozilla Firefox Add-ons (AMO)**, or **Microsoft Edge Add-ons**.

---

## 1. Store Metadata

### Extension Name
`StillSound Studio — Auto-pause Spotify for Videos`

### Short Description (Max 132 characters)
`Automatically pause Spotify when watching videos on YouTube, Netflix, Twitter, Udemy, Twitch, and more. Requires StillSound app.`

### Detailed Description
```
⚠️ REQUIRES THE FREE STILLSOUND DESKTOP APP: 
This extension is a companion bridge that connects your browser to the StillSound Studio desktop app. Download the free desktop app for Windows, Linux, or macOS at: https://github.com/saketjndl/StillSound-Studio/releases

---

StillSound automatically pauses your Spotify or media player when you play a video (on YouTube, Twitter, Reddit, Netflix, Twitch, Coursera, Udemy, etc.) and seamlessly resumes it when you stop or close the tab. No more alt-tabbing to pause your music during lectures, meetings, or while browsing feeds!

✨ FEATURES:
• Universal Multi-Site Support: Works on YouTube, Netflix, Prime Video, Twitch, Twitter/X, Reddit, Instagram, Udemy, Coursera, Google Meet, Zoom, and more.
• Universal Media Control: Works with Spotify, Apple Music, VLC, or any local media player via OS native media transport controls (SMTC/MPRIS/AppleScript).
• Dynamic Island Popup: View album art, track details, and control playback (Play, Pause, Next, Previous) directly from your browser toolbar.
• Audio-Responsive Aesthetics: The extension popup glows and pulses with the vibrant colors of your current album art.
• Zero Latency & Privacy-First: All communication happens locally over a secure WebSocket (localhost:9876) directly to your desktop app. Zero data ever leaves your computer!

🛠️ HOW IT WORKS:
1. Install this browser extension.
2. Download and run the StillSound Studio desktop app from GitHub.
3. Play any video — your music pauses automatically!
```

---

## 2. Privacy Policy & Permission Justifications

When submitting to the Web Store, you will be asked to justify why you need certain permissions:

- **`activeTab` / `tabs`**: Needed to inject content scripts (`content.js`) into supported video websites to detect HTML5 `<video>` play/pause events.
- **`alarms`**: Needed to run background keep-alive checks to ensure the local WebSocket bridge (`ws://127.0.0.1:9876`) stays connected to the desktop app.
- **Host Permissions (`*://*.youtube.com/*`, etc.)**: Needed to monitor video activity on the specific streaming, social, and educational platforms enabled by the user.

### Data Collection Declaration:
- **Does this extension collect user data?**: NO.
- **Does this extension communicate with remote servers?**: NO.
- **Explanation**: StillSound operates 100% locally. The extension sends lightweight JSON messages containing video playback state (e.g., `{"type": "yt_playing"}`) over a local WebSocket to `ws://127.0.0.1:9876` running on the user's own machine. No personal data, browsing history, or analytics are ever collected, stored, or transmitted over the internet.

---

## 3. Promotional Asset Checklist

When preparing your submission, ensure you have the following graphics ready:

- [x] **Store Icon (128x128 PNG)**: Available at `browser-extension/icons/icon128.png`.
- [ ] **Small Promo Tile (440x280 PNG)**: A clean banner showing the StillSound logo and tagline.
- [ ] **Screenshots (1280x800 or 640x400 PNG)**:
  1. **Dashboard View**: Screenshot of the browser popup showing active album art and playback controls while music is playing.
  2. **Auto-Pause in Action**: Screenshot showing a YouTube video playing and the extension status pill displaying `Auto-paused by video`.
  3. **Onboarding View**: Screenshot of the friendly `Desktop App Required` banner shown when the desktop app is closed.

---

## 4. Packaging Instructions

To create the release zip file for upload to the Chrome Web Store or Firefox AMO:

```bash
# On Windows / Linux / macOS:
# Create a zip of the browser-extension folder (excluding unnecessary files)
cd browser-extension
zip -r ../StillSound-Extension-v2.0.0.zip . -x "*.DS_Store*" "*.git*"
```

Upload `StillSound-Extension-v2.0.0.zip` directly to the Chrome Developer Dashboard or Firefox Add-on Developer Hub!
