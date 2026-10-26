//! The H.264 level the encoder declares follows the stream it encodes.
//!
//! A 1920x1080 stream at 60 fps needs level 4.2 (489,600 macroblocks per second
//! is over 4.1's 245,760), and a 3840x2160 stream at 60 fps needs level 5.2
//! (1,944,000 is over 5.1's 983,040). Each stream is encoded with the level
//! derived from its size and frame rate, decoded again, and checked two ways:
//! the decoded luma has to match the source, and ffprobe has to read the
//! declared level out of the SPS.
//!
//! Ignored by default: requires a Vulkan Video device and ffmpeg (for ffprobe).
//! Run with `cargo test --test h264_level -- --ignored --nocapture`.

#[allow(dead_code)]
mod common;

use std::fs::File;
use std::io::{Read, Write};
use std::process::Command;

use common::{Readback, decode_stream, write_nv12};
use pixelforge::decoder::{DecodeConfig, Decoder};
use pixelforge::{
    Codec, EncodeBitDepth, EncodeConfig, EncodeFuture, Encoder, InputImage, PixelFormat,
    RateControlMode, VideoContextBuilder,
};

const GOP: u32 = 30;
const FRAMES: u32 = 6;
/// Lowest luma PSNR accepted for a CQP-26 round trip of the synthetic pattern.
const MIN_LUMA_PSNR_DB: f64 = 30.0;

/// One I420 frame of a moving pattern. Smooth enough to code well at QP 26,
/// with edges so the comparison is not trivially satisfied by a flat picture.
fn synthetic_frame(width: u32, height: u32, t: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = Vec::with_capacity(w * h * 3 / 2);
    for y in 0..h {
        for x in 0..w {
            let v = (x * 3 + y * 2 + t as usize * 7) ^ (((x >> 5) * (y >> 4)) & 0xff);
            out.push(v as u8);
        }
    }
    for plane in 0..2 {
        for y in 0..h / 2 {
            for x in 0..w / 2 {
                out.push((128 + plane * 16 + ((x + y + t as usize) % 48)) as u8);
            }
        }
    }
    out
}

/// Luma PSNR in dB between two equal-length planes.
fn psnr(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mse: f64 = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| {
            let d = x as f64 - y as f64;
            d * d
        })
        .sum::<f64>()
        / a.len() as f64;
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0 * 255.0 / mse).log10()
    }
}

/// The level ffprobe reads from the first SPS, as `level_idc` (42 for 4.2).
fn probed_level(path: &std::path::Path) -> u32 {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=level",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .expect("run ffprobe");
    assert!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let level: u32 = text.trim().parse().expect("ffprobe reports a level");
    level
}

