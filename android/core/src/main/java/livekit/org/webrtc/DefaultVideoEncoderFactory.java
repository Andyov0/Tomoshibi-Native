package livekit.org.webrtc;

import app.tomoshibi.core.TestEncoders;

/**
 * The encoder factory WebRTC's native half makes for every picture this app
 * sends, in place of the one libwebrtc.jar carries (android/build-core.sh
 * takes that class out of the jar, so that this one is found by its name).
 *
 * webrtc-sys makes it through JNI by this exact name and constructor
 * (webrtc-sys src/android.cpp, CreateAndroidVideoEncoderFactory), so this is
 * the one place an Android build can say which encoders a picture gets.
 * Everything here is the original's: the hardware factory first, the
 * software one beside it with a fallback between them for a codec both have,
 * and the union of both as what may be negotiated; except that a debug build
 * on an emulator, which has no hardware H.264 at all, is given the
 * platform's software codec through the same hardware encoder
 * (TestH264EncoderFactory).
 */
public class DefaultVideoEncoderFactory implements VideoEncoderFactory {
    private final VideoEncoderFactory hardwareVideoEncoderFactory;
    private final VideoEncoderFactory softwareVideoEncoderFactory = new SoftwareVideoEncoderFactory();

    public DefaultVideoEncoderFactory(EglBase.Context eglContext, boolean enableIntelVp8Encoder, boolean enableH264HighProfile) {
        VideoEncoderFactory hardware = new HardwareVideoEncoderFactory(eglContext, enableIntelVp8Encoder, enableH264HighProfile);
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
