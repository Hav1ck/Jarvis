Jarvis – AI Voice Assistant

Jarvis is a desktop AI voice assistant built with Tauri (Rust + React). Speak with wake word listening, get answers, and hear responses with natural TTS.

### Fast AI Response

To get answers quickly (in under 10 seconds), make sure to download the GPU version of the AI.
If you use the default version, the Whisper model will run on your CPU, which can take up to a minute per response.

### Quick start (regular users)
- Download the latest Windows installer from the Releases page.
- Prerequisites:
  - Windows 10/11 (x64)
  - Microphone and speakers
  - Internet connection
  - Microsoft Edge WebView2 Runtime (installed automatically if missing)
  - API keys (required on first run):
    - Picovoice Porcupine (wake word)
    - Google Gemini (LLM)
    - ElevenLabs (text‑to‑speech)

On first launch you’ll see an onboarding screen to paste these keys. Links are provided in‑app.

### Using Jarvis
- Start/stop listening: use the mic button. When active, Jarvis listens for the wake word “Jarvis”.
- Text mode: toggle input mode and press Enter to send.
- History: open the left sidebar to browse conversations.
- Settings: open the right sidebar to configure API keys, theme, and input mode.

### Modify or run from source
Prerequisites: Node.js 18+, pnpm, Rust (stable), Tauri prerequisites for Windows (MSVC Build Tools, WebView2). See Tauri docs.

Commands:
```bash
git clone https://github.com/Hav1ck/Jarvis
cd Jarvis
pnpm install
pnpm tauri dev
```

### Support and contributions
- Issues and bug reports are welcome. Please use the issue template.
- Pull requests are not accepted for this project.

### License
AGPL-3.0 © 2025 Hav1ck


