# Skill: Extract Text and Audio/Video Transcripts using Anubis Extractor

This skill enables AI agents to extract text from images (OCR) and transcribe audio or video files using the `anubis-extractor` CLI tool on Windows.

---

## Prerequisites

The binary is already compiled and ready to use. It is located at:
`d:\workspaces\coding\projects\anubis-extractor\target\release\anubis-extractor.exe`

---

## How to Extract Text / Transcripts

Always run the commands from the root directory: `d:\workspaces\coding\projects\anubis-extractor`.

### 1. Auto-Detect and Extract (Recommended)
Automatically routes the file to OCR or transcription based on its file extension.
```powershell
.\target\release\anubis-extractor.exe extract "<absolute_path_to_file>" --write-sidecar
```

### 2. Extract Text from Images (OCR)
Supported formats: `.png`, `.jpg`, `.jpeg`, `.webp`, `.tiff`, `.bmp`.
```powershell
.\target\release\anubis-extractor.exe ocr "<absolute_path_to_image>" --write-sidecar
```

### 3. Transcribe Audio or Video Files
Supported formats: `.mp3`, `.wav`, `.m4a`, `.flac`, `.ogg`, `.opus`, `.mp4`, `.mov`, `.mkv`, `.webm`, `.avi`.
```powershell
.\target\release\anubis-extractor.exe transcribe "<absolute_path_to_audio_or_video>" --write-sidecar
```

---

## Command Reference & Options

| Flag / Parameter | Description |
|---|---|
| `--write-sidecar` | Writes the output text into a `<filename>.anubis.txt` file next to the source file (caching). |
| `--force` | Bypasses the sidecar cache and forces re-processing. |
| `--language <lang>` | *For transcription only.* Specifying a 2-letter code (e.g., `en`, `id`) speeds up processing and improves accuracy. Defaults to auto-detection. |
| `--whisper-model <model>` | *For transcription only.* Choose Whisper model size: `tiny`, `base`, `small`, `medium` (default), `large-v3`. Smaller models are faster; larger are more accurate. |

---

## Output Format

The CLI outputs a JSON string to `stdout`.

### OCR Output Format
```json
{
  "text": "Extracted text content...",
  "lines": [
    {
      "bbox": [0, 0, 1024, 768],
      "text": "Extracted text content..."
    }
  ],
  "sidecar_path": "D:/path/to/image.anubis.txt",
  "cache_hit": false
}
```

### Transcribe Output Format
```json
{
  "text": "Full transcribed text...",
  "segments": [
    {
      "start_ms": 0,
      "end_ms": 2500,
      "text": "Hello world."
    }
  ],
  "language": "en",
  "sidecar_path": "D:/path/to/video.anubis.txt",
  "cache_hit": false
}
```

---

## Agent Usage Instructions

When an agent needs to perform extraction:
1. Check if `target/release/anubis-extractor.exe` exists. If not, build it.
2. Run the command using the `run_command` tool.
3. Parse the standard output as JSON.
4. If `--write-sidecar` was used, you can also read the output from the sidecar file path returned in the JSON (`sidecar_path`).
