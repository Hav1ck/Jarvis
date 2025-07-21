# Local AI Voice Assistant

A lightweight, local voice assistant that listens for a wake word ("Jarvis"), processes your question using AI, and responds with realistic speech.

## 🚀 Getting Started

You’ll need a few **free API keys** to get up and running. Although these services offer paid plans, the **free tiers are generous** and should be sufficient for most users.

### 1. Download the Repository

Create a folder and place all the downloaded files from github release into it.

### 2. Get Your API Keys

You’ll need keys from three services:

- **Picovoice Porcupine** – Detects the wake word.

  - Get it here: [https://picovoice.ai/platform/porcupine/](https://picovoice.ai/platform/porcupine/)

- **Google Gemini** – Handles the AI/LLM response.

  - Get it here: [https://aistudio.google.com/](https://aistudio.google.com/)

- **ElevenLabs** – Generates natural-sounding voice responses.
  - Get it here: [https://elevenlabs.io/](https://elevenlabs.io/)

### 3. Configure Your `config.json`

Open the `config.json` file inside the `assets` folder with a text editor (e.g., Notepad). Paste your API keys into the corresponding fields:

```json
{
  "porcupine_key": "PASTE_YOUR_PICOVOICE_KEY_HERE",
  "gemini_key": "PASTE_YOUR_GEMINI_KEY_HERE",
  "elevenlabs_key": "PASTE_YOUR_ELEVENLABS_KEY_HERE",
  "whisper_language": "en"
}
```

You can change the `"whisper_language"` to any supported language (use the two-letter ISO code, like `"es"` for Spanish). Note: Some features, like clipboard interaction, currently work best in English.

### 4. Run the Assistant

Double-click the `.exe` file to start.

When you see `[DEBUG] Entered wait_for_wakeword`, the assistant is ready! Say **"Jarvis"** and ask your question.

You can also:

- Ask it to copy something to your clipboard, such as code it generated for you.
- Say **"Control V"** to insert clipboard content into your query.
