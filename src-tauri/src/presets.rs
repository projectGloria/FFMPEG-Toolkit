//! Built-in preset commands. They are plain `CustomCommand`s once installed:
//! seeded into an empty store on first run, and re-addable per category from
//! the UI (`add_presets`). After that the user owns them like any other command.
//!
//! Quality policy: every preset targets the best output its format allows.
//! - Lossless where the format supports it (remux/copy, FLAC, ALAC, WAV, PNG...).
//! - Lossy video uses low CRF (visually transparent); the x264/x265 preset only
//!   trades encode time for file size at that quality, so `slow` is the sweet spot.
//! - Lossy audio at each codec's transparent ceiling (MP3 320k, Vorbis q10...).

use crate::commands::{CustomCommand, FileCategory};
use uuid::Uuid;

struct Preset {
    label: String,
    output_name: &'static str,
    output_ext: &'static str,
    args: String,
    group: &'static str,
}

impl Preset {
    fn in_group(mut self, group: &'static str) -> Self {
        self.group = group;
        self
    }
}

/// Explorer drops every entry past the 16th in a static cascading menu, so the
/// video list (25 entries) is split into submenus that each stay well under it.
const G_CONVERT: &str = "Convert video";
const G_AUDIO: &str = "Extract audio";
const G_SUBS: &str = "Extract subtitles";

fn p(label: impl Into<String>, output_name: &'static str, output_ext: &'static str, args: impl Into<String>) -> Preset {
    Preset {
        label: label.into(),
        output_name,
        output_ext,
        args: args.into(),
        group: "",
    }
}

/// Audio encoders shared by "Video → audio" and "Audio → audio".
/// `cover` = the format can carry embedded cover art (attached picture).
const AUDIO_TARGETS: &[(&str, &str, &str, &str, bool)] = &[
    // (label suffix, output_name, ext, codec args, cover)
    ("MP3 (320 kbps)", "{name}", "mp3", "-c:a libmp3lame -b:a 320k -id3v2_version 3", true),
    ("FLAC (lossless)", "{name}", "flac", "-c:a flac -compression_level 12", true),
    ("WAV (24-bit PCM)", "{name}", "wav", "-c:a pcm_s24le -rf64 auto", false),
    ("AIFF (24-bit PCM)", "{name}", "aiff", "-c:a pcm_s24be", false),
    ("M4A (ALAC lossless)", "{name}_alac", "m4a", "-c:a alac -movflags +faststart", true),
    ("M4A (AAC 320 kbps)", "{name}", "m4a", "-c:a aac -b:a 320k -movflags +faststart", true),
    ("OGG (Vorbis q10)", "{name}", "ogg", "-c:a libvorbis -q:a 10", false),
    ("Opus (256 kbps)", "{name}", "opus", "-c:a libopus -b:a 256k -vbr on", false),
];

/// libopus rejects some layouts (e.g. `5.1(side)` from most MKVs) unless they're
/// normalized to one of the standard Vorbis-order layouts first.
const OPUS_LAYOUT_FIX: &str = r#"-af "aformat=channel_layouts=7.1|5.1|stereo|mono""#;

