<p align="center">
  <img src="src-tauri/icons/icon.png" width="120" alt="StillSound">
</p>

<h1 align="center">StillSound Studio</h1>

<p align="center">
  <a href="https://github.com/saketjndl/StillSound-Studio/releases">
    <img alt="Version" src="https://img.shields.io/badge/version-1.2.0-blue.svg?style=for-the-badge">
  </a>
  <a href="https://github.com/saketjndl/StillSound-Studio/releases">
    <img alt="Downloads" src="https://img.shields.io/github/downloads/saketjndl/StillSound-Studio/total.svg?style=for-the-badge">
  </a>
  <a href="https://github.com/saketjndl/StillSound-Studio/stargazers">
    <img alt="Stars" src="https://img.shields.io/github/stars/saketjndl/StillSound-Studio.svg?style=for-the-badge">
  </a>
  <a href="https://github.com/saketjndl/StillSound-Studio/issues">
    <img alt="Issues" src="https://img.shields.io/github/issues/saketjndl/StillSound-Studio.svg?style=for-the-badge">
  </a>
  <a href="LICENSE">
    <img alt="License" src="https://img.shields.io/badge/license-MIT-green.svg?style=for-the-badge">
  </a>
  <img alt="Rust" src="https://img.shields.io/badge/built_with-Rust-f26d00.svg?style=for-the-badge&logo=rust">
  <img alt="Tauri" src="https://img.shields.io/badge/built_with-Tauri-24c8db.svg?style=for-the-badge&logo=tauri">
  <img alt="Windows" src="https://img.shields.io/badge/platform-Windows-0078d7.svg?style=for-the-badge&logo=windows">
</p>

<p align="center">
  <strong>Auto-sync Spotify with YouTube — built for students who study with music.</strong>
</p>

<p align="center">
  <a href="https://github.com/saketjndl/StillSound-Studio/releases">Download</a> ·
  <a href="#demo">Demo</a> ·
  <a href="#how-it-works">How it works</a>
</p>

---

StillSound automatically pauses your Spotify when you play a YouTube video and resumes it when you stop. No more alt-tabbing to pause your music during lectures.

## Demo

<div align="center">
  <img src="media/demo.gif" alt="StillSound Demo" width="100%">
</div>

## Versions & Changelog

### v1.2.0 (Latest)
- **Multi-Browser Sync**: Intelligent cross-browser aggregation ensures Spotify only resumes when ALL connected browsers have stopped playing. No more race conditions when switching between Firefox and Brave.
- **Firefox Compatibility**: Official support for Firefox with a Manifest V3 bridge.
- **Robust WebSocket Engine**: Enhanced connection stability and automatic cleanup on browser/tab exit.

### v1.1.0
- **Minimize to Tray**: Keep the app running in the system tray without cluttering your taskbar.
- **Autostart**: Optionally launch StillSound on system boot.
- **Single Instance**: Prevents multiple instances of the app from running simultaneously.
- **UI Refinement**: Polished dashboard with better sync status indicators.

### v1.0.0
The initial release featuring the core sync engine:
- **WebSocket Bridge**: High-speed communication between the browser and desktop.
- **Spotify OAuth PKCE**: Secure authentication with your Spotify account.
- **Auto-Pause/Resume**: The foundation of StillSound—works perfectly with YouTube.


## How It Works

```
┌─────────────────────┐     WebSocket      ┌──────────────────┐     Spotify API     ┌──────────┐
│ Multiple Browsers   │ ◄───────────────► │  StillSound App  │ ◄────────────────► │  Spotify  │
│ (Chrome, FF, Brave) │   localhost:9876    │  (Desktop)       │     OAuth + REST    │          │
└─────────────────────┘                    └──────────────────┘                     └──────────┘
```

1. The **browser extension** detects when a YouTube video plays or pauses.
2. It sends that info to the **StillSound desktop app** over a local WebSocket.
3. The desktop app tells **Spotify** to pause or resume accordingly.

---

## Installation

### Desktop App (Windows)

1. Download the latest `.exe` installer from the [Releases](https://github.com/saketjndl/StillSound-Studio/releases) page.
2. Run the installer — choose where to install.
3. Launch **StillSound** from the Start Menu or desktop.

### Browser Extension (Chrome / Brave / Firefox)
#### Chrome & Brave
1. Download or clone this repository to get the `browser-extension` folder.
2. Open Browser → `chrome://extensions/`
3. Enable **Developer Mode** (toggle in the top right).
4. Click **Load Unpacked** → select the `browser-extension` folder.

#### Firefox
1. Open Firefox → `about:debugging` → "This Firefox".
2. Click **Load Temporary Add-on**.
3. Select the `manifest.json` inside the `browser-extension` folder.

---

## First-Time Setup

The app walks you through setup:

1. **Spotify Client ID** — Go to [Spotify Developer Dashboard](https://developer.spotify.com/dashboard), create an app, copy the Client ID, and set the Redirect URI to `http://127.0.0.1:8921/callback`.
2. **Connect** — Click "Connect to Spotify" in the app, sign in, and approve.

That's it. Open a YouTube video and your Spotify will pause automatically.

---

## Building from Source

Requires Rust, Node.js, and MSVC Build Tools.

```bash
# Development
npm run dev

# Build installer (.exe)
npm run build
```

The installer is output to `src-tauri/target/release/bundle/nsis/`.

---

## Project Structure

```
├── src/                    # Frontend (HTML/CSS/JS)
│   ├── index.html
│   ├── styles.css
│   └── main.js
├── src-tauri/              # Rust backend
│   ├── src/lib.rs          # Sync engine
│   ├── tauri.conf.json     # App & installer config
│   └── Cargo.toml          # Dependencies
└── browser-extension/      # Chrome extension
    ├── manifest.json
    ├── background.js
    ├── content.js
    ├── popup.html
    └── popup.js
```

---

## Credits

Built by **saketjndl (.sodiumcyanide)**. 

If StillSound helps your study sessions, consider [starring the repo](https://github.com/saketjndl/StillSound-Studio) &#9733;

---

## License

[MIT](LICENSE)
