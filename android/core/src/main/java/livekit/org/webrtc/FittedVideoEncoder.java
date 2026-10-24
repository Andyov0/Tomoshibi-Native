package livekit.org.webrtc;

/**
 * A hardware encoder handed each frame at the size it was set up for.
 *
 * Simulcast is WebRTC's SimulcastEncoderAdapter, one encoder for each rung.
 * It scales a frame in memory down to each rung itself, but passes a texture
 * frame to every rung as it is, at the top layer's size, on the understanding
 * that an encoder taking textures scales them (simulcast_encoder_adapter.cc:
 * "the underlying encoder is expected to be able to correctly sample/scale the
 * source texture"). HardwareVideoEncoder does not: a frame of another size
 * makes it set its codec up again at that size, so the 720p and 1080p rungs of
 * a shared screen would have been encoded at the full 1440p, three full-size
 * encodes and a ladder no browser publishes. The Android SDK of LiveKit wraps
 * its encoders for the same reason (SimulcastVideoEncoderFactoryWrapper,
 * StreamEncoderWrapper).
 *
 * So a frame of another size is cropped and scaled to this encoder's before it
 * is encoded. For a texture that is a change to its transform and nothing
 * else: the GPU scales it as it draws it into the codec's surface. A frame in
 * memory already arrives at its rung's size and is passed on untouched.
 */
final class FittedVideoEncoder implements VideoEncoder {
    private final VideoEncoder encoder;
    private int width;
    private int height;

    FittedVideoEncoder(VideoEncoder encoder) {
        this.encoder = encoder;
    }

    @Override
    public long createNative(long webrtcEnvRef) {
        return encoder.createNative(webrtcEnvRef);
    }

    @Override
    public boolean isHardwareEncoder() {
        return encoder.isHardwareEncoder();
    }

    @Override
    public VideoCodecStatus initEncode(Settings settings, Callback callback) {
        width = settings.width;
        height = settings.height;
        return encoder.initEncode(settings, callback);
    }

    @Override
    public VideoCodecStatus release() {
        return encoder.release();
    }

    @Override
    public VideoCodecStatus encode(VideoFrame frame, EncodeInfo info) {
        VideoFrame.Buffer buffer = frame.getBuffer();
        if (width <= 0 || height <= 0 || (buffer.getWidth() == width && buffer.getHeight() == height)) {
            return encoder.encode(frame, info);
        }
        VideoFrame.Buffer fitted = buffer.cropAndScale(0, 0, buffer.getWidth(), buffer.getHeight(), width, height);
        try {
            return encoder.encode(new VideoFrame(fitted, frame.getRotation(), frame.getTimestampNs()), info);
        } finally {
            fitted.release();
        }
    }

    @Override
    public VideoCodecStatus setRateAllocation(BitrateAllocation allocation, int framerate) {
        return encoder.setRateAllocation(allocation, framerate);
    }

    @Override
    public VideoCodecStatus setRates(RateControlParameters parameters) {
        return encoder.setRates(parameters);
    }

    @Override
    public ScalingSettings getScalingSettings() {
        return encoder.getScalingSettings();
    }

    @Override
    public ResolutionBitrateLimits[] getResolutionBitrateLimits() {
        return encoder.getResolutionBitrateLimits();
    }

    @Override
    public String getImplementationName() {
        return encoder.getImplementationName();
    }

    @Override
    public EncoderInfo getEncoderInfo() {
        return encoder.getEncoderInfo();
    }
}
