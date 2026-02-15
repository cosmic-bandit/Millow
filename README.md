# 🌿 Millow — macOS Voice Dictation App

Millow is a fast, lightweight macOS menubar app that transcribes your speech to text using **Groq Whisper**. Built with Tauri v2 (Rust + React).

**~0.5s transcription speed** — speak naturally, get text instantly in any app.

---

## Features

- ⚡ **Ultra-fast transcription** via Groq Whisper API (~0.3-1.0s)
- 🎹 **Fn double-tap** or **⌥ Space** to start/stop recording
- 📋 **Auto-paste** into any active application
- 🌍 **Translation mode** — speak in one language, get text in another
- 📖 **Custom dictionary** — teach Whisper your names and terms
- 🔄 **Hold-to-talk mode** — hold shortcut, speak, release to transcribe
- 🎯 **Configurable shortcuts** — choose from Alt+Space, Ctrl+Space, F5, F6, etc.
- 🇹🇷 **Turkish language optimized** with full UTF-8 support

---

## Installation

### From DMG (recommended)
1. Download `Millow_0.1.0_aarch64.dmg` from [Releases](https://github.com/cosmic-bandit/Millow/releases)
2. Drag `Millow.app` to `/Applications`
3. Open Millow — grant these permissions when prompted:
   - **Microphone** — required for recording
   - **Accessibility** — required for keyboard shortcuts
   - **Input Monitoring** — required for Fn double-tap

### Build from source
```bash
# Prerequisites
# - macOS 13+ (Ventura or later)
# - Node.js 18+
# - Rust (rustup.rs)

git clone https://github.com/cosmic-bandit/Millow.git
cd Millow
npm install
npm run tauri build
```

The DMG will be in `src-tauri/target/release/bundle/dmg/`.

---

## Setup

1. Get a free API key from [Groq Console](https://console.groq.com/keys)
2. Open Millow settings (click tray icon → Settings)
3. Paste your Groq API key
4. Start dictating!

---

## Usage

| Action | How |
|--------|-----|
| Start recording | **Fn** double-tap or **⌥ Space** |
| Stop & transcribe | **Fn** double-tap or **⌥ Space** again |
| Hold-to-talk | Hold shortcut → speak → release |

Millow automatically types the transcribed text into whatever app you were using.

---

## Custom Dictionary

Add names, technical terms, or words that Whisper might mishear:

**Settings → Custom Dictionary**

```
Mehmet
Cliniolabs
Nietzsche
```

These are sent as hints to Whisper for better accuracy.

---

## Tech Stack

- **Tauri v2** — native macOS app (Rust backend + React frontend)
- **Groq Whisper** — fast speech-to-text API
- **cpal** — cross-platform audio recording
- **rdev** — global keyboard event listener (Fn double-tap)
- **CGEvent** — native macOS key injection (Cmd+V paste)

---

## Config

Settings are stored in `~/.millow/config.json`.

---

## Requirements

- macOS 13+ (Ventura or later)
- Apple Silicon (M1/M2/M3/M4) or Intel
- Internet connection (for Groq API)
- Groq API key (free tier available)

---

## License

MIT
