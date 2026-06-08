//! ffmpeg → WAV → whisper.cpp → parsed segments.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::transcription::artifacts::Artifacts;
use crate::types::{Progress, Segment, TranscribeResult};
use crate::ExtractorError;

pub(crate) fn transcribe(
    source: &Path,
    artifacts: &Artifacts,
    language: Option<&str>,
    progress: Option<&tokio::sync::mpsc::Sender<Progress>>,
) -> Result<TranscribeResult, ExtractorError> {
    let _ = emit(
        progress,
        Progress::Stage {
            stage: "decode",
            message: format!("decoding {} via ffmpeg", source.display()),
        },
    );

    // NOTE: `tempfile()` keeps an open file handle which, on Windows, blocks
    // whisper-cli from reading the wav we hand it. `into_temp_path()` releases
    // the handle while keeping RAII deletion of the file when the path drops.
    let wav_path = tempfile::Builder::new()
        .prefix("anubis-extractor-")
        .suffix(".wav")
        .tempfile()
        .map_err(ExtractorError::Io)?
        .into_temp_path();

    let ffmpeg_status = Command::new(&artifacts.ffmpeg_exe)
        .args([
            "-y",
            "-loglevel",
            "error",
            "-i",
            &source.to_string_lossy(),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
            &wav_path.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ExtractorError::Ffmpeg(format!("spawn ffmpeg: {e}")))?;

    if !ffmpeg_status.status.success() {
        let stderr = String::from_utf8_lossy(&ffmpeg_status.stderr).into_owned();
        return Err(ExtractorError::Ffmpeg(format!(
            "ffmpeg exited {:?}: {stderr}",
            ffmpeg_status.status.code()
        )));
    }

    let _ = emit(
        progress,
        Progress::Stage {
            stage: "transcribe",
            message: format!("running whisper on {}", wav_path.display()),
        },
    );

    let mut cmd = Command::new(&artifacts.whisper_exe);
    cmd.args([
        "-m",
        &artifacts.whisper_model.to_string_lossy(),
        "-f",
        &wav_path.to_string_lossy(),
        "-pp",
    ]);
    if let Some(lang) = language {
        cmd.args(["-l", lang]);
    }
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ExtractorError::Whisper(format!("spawn whisper: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(ExtractorError::Whisper(format!(
            "whisper exited {:?}: {stderr}",
            output.status.code()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let segments = parse_segments(&stdout, progress);
    let text = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    let language = parse_language(&String::from_utf8_lossy(&output.stderr));

    Ok(TranscribeResult {
        text,
        segments,
        language,
        sidecar_path: None,
        cache_hit: false,
    })
}

fn parse_segments(
    stdout: &str,
    progress: Option<&tokio::sync::mpsc::Sender<Progress>>,
) -> Vec<Segment> {
    // Each line: `[hh:mm:ss.SSS --> hh:mm:ss.SSS]  text`
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('[') {
            continue;
        }
        let Some(close) = line.find(']') else {
            continue;
        };
        let bracket = &line[1..close];
        let Some(arrow) = bracket.find("-->") else {
            continue;
        };
        let start = parse_hms(bracket[..arrow].trim());
        let end = parse_hms(bracket[arrow + 3..].trim());
        let (Some(start_ms), Some(end_ms)) = (start, end) else {
            continue;
        };
        let text = line[close + 1..].trim().to_string();
        if text.is_empty() {
            continue;
        }
        let idx = out.len();
        let _ = emit(
            progress,
            Progress::Segment {
                index: idx,
                total: None,
                text: text.clone(),
            },
        );
        out.push(Segment {
            start_ms,
            end_ms,
            text,
        });
    }
    out
}

/// Parse `hh:mm:ss.SSS` → milliseconds.
fn parse_hms(s: &str) -> Option<u64> {
    let (hms, ms) = match s.split_once('.') {
        Some((h, m)) => (h, m),
        None => (s, "0"),
    };
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let sec: u64 = parts.next()?.parse().ok()?;
    let ms_pad = format!("{:0<3}", ms);
    let ms_val: u64 = ms_pad[..3].parse().ok()?;
    Some(((h * 3600) + (m * 60) + sec) * 1000 + ms_val)
}

fn parse_language(stderr: &str) -> Option<String> {
    // whisper.cpp prints "whisper_full_with_state: auto-detected language: en"
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix("whisper_full_with_state: auto-detected language: ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn emit(
    progress: Option<&tokio::sync::mpsc::Sender<Progress>>,
    ev: Progress,
) -> Result<(), ExtractorError> {
    if let Some(tx) = progress {
        match tx.try_send(ev) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Err(ExtractorError::Cancelled)
            }
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hms_handles_basic_format() {
        assert_eq!(parse_hms("00:00:01.500"), Some(1500));
        assert_eq!(parse_hms("01:02:03.004"), Some(3723004));
        assert_eq!(parse_hms("00:00:00.000"), Some(0));
    }

    #[test]
    fn parse_segments_pulls_text_after_bracket() {
        let stdout = "\
[00:00:00.000 --> 00:00:01.500]  Hello world.
[00:00:01.500 --> 00:00:03.000]  How are you?
";
        let segs = parse_segments(stdout, None);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Hello world.");
        assert_eq!(segs[0].start_ms, 0);
        assert_eq!(segs[0].end_ms, 1500);
        assert_eq!(segs[1].text, "How are you?");
    }

    #[test]
    fn parse_segments_skips_garbage_lines() {
        let stdout = "junk\n[bad bracket\n[00:00:00.000 --> 00:00:01.000] ok\n";
        let segs = parse_segments(stdout, None);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "ok");
    }
}
