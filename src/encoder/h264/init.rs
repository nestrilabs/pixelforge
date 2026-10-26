use super::{H264, MB_SIZE, level};

use crate::encoder::codec::{
    CodecEncoder, CommonInitRequest, RgbConversionCaps, build_encoder_common, coded_extent,
    query_video_caps, rgb_conversion_profile,
};
use crate::encoder::dpb::{DecodedPictureBuffer, DecodedPictureBufferTrait, DpbConfig};
use crate::encoder::resources::MIN_BITSTREAM_BUFFER_SIZE;
use crate::encoder::{ColorDescription, EncodeConfig, PixelFormat};
use crate::error::{PixelForgeError, Result};
use crate::vulkan::VideoContext;
use ash::vk;
use ash::vk::TaggedStructure;
use tracing::{debug, info, warn};

impl H264 {
    /// Create a new H.264 encoder.
    pub fn create(context: VideoContext, config: EncodeConfig) -> Result<CodecEncoder<Self>> {
        // B-frames are not yet supported. `with_b_frames` is public and accepts
        // any count, so this is reachable from safe caller code: report it
        // rather than panicking inside the library.
        if config.b_frame_count != 0 {
            return Err(PixelForgeError::InvalidInput(format!(
                "H.264 encode: B-frames are not yet supported; set b_frame_count = 0 (got {})",
                config.b_frame_count
            )));
        }

        info!(
            "Creating H.264 encoder: {}x{}, pixel_format={:?}",
            config.dimensions.width, config.dimensions.height, config.pixel_format
        );

        // Select the profile based on chroma subsampling.
        let profile_idc = match config.pixel_format {
            PixelFormat::Yuv444 => {
                ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH_444_PREDICTIVE
            }
            _ => ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
        };

        let chroma_subsampling: vk::VideoChromaSubsamplingFlagsKHR = config.pixel_format.into();
        let bit_depth: vk::VideoComponentBitDepthFlagsKHR = config.bit_depth.into();

        // Get encoder tuning mode, usage and content hints.
        let encode_usage_hint: vk::VideoEncodeUsageFlagsKHR = config.encode_usage_hint.into();
        let encode_content_hint: vk::VideoEncodeContentFlagsKHR = config.encode_content_hint.into();
        let encoder_tuning_mode: vk::VideoEncodeTuningModeKHR = config.encoder_tuning_mode.into();

        let mut video_encode_usage_info = vk::VideoEncodeUsageInfoKHR::default()
            .video_usage_hints(encode_usage_hint)
            .video_content_hints(encode_content_hint)
            .tuning_mode(encoder_tuning_mode);

        let mut h264_profile_info =
            vk::VideoEncodeH264ProfileInfoKHR::default().std_profile_idc(profile_idc);
        let mut rgb_profile = rgb_conversion_profile();
        let mut profile_info = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::ENCODE_H264)
            .chroma_subsampling(chroma_subsampling)
            .luma_bit_depth(bit_depth)
            .chroma_bit_depth(bit_depth)
            .push(&mut h264_profile_info)
            .push(&mut video_encode_usage_info);
        if config.rgb_input.is_some() {
            profile_info = profile_info.push(&mut rgb_profile);
        }

        // The driver's preferred entropy mode is H.264-specific (some require
        // CAVLC for High 4:4:4 Predictive).
        let preferred_entropy_cabac = query_preferred_entropy_cabac(&context, &profile_info);

