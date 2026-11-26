# Changelog

All notable changes to **StillSound Studio** are documented here. See [README.md](README.md) for the quick feature overview as well.

## [3.1.0] - 2026-11-27

### Added
- **Boot Readiness Startup Screen (`startup_splash_enabled`)**: Added a Discord-style launch overlay that verifies loaded studio preferences, audio/call monitor engine readiness, and browser bridge + linked client status (with rotating quips, weekly focus pulse, click-to-skip, and a **Startup Screen** toggle in Settings).
- **Desktop & Extension Ambient Gradient Toggles (`app_gradient_enabled`, `ext_gradient_enabled`)**: Added separate accessibility contrast switches inside **Settings → Mini-HUD & Visuals** to toggle blurred album-art backdrop gradients in the Desktop Studio + Mini-HUD and in the Browser Extension popup (preserving album cover art while removing background color glow).
- **Resizable Floating Mini-HUD (`mini_hud_scale`)**: Added a **Mini-HUD Size** slider (`60%–175%`) with click-to-reset pill, proportional OS window resizing + CSS zoom scaling, live drag preview, and playful quips when set extra small or extra large.
- **Spinning Mini-HUD Album Art Toggle (`album_art_spin`)**: Added Settings switch (`#spin-art-toggle`) so users can disable vinyl album rotation during lectures or focus sessions, switching Mini-HUD art to a calm rounded square.
- **Timed Snooze & Dynamic Tray Status Badges**: Added `15m`, `1h`, and `Until Restart` Snooze Sync controls in both the Console UI and Tray menu (auto-expiring and restoring ducked/paused playback cleanly), plus state-tinted status dots on the tray icon.
- **Peek-on-Action Mini-HUD & Lock Persistence**: Added **Pinned / Peek / Auto-Hide** visibility modes (gliding down briefly on pause, duck, or track changes when set to `Peek`), persisted HUD drag-lock state, and made HUD dragging unlocked by default.
- **Console | Rules | Settings UI & Custom Domains**: Upgraded the desktop console with segmented tabs, Universal-first setup checklist, interactive Console **Sync Engine / Duck Level / Snooze** bar, searchable Site Rules with **`Study` / `Chill` / `Meeting` Presets**, **+ Add Custom Domain**, customizable Smart Rewind intervals (`3s–15s` / `10s–60s`), OBS Overlay copy card, and higher-contrast accessible sliders/scrollbars.
- **1-Click Popup Rule Switcher**: Made the browser extension popup's active-tab rule row interactive (`[Pause | Duck | Ignore]`) with instant multi-browser sync, and sent browser names (`Brave`, `Firefox`, `Edge`, `Chrome`) to the Console.
- **Multi-Browser Heartbeats & Bridge Count UI**: Added 20s WebSocket `Ping` heartbeats, 60s dead-socket cleanup, live `active_bridge_count` payloads (`linked` / `linked (N)` / `unlinked`), and Firefox MV3 1-click site-permission prompt in the extension popup.
- **macOS Packaging Assets & Metadata**: Added `src-tauri/Info.plist` (`NSAppleEventsUsageDescription`, `NSMicrophoneUsageDescription`), multi-resolution `src-tauri/icons/icon.icns`, ad-hoc CI signing (`signingIdentity: "-"`), and `plutil -lint` check in CI.
- **Update & Feedback UX**: Added OS-aware GitHub Releases semver update checks (`.exe`/`.dmg`/`.AppImage`), 12h check caching, Dashboard update & once-per-version "What's New" banners, and pre-filled GitHub issue feedback links.
- **Rust Unit Tests**: Added tests in `lib.rs`, `macos_backend.rs`, and `mpris_backend.rs` covering domain matching, WS origins, AppleScript seek clamping/parsing, and MPRIS browser exclusion.

### Changed
- Added animated `.tab-glider` pill and directional cross-slide view transitions when switching between Console, Rules, Settings, and Setup.
- Widened the base Mini-HUD capsule (`350px` → `390px` inside `400px` frame) and tightened control spacing for ~60% more readable track/artist width.
- Upgraded `@tauri-apps/cli` to `^2.12.6` (fixing Linux AppImage `.DirIcon` failure) and resized `32x32.png` / `128x128.png` icons to match exact pixel dimensions.
- Switched OBS Browser Source overlay (`:9877/overlay`) to poll `/state` JSON directly instead of piggybacking on the extension WebSocket bridge.
- Unified manual play/pause state handling across UI controls, tray menu, and global shortcuts (`Ctrl+Alt+P`, `Alt+Shift+P`), restoring ducked volume cleanly on tray quit.

### Fixed
- Eliminated sleep/wake ghost `video_playing` / `video_paused` toggles across `content.js`, `background.js`, `mic_monitor.rs`, and `smtc_backend.rs`.
- Fixed Firefox Event Page & Chromium Service Worker cold-wake message drops and simultaneous multi-browser disconnections prematurely flipping status to `unlinked`.
- Fixed the `.max(75)` ducking floor bug, Windows volume ducking leaking onto communication/system apps, and overlapping duck/unduck fade loops.
- Fixed Firefox AMO MV3 metadata (`data_collection_permissions: ["none"]`, `strict_min_version: "140.0"`, `"storage"` permission) and missing `tiktok.com` manifest rules.
- Fixed `"mini_hud"` ACL permissions in `capabilities/default.json`, macOS `data-tauri-drag-region` & `macOSPrivateApi` window support, cross-OS `settings.json` persistence path, Windows SMTC `GetCurrentSession()` browser fallback loop + Firefox 16-hex Windows hash AUMID detection (`308046B0AF4A39CB`) & dedicated-music-first session priority (preventing secondary video browsers from being paused once Spotify is already paused), Linux MPRIS pausing browsers, OAuth PKCE `state` validation & `204` token-refresh loops, `autostart_minimized`, `currentTrackName` empty-state reset, corrupted `#win-settings` header gear & mismatched Ducking `$` SVG icons, Settings accordion scrollbar reflow/wrapping, Mini-HUD subtitle flex-ellipsis truncation, stale duplicate browser WebSocket socket deduplication via persistent `client_id` + single-line alignment, Chromium global `browser` namespace misclassification (`isFirefox` now checks `moz-extension://` so Brave/Chrome never report as Firefox), startup splash gating before revealing Console UI, and 7-day weekly recap accuracy.

## [3.0.0]
- Smooth Audio Ducking & Fade Engine, Per-Site Action Matrix (`Pause | Duck | Ignore`), Smart Rewind, Mic/Call-Aware Mode, and Floating Mini-HUD.

## [2.5.0]
- 4-Tier Smart Session Priority Engine, deterministic Play/Pause sync, resilient extension bridge, and UI/SVG polish.

## [2.0.0]
- Universal Multi-Site support, Universal Mode (Beta), cross-platform installers, Dynamic Island UI, and bi-directional Spotify volume sync.

## [1.2.0] - [1.0.0]
- Multi-browser aggregation, system tray + autostart, and initial WebSocket/Spotify PKCE release.
