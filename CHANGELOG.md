# Changelog

All notable changes to **StillSound Studio** are documented here. See [README.md](README.md) for the quick feature overview as well.

## [3.0.1] - 2026-11-27

### Added
- **Spinning Mini-HUD Album Art Toggle (`album_art_spin`)**: Added Settings switch (`#spin-art-toggle`) so users can disable vinyl album rotation during lectures or focus sessions, switching Mini-HUD art to a calm rounded square.
- **Multi-Browser Heartbeats & Bridge Count UI**: Added 20s WebSocket `Ping` heartbeats, 60s dead-socket cleanup, live `active_bridge_count` payloads (`linked` / `linked (N)` / `unlinked`), and Firefox MV3 1-click site-permission prompt in the extension popup.
- **macOS Packaging Assets & Metadata**: Added `src-tauri/Info.plist` (`NSAppleEventsUsageDescription`, `NSMicrophoneUsageDescription`), multi-resolution `src-tauri/icons/icon.icns`, ad-hoc CI signing (`signingIdentity: "-"`), and `plutil -lint` check in CI.
- **Update & Feedback UX**: Added OS-aware GitHub Releases semver update checks (`.exe`/`.dmg`/`.AppImage`), 12h check caching, Dashboard update & once-per-version "What's New" banners, and pre-filled GitHub issue feedback links.
- **Rust Unit Tests**: Added tests in `lib.rs`, `macos_backend.rs`, and `mpris_backend.rs` covering domain matching, WS origins, AppleScript seek clamping/parsing, and MPRIS browser exclusion.

### Changed
- Upgraded `@tauri-apps/cli` to `^2.12.6` (fixing Linux AppImage `.DirIcon` failure) and resized `32x32.png` / `128x128.png` icons to match exact pixel dimensions.
- Switched OBS Browser Source overlay (`:9877/overlay`) to poll `/state` JSON directly instead of piggybacking on the extension WebSocket bridge.
- Unified manual play/pause state handling across UI controls, tray menu, and global shortcuts (`Ctrl+Alt+P`, `Alt+Shift+P`), restoring ducked volume cleanly on tray quit.

### Fixed
- Eliminated sleep/wake ghost `video_playing` / `video_paused` toggles across `content.js`, `background.js`, `mic_monitor.rs`, and `smtc_backend.rs`.
- Fixed Firefox Event Page & Chromium Service Worker cold-wake message drops and simultaneous multi-browser disconnections prematurely flipping status to `unlinked`.
- Fixed the `.max(75)` ducking floor bug, Windows volume ducking leaking onto communication/system apps, and overlapping duck/unduck fade loops.
- Fixed Firefox AMO MV3 metadata (`data_collection_permissions: ["none"]`, `strict_min_version: "140.0"`, `"storage"` permission) and missing `tiktok.com` manifest rules.
- Fixed `"mini_hud"` ACL permissions in `capabilities/default.json`, macOS `data-tauri-drag-region` & `macOSPrivateApi` window support, cross-OS `settings.json` persistence path, Linux MPRIS pausing browsers, OAuth PKCE `state` validation & `204` token-refresh loops, `autostart_minimized`, `currentTrackName` empty-state reset, and 7-day weekly recap accuracy.

## [3.0.0]
- Smooth Audio Ducking & Fade Engine, Per-Site Action Matrix (`Pause | Duck | Ignore`), Smart Rewind, Mic/Call-Aware Mode, and Floating Mini-HUD.

## [2.5.0]
- 4-Tier Smart Session Priority Engine, deterministic Play/Pause sync, resilient extension bridge, and UI/SVG polish.

## [2.0.0]
- Universal Multi-Site support, Universal Mode (Beta), cross-platform installers, Dynamic Island UI, and bi-directional Spotify volume sync.

## [1.2.0] - [1.0.0]
- Multi-browser aggregation, system tray + autostart, and initial WebSocket/Spotify PKCE release.