        // Query device capabilities with the H.264-specific capability struct
        // chained in (required by the driver).
        let mut h264_caps = vk::VideoEncodeH264CapabilitiesKHR::default();
        let mut encode_caps = vk::VideoEncodeCapabilitiesKHR::default();
        // Chained even when intra refresh was not asked for: the answer is
        // wanted for the log either way, and a driver that does not know the
        // struct leaves it at its defaults rather than failing.
        let mut intra_refresh_caps = vk::VideoEncodeIntraRefreshCapabilitiesKHR::default();
        // Chained on the same terms as intra refresh: a driver that does not
        // know these leaves them at their defaults, and a zero delta range
        // reads correctly as "no delta map here".
        let mut qp_map_caps = vk::VideoEncodeQuantizationMapCapabilitiesKHR::default();
        let mut qp_map_codec_caps = vk::VideoEncodeH264QuantizationMapCapabilitiesKHR::default();
        let mut capabilities = vk::VideoCapabilitiesKHR::default()
            .push(&mut encode_caps)
            .push(&mut h264_caps)
            .push(&mut intra_refresh_caps)
            .push(&mut qp_map_caps)
            .push(&mut qp_map_codec_caps);
        let mut rgb_caps = vk::VideoEncodeRgbConversionCapabilitiesVALVE::default();
        if config.rgb_input.is_some() {
            capabilities = capabilities.push(&mut rgb_caps);
        }
        let caps = query_video_caps(&context, &profile_info, &mut capabilities)?;
        // The highest level the device will encode at. A value this table does
        // not know is logged and left out of the check, not guessed at.
        let device_max_level = level::from_std(h264_caps.max_level_idc);
        if device_max_level.is_none() {
            warn!(
                "Device reports H.264 maxLevelIdc {:?}, which is not a level in the table; \
                 the level is not checked against it",
                h264_caps.max_level_idc
            );
        }
        let rgb_caps = config.rgb_input.map(|_| RgbConversionCaps::from(&rgb_caps));

        // The level is resolved from the coded picture, not the requested one:
        // alignment can make a macroblock larger than the request implies.
        let (coded_width, coded_height) = coded_extent(
            config.dimensions.width,
            config.dimensions.height,
            MB_SIZE,
            caps.picture_access_granularity,
            caps.min_coded_extent,
        );
        let frame_mbs = level::frame_macroblocks(coded_width / MB_SIZE, coded_height / MB_SIZE);
        let level_idc = level::resolve(
            config.h264_level_idc,
            frame_mbs,
            config.frame_rate_numerator,
            config.frame_rate_denominator,
            device_max_level,
        )?;
        let declared_level = level::lookup(level_idc).ok_or_else(|| {
            PixelForgeError::InvalidInput(format!("No H.264 table entry for level {level_idc}"))
        })?;
        // The level's decoded picture buffer, in frames at this picture size.
        // The SPS promises `max_dec_frame_buffering = references + 1`, which
        // must fit it, so the reference count the device may use is capped.
        let max_dpb_frames = level::max_dpb_frames(declared_level, frame_mbs);
        if max_dpb_frames < 2 {
            return Err(PixelForgeError::InvalidInput(format!(
                "H.264 level {} holds {max_dpb_frames} frame(s) at {}x{}; one reference needs two",
                level::fmt_level(level_idc),
                config.dimensions.width,
                config.dimensions.height
            )));
        }
        let level_reference_cap = max_dpb_frames - 1;
        if config.max_reference_frames > level_reference_cap {
            warn!(
                "H.264 level {} holds {max_dpb_frames} frames at {}x{}, so at most \
                 {level_reference_cap} reference frame(s) fit; requested {}, using {level_reference_cap}",
                level::fmt_level(level_idc),
                config.dimensions.width,
                config.dimensions.height,
                config.max_reference_frames
            );
        }
        info!(
            "H.264 level {} declared: {frame_mbs} macroblocks per frame at {} macroblocks per second \
             (device maxLevelIdc {}), DPB {max_dpb_frames} frame(s)",
            level::fmt_level(level_idc),
            level::macroblocks_per_second(
                frame_mbs,
                config.frame_rate_numerator,
                config.frame_rate_denominator
            ),
            device_max_level.map_or_else(|| "unknown".to_string(), level::fmt_level)
        );
        let qp_map_plan = crate::encoder::codec::QpMapCaps {
            min_delta: qp_map_codec_caps.min_qp_delta,
            max_delta: qp_map_codec_caps.max_qp_delta,
            max_extent: qp_map_caps.max_quantization_map_extent,
        };
        let intra_refresh_plan = crate::encoder::codec::IntraRefreshCaps {
            modes: intra_refresh_caps.intra_refresh_modes,
            max_cycle_duration: intra_refresh_caps.max_intra_refresh_cycle_duration,
            max_active_reference_pictures: intra_refresh_caps
                .max_intra_refresh_active_reference_pictures,
            partition_independent_refresh_regions: intra_refresh_caps
                .partition_independent_intra_refresh_regions,
            non_rectangular_refresh_regions: intra_refresh_caps
                .non_rectangular_intra_refresh_regions,
        };
        crate::encoder::codec::warn_unsupported_rate_control(
            &config,
            encode_caps.rate_control_modes,
        );