fn audio_args(codec: &str, ext: &str, from_video: bool, cover: bool) -> String {
    let map = if !from_video && cover {
        // Keep embedded album art (attached picture) where the container supports it.
        "-map 0:a:0 -map 0:v? -c:v copy -disposition:v attached_pic"
    } else {
        // First audio track only; `-vn` so no video/cover sneaks in.
        "-vn -map 0:a:0"
    };
    let opus_fix = if ext == "opus" { OPUS_LAYOUT_FIX } else { "" };
    ["-i {input}", map, codec, opus_fix, "{output}"]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn video_presets() -> Vec<Preset> {
    // `0:V` (capital) = real video streams, excluding cover-art attached pictures.
    // `0:a?` = every audio track, and don't fail on silent sources.
    let mut v = vec![
        p(
            "Convert to MP4 (H.264 · near-lossless)",
            "{name}_h264",
            "mp4",
            "-i {input} -map 0:V:0 -map 0:a? -c:v libx264 -preset slow -crf 16 -pix_fmt yuv420p -c:a aac -b:a 320k -movflags +faststart {output}",
        ),
        p(
            "Convert to MP4 (H.265 10-bit · near-lossless)",
            "{name}_hevc",
            "mp4",
            // hvc1 tag = plays in QuickTime/iOS; 10-bit avoids banding even from 8-bit sources.
            "-i {input} -map 0:V:0 -map 0:a? -c:v libx265 -preset slow -crf 18 -pix_fmt yuv420p10le -tag:v hvc1 -x265-params log-level=error -c:a aac -b:a 320k -movflags +faststart {output}",
        ),
        p(
            "Remux to MP4 (no re-encode · lossless)",
            "{name}_remux",
            "mp4",
            "-i {input} -map 0:V -map 0:a? -c copy -movflags +faststart {output}",
        ),
        p(
            "Remux to MKV (no re-encode · lossless)",
            "{name}",
            "mkv",
            // All video/audio/subtitle tracks + attachments (fonts for ASS), bit-exact.
            "-i {input} -map 0:V -map 0:a? -map 0:s? -map 0:t? -c copy {output}",
        ),
        p(
            "Convert to MKV (AV1 10-bit · original audio & subs)",
            "{name}_av1",
            "mkv",
            "-i {input} -map 0:V:0 -map 0:a? -map 0:s? -map 0:t? -c copy -c:v libsvtav1 -preset 4 -crf 20 -pix_fmt yuv420p10le {output}",
        ),
        p(
            "Convert to WebM (VP9 · near-lossless)",
            "{name}_vp9",
            "webm",
            // `-b:v 0` = pure constant-quality mode; row-mt = multithreaded.
            format!(
                "-i {{input}} -map 0:V:0 -map 0:a? -c:v libvpx-vp9 -crf 18 -b:v 0 -deadline good -cpu-used 2 -row-mt 1 -pix_fmt yuv420p -c:a libopus -b:a 256k {OPUS_LAYOUT_FIX} {{output}}"
            ),
        ),
        p(
            "Convert to WebM (AV1 10-bit · near-lossless)",
            "{name}_av1",
            "webm",
            format!(
                "-i {{input}} -map 0:V:0 -map 0:a? -c:v libsvtav1 -preset 4 -crf 22 -pix_fmt yuv420p10le -c:a libopus -b:a 256k {OPUS_LAYOUT_FIX} {{output}}"
            ),
        ),
        p(
            "Convert to MOV (ProRes 422 HQ · editing)",
            "{name}_prores",
            "mov",
            "-i {input} -map 0:V:0 -map 0:a? -c:v prores_ks -profile:v 3 -vendor apl0 -pix_fmt yuv422p10le -c:a pcm_s24le {output}",
        ),
        p(
            "Convert to AVI (Xvid · legacy players)",
            "{name}",
            "avi",
            "-i {input} -map 0:V:0 -map 0:a:0? -c:v libxvid -q:v 2 -vtag XVID -c:a libmp3lame -b:a 320k {output}",
        ),
        p(
            "Convert to GIF (high-quality palette)",
            "{name}",
            "gif",
            // Per-video optimal palette + error-diffusion dithering; capped at 720 px wide.
            r#"-i {input} -an -vf "fps=20,scale='min(720,iw)':-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle" -loop 0 {output}"#,
        ),
        p(
            "Convert to WhatsApp video (HD 720p)",
            "{name}_whatsapp",
            "mp4",
            // WhatsApp HD caps at 720p/30 and re-compresses anything bigger, so we
            // hand it exactly that: fit inside 1280x1280 (portrait-safe), never
            // upscale, High profile, ≤30 fps, bounded bitrate, stereo AAC.
            r#"-i {input} -map 0:V:0 -map 0:a:0? -vf "scale='min(1280,iw)':'min(1280,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2" -fpsmax 30 -c:v libx264 -preset slow -crf 20 -maxrate 6M -bufsize 12M -profile:v high -level 4.0 -pix_fmt yuv420p -c:a aac -b:a 192k -ac 2 -movflags +faststart {output}"#,
        ),
    ];
    v.iter_mut().for_each(|p| p.group = G_CONVERT);

    v.push(
        p(
            "Extract audio (original · no re-encode)",
            "{name}_audio",
            "mka",
            "-i {input} -vn -map 0:a -c copy {output}",
        )
        .in_group(G_AUDIO),
    );
    for &(suffix, name, ext, codec, _) in AUDIO_TARGETS {
        v.push(p(format!("Extract audio to {suffix}"), name, ext, audio_args(codec, ext, true, false)).in_group(G_AUDIO));
    }

    // `disp:hearing_impaired` needs FFmpeg 7.1+. `:0` = first match, since
    // single-stream formats (srt/vtt/ass) refuse a second subtitle stream.
    const SDH: &str = "0:s:disp:hearing_impaired:0";
    v.extend([
        p("Extract SDH subtitle to SRT", "{name}.sdh", "srt", format!("-i {{input}} -map {SDH} -c:s srt {{output}}")),
        p("Extract SDH subtitle to VTT", "{name}.sdh", "vtt", format!("-i {{input}} -map {SDH} -c:s webvtt {{output}}")),
        p("Extract SDH subtitle to ASS", "{name}.sdh", "ass", format!("-i {{input}} -map {SDH} -c:s ass {{output}}")),
        p("Extract first subtitle to SRT", "{name}", "srt", "-i {input} -map 0:s:0 -c:s srt {output}"),
        p(
            "Extract all subtitles (original · MKS)",
            "{name}_subs",
            "mks",
            // Bit-exact copy of every subtitle track (text or bitmap) + fonts.
            "-i {input} -map 0:s -map 0:t? -c copy -f matroska {output}",
        ),
    ]
    .map(|p| p.in_group(G_SUBS)));
    v
}

fn audio_presets() -> Vec<Preset> {
    AUDIO_TARGETS
        .iter()
        .map(|&(suffix, name, ext, codec, cover)| p(format!("Convert to {suffix}"), name, ext, audio_args(codec, ext, false, cover)))
        .collect()
}

fn image_presets() -> Vec<Preset> {
    // `-frames:v 1` = one still even from animated GIF/WebP input.
    vec![
        p("Convert to PNG (lossless)", "{name}", "png", "-i {input} -frames:v 1 -c:v png -compression_level 9 -pred mixed {output}"),
        p(
            "Convert to JPG (max quality · 4:4:4)",
            "{name}",
            "jpg",
            // q 1 + full-resolution chroma: the best baseline JPEG can do.
            "-i {input} -frames:v 1 -c:v mjpeg -q:v 1 -qmin 1 -pix_fmt yuvj444p {output}",
        ),
        p("Convert to WebP (lossless)", "{name}", "webp", "-i {input} -frames:v 1 -c:v libwebp -lossless 1 -compression_level 6 -quality 100 {output}"),
        p(
            "Convert to AVIF (near-lossless)",
            "{name}",
            "avif",
            "-i {input} -frames:v 1 -c:v libaom-av1 -still-picture 1 -crf 10 -b:v 0 -cpu-used 4 -pix_fmt yuv444p {output}",
        ),
        p("Convert to JPEG XL (lossless)", "{name}", "jxl", "-i {input} -frames:v 1 -c:v libjxl -distance 0 -effort 7 {output}"),
        p("Convert to TIFF (lossless)", "{name}", "tif", "-i {input} -frames:v 1 -c:v tiff -compression_algo deflate {output}"),
    ]
}

fn subtitle_presets() -> Vec<Preset> {
    vec![
        p("Convert to SRT", "{name}", "srt", "-i {input} -c:s srt {output}"),
        p("Convert to VTT", "{name}", "vtt", "-i {input} -c:s webvtt {output}"),
        p("Convert to ASS", "{name}", "ass", "-i {input} -c:s ass {output}"),
    ]
}

/// Fresh commands (new uuids) for one category, in menu order.
pub fn presets_for(category: FileCategory) -> Vec<CustomCommand> {
    let list = match category {
        FileCategory::Video => video_presets(),
        FileCategory::Audio => audio_presets(),
        FileCategory::Image => image_presets(),
        FileCategory::Subtitle => subtitle_presets(),
    };
    list.into_iter()
        .map(|p| CustomCommand {
            id: Uuid::new_v4().to_string(),
            category,
            label: p.label,
            args: p.args,
            output_name: p.output_name.to_string(),
            output_ext: p.output_ext.to_string(),
            enabled: true,
            group: p.group.to_string(),
        })
        .collect()
}

pub fn all_presets() -> Vec<CustomCommand> {
    FileCategory::ALL.iter().flat_map(|&c| presets_for(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{resolve, validate};

    #[test]
    fn presets_are_valid_and_unique() {
        let all = all_presets();
        for c in &all {
            validate(c).unwrap_or_else(|e| panic!("{}: {}", c.label, e));
            let r = resolve(c, r"C:\My Videos\clip one.mkv").unwrap();
            assert!(r.warnings.is_empty(), "{} has warnings", c.label);
        }
        let mut labels: Vec<_> = all.iter().map(|c| (c.category, &c.label)).collect();
        labels.sort_by_key(|(c, l)| (*c as u8, l.to_string()));
        let n = labels.len();
        labels.dedup();
        assert_eq!(n, labels.len(), "duplicate preset label");
    }

    #[test]
    fn opus_filter_survives_tokenizing() {
        let c = presets_for(FileCategory::Audio).into_iter().find(|c| c.output_ext == "opus").unwrap();
        let r = resolve(&c, r"C:\a.flac").unwrap();
        let i = r.argv.iter().position(|a| a == "-af").unwrap();
        assert_eq!(r.argv[i + 1], "aformat=channel_layouts=7.1|5.1|stereo|mono");
    }

    /// Runs every preset through real ffmpeg. Needs `FFTK_SAMPLES` = a dir with
    /// sample.mkv / sample.flac / sample.png / sample.srt (see test notes).
    /// `cargo test -- --ignored presets_run_in_ffmpeg`
    #[test]
    #[ignore]
    fn presets_run_in_ffmpeg() {
        let dir = std::env::var("FFTK_SAMPLES").expect("set FFTK_SAMPLES");
        let mut failed = Vec::new();
        for c in all_presets() {
            let sample = match c.category {
                FileCategory::Video => "sample.mkv",
                FileCategory::Audio => "sample.flac",
                FileCategory::Image => "sample.png",
                FileCategory::Subtitle => "sample.srt",
            };
            let input = format!("{}\\{}", dir, sample);
            let r = resolve(&c, &input).unwrap();
            let out = std::process::Command::new("ffmpeg")
                .args(["-hide_banner", "-nostdin", "-v", "error"])
                .args(&r.argv)
                .output()
                .unwrap();
            let ok = out.status.success()
                && r.output_path.as_ref().map(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)).unwrap_or(false);
            println!("{} {:?} -> {:?}", if ok { "OK  " } else { "FAIL" }, c.label, r.output_path);
            if !ok {
                failed.push(format!("{}: {}", c.label, String::from_utf8_lossy(&out.stderr)));
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}
