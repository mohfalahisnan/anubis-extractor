# anubis-extractor

OCR and audio/video transcription as a library and MCP server.

- `crates/core` — `anubis-extractor` library
- `crates/mcp`  — `anubis-extractor-mcp` (stdio + HTTP/SSE)
- `crates/cli`  — `anubis-extractor` CLI (debugging)

See [the design spec](../anubis-engine/docs/superpowers/specs/2026-05-27-anubis-extractor-design.md).

## v0.1.0 — what works

- Library API (`anubis-extractor` crate): `Extractor::ocr`, `Extractor::transcribe`, `Extractor::extract_text`.
- MCP server (`anubis-extractor-mcp`): stdio transport + HTTP/SSE on `127.0.0.1:7878`.
- CLI (`anubis-extractor`): `transcribe`, `ocr`, `extract` subcommands.
- Backends: `ocrs` 0.12 for images, whisper.cpp v1.7.6 + ffmpeg (auto-downloaded on Windows; system PATH elsewhere) for audio/video.
- Sidecar cache (`<stem>.anubis.txt`) byte-compatible with anubis-engine.

## v0.1.0 — known limits

- whisper.cpp auto-download is Windows-only. Linux/macOS users must `brew install ffmpeg` and put `whisper-cli` on PATH (build whisper.cpp from source).
- HTTP transport has no auth or TLS — localhost only by default; `--allow-remote` to override.
- OCR returns line-level bboxes as a single page-sized box for now; per-line bboxes are a follow-up.