        // Scale the bitstream buffer by resolution so a single frame can't overflow (matches AV1).
        let bitstream_buffer_size = MIN_BITSTREAM_BUFFER_SIZE
            .max(config.dimensions.width as usize * config.dimensions.height as usize);

        let init = build_encoder_common(&CommonInitRequest {
            context: &context,
            config: &config,
            profile_info: &profile_info,
            caps: &caps,
            align_unit: MB_SIZE,
            // Macroblocks are 16x16 in every H.264 profile; nothing to query.
            refresh_block: MB_SIZE,
            max_active_refs_cap: level_reference_cap as usize,
            bitstream_buffer_size,
            allow_layered_dpb: true,
            rgb_caps,
            intra_refresh_caps: intra_refresh_plan,
            qp_map_caps: qp_map_plan,
        })?;
        let active_reference_count = init.active_reference_count;
        let common = init.common;

        // H.264 reference marking bookkeeping.
        let mut dpb = DecodedPictureBuffer::new();
        dpb.h264.sequence_start(DpbConfig {
            dpb_size: common.dpb_slot_count as u32,
            max_num_ref_frames: active_reference_count,
            use_multiple_references: config.b_frame_count > 0,
            max_long_term_refs: 0,
            log2_max_frame_num_minus4: 4,
            log2_max_pic_order_cnt_lsb_minus4: 4,
            num_temporal_layers: 1,
        });

        let codec = H264 {
            dpb,
            frame_num_syntax: 0,
            idr_pic_id: 0,
            has_backward_reference: false,
            backward_reference_frame_num: 0,
            backward_reference_poc: 0,
            backward_reference_dpb_slot: 2,
            l0_references: Vec::new(),
            active_reference_count,
            pending_unmark_frame_nums: Vec::new(),
            profile_idc,
            preferred_entropy_cabac,
            level_idc,
        };

        let mut encoder = CodecEncoder { common, codec };
        let color_desc = config
            .color_description
            .unwrap_or(ColorDescription::bt709());
        encoder.common.session_params = encoder
            .codec
            .build_session_params(&encoder.common, &color_desc)?;

        info!("H.264 encoder created successfully");
        Ok(encoder)
    }

    /// Profile IDC (used when rebuilding parameter sets).
    pub(super) fn profile_idc(&self) -> u32 {
        self.profile_idc
    }

    /// Preferred entropy coding mode (CABAC if true, CAVLC otherwise).
    pub(super) fn preferred_entropy_cabac(&self) -> bool {
        self.preferred_entropy_cabac
    }

    /// Negotiated active reference count.
    pub(super) fn active_reference_count(&self) -> u32 {
        self.active_reference_count
    }
}

/// Query the driver's preferred H.264 entropy coding mode for quality level 0.
/// Defaults to CABAC if the query is unsupported.
fn query_preferred_entropy_cabac(
    context: &VideoContext,
    profile_info: &vk::VideoProfileInfoKHR,
) -> bool {
    let video_encode_instance =
        ash::khr::video_encode_queue::Instance::load(context.entry(), context.instance());
    let mut h264_quality_level_properties = vk::VideoEncodeH264QualityLevelPropertiesKHR::default();
    let mut quality_level_properties = vk::VideoEncodeQualityLevelPropertiesKHR::default()
        .push(&mut h264_quality_level_properties);
    let quality_level_info = vk::PhysicalDeviceVideoEncodeQualityLevelInfoKHR::default()
        .video_profile(profile_info)
        .quality_level(0);
    let result = unsafe {
        (video_encode_instance
            .fp()
            .get_physical_device_video_encode_quality_level_properties_khr)(
            context.physical_device(),
            &quality_level_info,
            &mut quality_level_properties,
        )
    };
    if result == vk::Result::SUCCESS {
        debug!(
            "H.264 quality level 0: preferredStdEntropyCodingModeFlag={}",
            h264_quality_level_properties.preferred_std_entropy_coding_mode_flag
        );
        h264_quality_level_properties.preferred_std_entropy_coding_mode_flag != 0
    } else {
        debug!("Quality level query failed ({result:?}); defaulting to CABAC");
        true
    }
}
