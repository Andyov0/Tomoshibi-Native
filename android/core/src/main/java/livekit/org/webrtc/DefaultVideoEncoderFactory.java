package livekit.org.webrtc;

import app.tomoshibi.core.SharedEgl;
import app.tomoshibi.core.TestEncoders;

/**
 * The encoder factory WebRTC's native half makes for every picture this app
 * sends, in place of the one libwebrtc.jar carries (android/build-core.sh
 * takes that class out of the jar, so that this one is found by its name).
 *
 * webrtc-sys makes it through JNI by this exact name and constructor
 * (webrtc-sys src/android.cpp, CreateAndroidVideoEncoderFactory) and always
 * passes no EGL context. Without one, HardwareVideoEncoderFactory makes every
 * encoder take its pictures as bytes: a shared screen was read back from the
 * GPU, copied four times on its way into the core, scaled on the processor
 * for each rung beneath the top, and copied again into each of three
 * MediaCodec input buffers, sixty times a second, which is what made a Fold
 * hot while it shared its inner screen (owner, 2026-10-22). Given the
 * process's shared context (SharedEgl), the hardware encoders take a texture
 * through an input surface instead, and the screen's frames reach them as
 * textures the GPU scales and the codec converts, nothing of the picture
 * passing through the processor at all. A frame in memory, a camera's, is
 * still taken as bytes by the same encoder: HardwareVideoEncoder chooses per
 * frame.
 *
 * Everything else is the original's: the hardware factory first, the
 * software one beside it with a fallback between them for a codec both
 * have, and the union of both as what may be negotiated. The pictures that
 * leave are unchanged, the same codec, profile and ladder; only the way the
 * pixels reach the encoder differs.
 */
public class DefaultVideoEncoderFactory implements VideoEncoderFactory {
    private final VideoEncoderFactory hardwareVideoEncoderFactory;
    private final VideoEncoderFactory softwareVideoEncoderFactory = new SoftwareVideoEncoderFactory();

    public DefaultVideoEncoderFactory(EglBase.Context eglContext, boolean enableIntelVp8Encoder, boolean enableH264HighProfile) {
        EglBase.Context shared = eglContext != null ? eglContext : SharedEgl.context();
        VideoEncoderFactory hardware = new HardwareVideoEncoderFactory(shared, enableIntelVp8Encoder, enableH264HighProfile);
        if (TestEncoders.softwareH264 && !offers(hardware, "H264")) {
            hardware = new TestH264EncoderFactory();
        }
        hardwareVideoEncoderFactory = hardware;
    }

    DefaultVideoEncoderFactory(VideoEncoderFactory hardwareVideoEncoderFactory) {
        this.hardwareVideoEncoderFactory = hardwareVideoEncoderFactory;
    }

    @Override
    public VideoEncoder createEncoder(VideoCodecInfo info) {
        VideoEncoder software = softwareVideoEncoderFactory.createEncoder(info);
        VideoEncoder hardware = hardwareVideoEncoderFactory.createEncoder(info);
        if (hardware instanceof HardwareVideoEncoder) hardware = new FittedVideoEncoder(hardware);
        if (hardware != null && software != null) return new VideoEncoderFallback(software, hardware);
        return hardware != null ? hardware : software;
    }

    @Override
    public VideoCodecInfo[] getSupportedCodecs() {
        java.util.LinkedHashSet<VideoCodecInfo> codecs = new java.util.LinkedHashSet<>();
        codecs.addAll(java.util.Arrays.asList(softwareVideoEncoderFactory.getSupportedCodecs()));
        codecs.addAll(java.util.Arrays.asList(hardwareVideoEncoderFactory.getSupportedCodecs()));
        return codecs.toArray(new VideoCodecInfo[0]);
    }

    private static boolean offers(VideoEncoderFactory factory, String name) {
        for (VideoCodecInfo codec : factory.getSupportedCodecs()) {
            if (codec.name.equalsIgnoreCase(name)) return true;
        }
        return false;
    }
}
