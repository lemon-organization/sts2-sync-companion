# STS2 Sync Companion

Auto-upload Slay the Spire 2 run files to your [STS2 Dashboard](https://sts2-dashboard.lemoncode.dev).

## What it does

The Companion app runs in your system tray and automatically detects new Slay the Spire 2 runs as you complete them. Once paired with your STS2 Dashboard account, it uploads your runs for analytics, stats tracking, and sharing.

- **Works offline** — queues runs if the dashboard is unreachable
- **No telemetry** — only accesses `*.run` files in the STS2 save folder
- **Lightweight** — runs in the background with minimal CPU/memory impact
- **Cross-platform** — Windows and macOS (both Intel and Apple Silicon)

## Installation

1. Download the installer for your platform from [GitHub Releases](https://github.com/lemon-organization/sts2-sync-companion/releases)
2. Run the installer (`.exe` on Windows, `.dmg` on macOS)
3. The app will launch automatically and appear in your system tray

### First launch (unsigned app warning)

Since the app is unsigned, your OS will show a warning on first launch. This is safe to proceed:

**Windows:**
- If you see "Windows protected your PC" → click "More info" → "Run anyway"

**macOS:**
- Right-click the app in Applications → "Open" → "Open" (on the second dialog)
- On future launches, you can click normally

## Pairing with your dashboard

### Option 1: Deep-link from the dashboard (easier)
1. Go to your [STS2 Dashboard](https://sts2-dashboard.lemoncode.dev)
2. Click "Connect desktop app" in the SyncPanel
3. A pairing window will appear in the Companion app
4. Follow the prompts to connect

### Option 2: Manual entry
1. Open the Companion app (click the icon in your system tray)
2. Click "Pair with a code"
3. Go to your [STS2 Dashboard](https://sts2-dashboard.lemoncode.dev) and get a new pairing code
4. Enter the code in the Companion app and confirm

## Using the app

Once paired:

- The app runs in your system tray (toggle "Syncing" on/off as needed)
- Your runs are automatically uploaded as you complete them
- Check the app's status window to see sync history and any errors
- Right-click the tray icon to access options or quit

## What data is used?

The app **only** reads:
- `.run` files in your STS2 save folder (typically `Documents/SlayTheSpire2/runs/`)

The app **does not**:
- Read other game files or folders
- Collect personal data or telemetry
- Send anything other than run JSON to the dashboard
- Require internet access to play the game

## Build from source

Requires:
- Rust (stable toolchain)
- Node.js 22+
- macOS: Xcode command-line tools (`xcode-select --install`)
- Windows: Visual Studio Build Tools (C++ workload)

```bash
# Install dependencies
npm install

# Development mode
npm run dev

# Build native app for your platform
cargo tauri build

# Built app will be in src-tauri/target/release/bundle/
```

See [Tauri docs](https://tauri.app/) for more build details.

## Support

Found a bug or have a question? Please open an issue on [GitHub](https://github.com/lemon-organization/sts2-sync-companion/issues).
