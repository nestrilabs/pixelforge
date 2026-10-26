//! H.264 level selection: the limits from Table A-1 of the specification and
//! the rule that picks the level a stream needs.
//!
//! A level bounds how many macroblocks a picture may have (`MaxFS`), how many
//! macroblocks may be decoded per second (`MaxMBPS`), and how many frames the
//! decoded picture buffer may hold (`MaxDpbMbs`, which becomes a frame count
//! once divided by the picture size). The encoder declares one level in the
//! SPS, so it has to be one the stream fits in and one the device can encode.

use crate::error::{PixelForgeError, Result};
use ash::vk;

/// One row of Table A-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Level {
    /// `level_idc` as written in the SPS: the level times ten, so 4.2 is 42.
    pub idc: u8,
    /// Maximum macroblocks decoded per second.
    pub max_mbps: u64,
    /// Maximum frame size in macroblocks.
    pub max_fs: u32,
    /// Maximum decoded picture buffer size in macroblocks.
    pub max_dpb_mbs: u32,
}

/// The H.264 levels this encoder can declare, from Table A-1.
///
/// Level 1b is left out: it is signalled as level 1.1 with `constraint_set3`,
/// which only the Baseline and Main profiles use, and the encoder emits High
/// and High 4:4:4 Predictive.
pub(crate) const LEVELS: &[Level] = &[
    Level {
        idc: 10,
        max_mbps: 1_485,
        max_fs: 99,
        max_dpb_mbs: 396,
    },
    Level {
        idc: 11,
        max_mbps: 3_000,
        max_fs: 396,
        max_dpb_mbs: 900,
    },
    Level {
        idc: 12,
        max_mbps: 6_000,
        max_fs: 396,
        max_dpb_mbs: 2_376,
    },
    Level {
        idc: 13,
        max_mbps: 11_880,
        max_fs: 396,
        max_dpb_mbs: 2_376,
    },
    Level {
        idc: 20,
        max_mbps: 11_880,
        max_fs: 396,
        max_dpb_mbs: 2_376,
    },
    Level {
        idc: 21,
        max_mbps: 19_800,
        max_fs: 792,
        max_dpb_mbs: 4_752,
    },
    Level {
        idc: 22,
        max_mbps: 20_250,
        max_fs: 1_620,
        max_dpb_mbs: 8_100,
    },
    Level {
        idc: 30,
        max_mbps: 40_500,
        max_fs: 1_620,
        max_dpb_mbs: 8_100,
    },
    Level {
        idc: 31,
        max_mbps: 108_000,
        max_fs: 3_600,
        max_dpb_mbs: 18_000,
    },
    Level {
        idc: 32,
        max_mbps: 216_000,
        max_fs: 5_120,
        max_dpb_mbs: 20_480,
    },
    Level {
        idc: 40,
        max_mbps: 245_760,
        max_fs: 8_192,
        max_dpb_mbs: 32_768,
    },
    Level {
        idc: 41,
        max_mbps: 245_760,
        max_fs: 8_192,
        max_dpb_mbs: 32_768,
    },
    Level {
        idc: 42,
        max_mbps: 522_240,
        max_fs: 8_704,
        max_dpb_mbs: 34_816,
    },
    Level {
        idc: 50,
        max_mbps: 589_824,
        max_fs: 22_080,
        max_dpb_mbs: 110_400,
    },
    Level {
        idc: 51,
        max_mbps: 983_040,
        max_fs: 36_864,
        max_dpb_mbs: 184_320,
    },
    Level {
        idc: 52,
        max_mbps: 2_073_600,
        max_fs: 36_864,
        max_dpb_mbs: 184_320,
    },
    Level {
        idc: 60,
        max_mbps: 4_177_920,
        max_fs: 139_264,
        max_dpb_mbs: 696_320,
    },
    Level {
        idc: 61,
        max_mbps: 8_355_840,
        max_fs: 139_264,
        max_dpb_mbs: 696_320,
    },
    Level {
        idc: 62,
        max_mbps: 16_711_680,
        max_fs: 139_264,
        max_dpb_mbs: 696_320,
    },
];

/// The table row for a `level_idc`, if there is one.
pub(crate) fn lookup(idc: u8) -> Option<&'static Level> {
    LEVELS.iter().find(|level| level.idc == idc)
}

/// The `StdVideoH264LevelIdc` for a `level_idc`, if there is one.
pub(crate) fn to_std(idc: u8) -> Option<vk::native::StdVideoH264LevelIdc> {
    use vk::native::*;
    Some(match idc {
        10 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0,
        11 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1,
        12 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2,
        13 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3,
        20 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0,
        21 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1,
        22 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2,
        30 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0,
        31 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1,
        32 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2,
        40 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0,
        41 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1,
        42 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2,
        50 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0,
        51 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1,
        52 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2,
        60 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0,
        61 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1,
        62 => StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2,
        _ => return None,
    })
}