/// Encode `FRAMES` synthetic frames at `width` x `height` and `fps`, decode
/// them, and check the level and the picture.
fn encode_decode_check(width: u32, height: u32, fps: u32, expected_level: u32, tag: &str) {
    common::init_logging();
    let dir = common::scratch_dir(&format!("h264_level_{tag}"));
    let bitstream_path = dir.join("out.h264");
    let yuv_path = dir.join("out.yuv");

    let context = VideoContextBuilder::new()
        .app_name("pixelforge-h264-level")
        .require_encode(Codec::H264)
        .require_decode(Codec::H264)
        .enable_validation(std::env::var("PIXELFORGE_VALIDATION").is_ok())
        .build()
        .expect("Vulkan Video device with H.264 encode and decode");

    // No level is set, so the encoder derives it from the size and rate.
    let config = EncodeConfig::h264(width, height)
        .with_rate_control(RateControlMode::Cqp)
        .with_quality_level(26)
        .with_frame_rate(fps, 1)
        .with_gop_size(GOP)
        .with_b_frames(0);
    assert_eq!(config.h264_level_idc, None);

    let sources: Vec<Vec<u8>> = (0..FRAMES)
        .map(|t| synthetic_frame(width, height, t))
        .collect();

    let mut input_image = InputImage::new(
        context.clone(),
        Codec::H264,
        width,
        height,
        EncodeBitDepth::Eight,
        PixelFormat::Yuv420,
    )
    .expect("input image");
    let mut encoder = Encoder::new(context.clone(), config).expect("encoder");

    let mut bitstream = Vec::new();
    let mut pending: std::collections::VecDeque<EncodeFuture> = Default::default();
    for source in &sources {
        input_image.upload_yuv420(source).expect("upload");
        pending.push_back(encoder.encode(input_image.image()).expect("encode"));
        while pending.len() > 2 {
            let packet =
                pollster::block_on(pending.pop_front().expect("non-empty")).expect("packet");
            bitstream.extend_from_slice(&packet.data);
        }
    }
    encoder.flush().expect("flush");
    while let Some(future) = pending.pop_front() {
        let packet = pollster::block_on(future).expect("packet");
        bitstream.extend_from_slice(&packet.data);
    }
    File::create(&bitstream_path)
        .and_then(|mut f| f.write_all(&bitstream))
        .expect("write bitstream");
    println!(
        "{tag}: encoded {FRAMES} frames at {width}x{height}@{fps} -> {} bytes",
        bitstream.len()
    );

    // The level the SPS declares, read by an independent decoder.
    let probed = probed_level(&bitstream_path);
    assert_eq!(
        probed, expected_level,
        "{tag}: SPS level_idc (ffprobe) is {probed}, expected {expected_level}"
    );
    println!("{tag}: ffprobe reads SPS level_idc {probed}");

    // Decode with pixelforge and compare each luma plane with its source.
    let mut out = Some(File::create(&yuv_path).expect("create yuv"));
    let mut readback = Some(Readback::new(&context).expect("readback"));
    let mut decoder =
        Decoder::new(context, DecodeConfig::h264().with_byte_stream()).expect("decoder");
    let mut decoded_count = 0usize;
    decode_stream(&mut decoder, &bitstream, |frame| {
        write_nv12(&frame, &mut readback, &mut out)?;
        decoded_count += 1;
        Ok(())
    })
    .expect("decode");
    drop(out);
    assert_eq!(
        decoded_count, FRAMES as usize,
        "{tag}: frames lost in decode"
    );

    let mut decoded = Vec::new();
    File::open(&yuv_path)
        .and_then(|mut f| f.read_to_end(&mut decoded))
        .expect("read decoded");
    let frame_size = (width * height * 3 / 2) as usize;
    let luma_len = (width * height) as usize;
    assert_eq!(
        decoded.len(),
        frame_size * FRAMES as usize,
        "{tag}: decoded size"
    );
    for (t, source) in sources.iter().enumerate() {
        let got = &decoded[t * frame_size..t * frame_size + luma_len];
        let db = psnr(&source[..luma_len], got);
        println!("{tag}: frame {t} luma PSNR {db:.2} dB");
        assert!(
            db >= MIN_LUMA_PSNR_DB,
            "{tag}: frame {t} luma PSNR {db:.2} dB is below {MIN_LUMA_PSNR_DB} dB"
        );
    }
    println!("{tag}: decoded {decoded_count} frames, all above {MIN_LUMA_PSNR_DB} dB");
}

#[test]
#[ignore = "requires a Vulkan Video device and ffmpeg"]
fn fhd60_stream_declares_level_4_2() {
    encode_decode_check(1920, 1080, 60, 42, "fhd60");
}

#[test]
#[ignore = "requires a Vulkan Video device and ffmpeg"]
fn uhd60_stream_declares_level_5_2() {
    encode_decode_check(3840, 2160, 60, 52, "uhd60");
}

#[test]
#[ignore = "requires a Vulkan Video device"]
fn explicit_level_too_small_is_refused() {
    common::init_logging();
    let context = VideoContextBuilder::new()
        .app_name("pixelforge-h264-level")
        .require_encode(Codec::H264)
        .build()
        .expect("Vulkan Video device with H.264 encode");

    // 1080p60 needs 4.2; asking for 4.1 must fail rather than be bumped.
    let config = EncodeConfig::h264(1920, 1080)
        .with_frame_rate(60, 1)
        .with_h264_level_idc(41);
    let err = Encoder::new(context, config).err().expect("refused");
    let message = err.to_string();
    assert!(
        message.contains("cannot carry"),
        "unexpected error: {message}"
    );
    println!("refused as expected: {message}");
}
