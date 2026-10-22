package livekit.org.webrtc;

import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import java.util.ArrayList;
import java.util.Map;

/**
 * H.264 from whatever encoder the device has, software included: a debug
 * build's, on an emulator, and nowhere else (app.tomoshibi.core.TestEncoders).
 *
 * The prebuilt WebRTC is built without OpenH264 (its args.gn says
 * rtc_use_h264 = false), and HardwareVideoEncoderFactory admits only codecs
 * the platform calls hardware-accelerated, so an emulator, whose only H.264
 * encoder is the platform's software one (c2.android.avc.encoder), published
 * a share and a camera that never reached a browser at all, and nothing about
 * sending pictures could be checked without the owner's phone. This makes the
 * same HardwareVideoEncoder the factory would have made, with the same
 * parameters, around that codec, so what an emulator sends goes the way a
 * phone's does, but for the silicon and one step.
 *
 * That step is the input surface: these encoders take every frame as bytes, a
 * texture frame converted by WebRTC's YuvConverter in the capture's context.
 * The emulator's codec does not read back what GL draws into its input
 * surface: a shared screen arrived black when the encoder sampled the
 * capture's external texture, and as a stripe of misread rows when it drew a
 * copy of it in a plain texture, though screenrecord's own composition into
 * the same codec encodes correctly. So the surface path is checked on a phone,
 * and an emulator checks everything before it with pictures that are right.
 */
final class TestH264EncoderFactory implements VideoEncoderFactory {
    private static final String MIME = "video/avc";
    private final MediaCodecInfo codec = find();

    TestH264EncoderFactory() {}

    private static MediaCodecInfo find() {
        for (MediaCodecInfo info : new MediaCodecList(MediaCodecList.REGULAR_CODECS).getCodecInfos()) {
            if (!info.isEncoder()) continue;
            for (String type : info.getSupportedTypes()) {
                if (!type.equalsIgnoreCase(MIME)) continue;
                if (MediaCodecUtils.selectColorFormat(MediaCodecUtils.ENCODER_COLOR_FORMATS, info.getCapabilitiesForType(MIME)) != null) {
                    Logging.w("TestH264EncoderFactory", "H.264 for tests from " + info.getName());
                    return info;
                }
            }
        }
        return null;
    }

    private static Map<String, String> baseline() {
        return MediaCodecUtils.getCodecProperties(VideoCodecMimeType.H264, false);
    }

    @Override
    public VideoEncoder createEncoder(VideoCodecInfo info) {
        if (codec == null || !info.name.equalsIgnoreCase("H264") || !H264Utils.isSameH264Profile(info.params, baseline())) return null;
        MediaCodecInfo.CodecCapabilities capabilities = codec.getCapabilitiesForType(MIME);
        // As HardwareVideoEncoderFactory.createEncoder makes one for H.264:
        // an hour between key frames, none forced, the plain bitrate adjuster.
        return new HardwareVideoEncoder(new MediaCodecWrapperFactoryImpl(), codec.getName(), VideoCodecMimeType.H264,
            MediaCodecUtils.selectColorFormat(MediaCodecUtils.TEXTURE_COLOR_FORMATS, capabilities),
            MediaCodecUtils.selectColorFormat(MediaCodecUtils.ENCODER_COLOR_FORMATS, capabilities),
            info.params, 3600, 0, new BaseBitrateAdjuster(), null);
    }

    @Override
    public VideoCodecInfo[] getSupportedCodecs() {
        if (codec == null) return new VideoCodecInfo[0];
        return new VideoCodecInfo[] {new VideoCodecInfo("H264", baseline(), new ArrayList<>())};
    }
}