/// The `level_idc` for a `StdVideoH264LevelIdc`, if it is one this table has.
pub(crate) fn from_std(std: vk::native::StdVideoH264LevelIdc) -> Option<u8> {
    LEVELS
        .iter()
        .map(|level| level.idc)
        .find(|&idc| to_std(idc) == Some(std))
}

/// Macroblocks in a picture of the given size in macroblocks.
pub(crate) fn frame_macroblocks(width_mbs: u32, height_mbs: u32) -> u32 {
    width_mbs * height_mbs
}

/// Macroblocks per second for a picture of `frame_mbs` at the given frame rate,
/// rounded up: a rate that is a fraction of a macroblock over the limit is over
/// the limit.
pub(crate) fn macroblocks_per_second(frame_mbs: u32, fps_num: u32, fps_den: u32) -> u64 {
    let num = fps_num.max(1) as u64;
    let den = fps_den.max(1) as u64;
    (frame_mbs as u64 * num).div_ceil(den)
}

/// Whether a picture of `frame_mbs` at `mbps` macroblocks per second is within
/// the limits of `level`.
pub(crate) fn fits(level: &Level, frame_mbs: u32, mbps: u64) -> bool {
    frame_mbs <= level.max_fs && mbps <= level.max_mbps
}

/// The lowest level whose limits a picture of `frame_mbs` at `mbps` fits.
pub(crate) fn lowest_fitting(frame_mbs: u32, mbps: u64) -> Option<&'static Level> {
    LEVELS.iter().find(|level| fits(level, frame_mbs, mbps))
}

/// Frames the decoded picture buffer holds at this level, capped at the
/// specification's 16.
pub(crate) fn max_dpb_frames(level: &Level, frame_mbs: u32) -> u32 {
    (level.max_dpb_mbs / frame_mbs.max(1)).min(16)
}

/// The level the stream is declared at.
///
/// With `explicit` set, that level is used, provided the picture and frame rate
/// fit it. Without it, the lowest level that fits is used. Either way the level
/// must not exceed `device_max`, the highest `level_idc` the device reports for
/// encoding, and nothing is clamped: a stream the chosen level cannot carry is
/// an error.
pub(crate) fn resolve(
    explicit: Option<u8>,
    frame_mbs: u32,
    fps_num: u32,
    fps_den: u32,
    device_max: Option<u8>,
) -> Result<u8> {
    let mbps = macroblocks_per_second(frame_mbs, fps_num, fps_den);

    let level = match explicit {
        Some(idc) => {
            let level = lookup(idc).ok_or_else(|| {
                PixelForgeError::InvalidInput(format!(
                    "Unknown H.264 level_idc {idc}; expected one of {}",
                    LEVELS
                        .iter()
                        .map(|l| l.idc.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
            if !fits(level, frame_mbs, mbps) {
                return Err(PixelForgeError::InvalidInput(format!(
                    "H.264 level {} cannot carry this stream: {frame_mbs} macroblocks per frame \
                     at {mbps} macroblocks per second needs MaxFS >= {frame_mbs} and MaxMBPS >= \
                     {mbps}, but level {} allows {} and {}",
                    fmt_level(idc),
                    fmt_level(idc),
                    level.max_fs,
                    level.max_mbps
                )));
            }
            level
        }
        None => lowest_fitting(frame_mbs, mbps).ok_or_else(|| {
            PixelForgeError::InvalidInput(format!(
                "No H.264 level carries {frame_mbs} macroblocks per frame at {mbps} macroblocks \
                 per second (the highest level allows MaxFS 139264 and MaxMBPS 16711680)"
            ))
        })?,
    };

    if let Some(max) = device_max
        && level.idc > max
    {
        return Err(PixelForgeError::NoSuitableDevice(format!(
            "The stream needs H.264 level {}, but the device reports maxLevelIdc {}",
            fmt_level(level.idc),
            fmt_level(max)
        )));
    }
    Ok(level.idc)
}

/// `level_idc` as the dotted level, `42` as `4.2`.
pub(crate) fn fmt_level(idc: u8) -> String {
    format!("{}.{}", idc / 10, idc % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Macroblocks in a 1920x1080 picture (120 by 68, the 1088-line coded size).
    const FHD_MBS: u32 = 120 * 68;
    /// Macroblocks in a 3840x2160 picture.
    const UHD_MBS: u32 = 240 * 135;

    #[test]
    fn table_is_sorted_and_every_level_has_a_std_enum() {
        for pair in LEVELS.windows(2) {
            assert!(
                pair[0].idc < pair[1].idc,
                "table out of order at {}",
                pair[1].idc
            );
        }
        for level in LEVELS {
            let std = to_std(level.idc).expect("every table level maps to StdVideo");
            assert_eq!(from_std(std), Some(level.idc));
        }
    }

    #[test]
    fn table_values_match_table_a1() {
        // Spot checks on the rows the encoder's sizes depend on.
        assert_eq!(
            lookup(41).map(|l| (l.max_mbps, l.max_fs)),
            Some((245_760, 8_192))
        );
        assert_eq!(
            lookup(42).map(|l| (l.max_mbps, l.max_fs)),
            Some((522_240, 8_704))
        );
        assert_eq!(
            lookup(51).map(|l| (l.max_mbps, l.max_fs)),
            Some((983_040, 36_864))
        );
        assert_eq!(
            lookup(52).map(|l| (l.max_mbps, l.max_fs)),
            Some((2_073_600, 36_864))
        );
        assert_eq!(lookup(42).map(|l| l.max_dpb_mbs), Some(34_816));
        assert_eq!(lookup(52).map(|l| l.max_dpb_mbs), Some(184_320));
    }

    #[test]
    fn macroblock_rate_rounds_up() {
        assert_eq!(macroblocks_per_second(FHD_MBS, 60, 1), 489_600);
        assert_eq!(macroblocks_per_second(UHD_MBS, 60, 1), 1_944_000);
        // 30000/1001 fps: the rate is not whole, so round up.
        assert_eq!(macroblocks_per_second(1, 30_000, 1_001), 30);
        assert_eq!(macroblocks_per_second(1, 1, 3), 1);
    }

    #[test]
    fn lowest_level_for_known_sizes() {
        // 1080p60 is 489,600 MB/s: over 4.1's 245,760, under 4.2's 522,240.
        let fhd60 = lowest_fitting(FHD_MBS, 489_600).expect("fits");
        assert_eq!(fhd60.idc, 42);
        // 1080p30 is 244,800 MB/s: just under 4.0's and 4.1's 245,760, which
        // share their limits, so the lowest is 4.0.
        let fhd30 = lowest_fitting(FHD_MBS, 244_800).expect("fits");
        assert_eq!(fhd30.idc, 40);
        // 4K60 is 1,944,000 MB/s: over 5.1's 983,040, under 5.2's 2,073,600.
        let uhd60 = lowest_fitting(UHD_MBS, 1_944_000).expect("fits");
        assert_eq!(uhd60.idc, 52);
        // 4K30 fits 5.1.
        let uhd30 = lowest_fitting(UHD_MBS, 972_000).expect("fits");
        assert_eq!(uhd30.idc, 51);
    }

    #[test]
    fn frame_size_alone_can_raise_the_level() {
        // A tiny picture at a huge rate is limited by MaxMBPS, a huge picture at
        // a low rate by MaxFS: 139264 MBs is the largest frame in the table.
        let level = lowest_fitting(139_264, 139_264 * 30).expect("fits");
        assert_eq!(level.idc, 60);
        assert!(lowest_fitting(139_265, 1).is_none());
    }

    #[test]
    fn dpb_frames_from_max_dpb_mbs() {
        // 1080p at 4.2 holds four frames, 4K at 5.2 five.
        assert_eq!(max_dpb_frames(lookup(42).unwrap(), FHD_MBS), 4);
        assert_eq!(max_dpb_frames(lookup(52).unwrap(), UHD_MBS), 5);
        assert_eq!(max_dpb_frames(lookup(10).unwrap(), 99), 4);
    }

    #[test]
    fn resolve_derives_the_level() {
        assert_eq!(resolve(None, FHD_MBS, 60, 1, Some(52)).unwrap(), 42);
        assert_eq!(resolve(None, UHD_MBS, 60, 1, Some(52)).unwrap(), 52);
    }

    #[test]
    fn resolve_rejects_a_level_the_device_lacks() {
        let err = resolve(None, UHD_MBS, 60, 1, Some(51)).unwrap_err();
        assert!(err.to_string().contains("5.2"), "{err}");
        assert!(err.to_string().contains("5.1"), "{err}");
    }

    #[test]
    fn resolve_keeps_an_explicit_level_and_refuses_one_that_is_too_small() {
        assert_eq!(resolve(Some(52), FHD_MBS, 60, 1, Some(52)).unwrap(), 52);
        let err = resolve(Some(41), FHD_MBS, 60, 1, Some(52)).unwrap_err();
        assert!(err.to_string().contains("cannot carry"), "{err}");
    }

    #[test]
    fn resolve_rejects_an_unknown_level() {
        let err = resolve(Some(43), FHD_MBS, 60, 1, None).unwrap_err();
        assert!(err.to_string().contains("Unknown"), "{err}");
    }
}
