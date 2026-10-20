using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * Which device a call opens and where it goes when one is unplugged
 * (livekit-client's handleTrackEnded and selectDefaultDevices, which a
 * browser in the same room follows). What these guard: a call that stops
 * its microphone when the headset is pulled though the laptop's own is
 * there; a camera that goes black when the webcam in use is unplugged
 * though another is plugged in; a call that keeps sending from a device
 * that is gone; and the choice somebody made being forgotten by a moment's
 * unplugging.
 */
public class DeviceRulesTests
{
    private static readonly string[] Two = ["usb", "builtin"];

    [Fact]
    public void OpensTheOneAskedForWhileItIsThere()
    {
        Assert.Equal("usb", DeviceRules.Opened("usb", Two, systemHasOne: true));
        Assert.Equal("usb", DeviceRules.Opened("usb", Two, systemHasOne: false));
    }

    [Fact]
    public void AMicrophoneAskedForThatIsGoneOpensTheSystemsOwn()
    {
        Assert.Null(DeviceRules.Opened("headset", Two, systemHasOne: true));
        Assert.Null(DeviceRules.Opened(null, Two, systemHasOne: true));
    }

    [Fact]
    public void ACameraAskedForThatIsGoneOpensTheFirstListed()
    {
        // Windows names no default camera: the browser's getUserMedia
        // without a device takes the first.
        Assert.Equal("usb", DeviceRules.Opened("gone", Two, systemHasOne: false));
        Assert.Equal("usb", DeviceRules.Opened(null, Two, systemHasOne: false));
        Assert.Null(DeviceRules.Opened(null, [], systemHasOne: false));
    }

    [Fact]
    public void ADeviceStillListedStays()
    {
        Assert.Equal((DeviceRules.Move.Stay, "usb"), DeviceRules.Follow("usb", Two, systemHasOne: true));
        Assert.Equal((DeviceRules.Move.Stay, "builtin"), DeviceRules.Follow("builtin", Two, systemHasOne: false));
    }

    [Fact]
    public void AMicrophoneUnpluggedMovesToTheSystemsOwnOnTheSameTrack()
    {
        Assert.Equal((DeviceRules.Move.Switch, (string?)null), DeviceRules.Follow("headset", Two, systemHasOne: true));
    }

    [Fact]
    public void ACameraUnpluggedMovesToTheFirstStillThere()
    {
        Assert.Equal((DeviceRules.Move.Switch, "builtin"), DeviceRules.Follow("usb", ["builtin"], systemHasOne: false));
    }

    [Fact]
    public void TheLastOneUnpluggedStopsIt()
    {
        Assert.Equal((DeviceRules.Move.Stop, (string?)null), DeviceRules.Follow("usb", [], systemHasOne: false));
        Assert.Equal((DeviceRules.Move.Stop, (string?)null), DeviceRules.Follow("usb", [], systemHasOne: true));
        // The system's own with nothing left is nothing, too.
        Assert.Equal((DeviceRules.Move.Stop, (string?)null), DeviceRules.Follow(null, [], systemHasOne: true));
    }

    [Fact]
    public void TheSystemsOwnFollowsTheSystemWhileAnythingIsThere()
    {
        Assert.Equal((DeviceRules.Move.Stay, (string?)null), DeviceRules.Follow(null, ["builtin"], systemHasOne: true));
    }

    [Fact]
    public void TwoOfTheSameNameAreNumbered()
    {
        var named = DeviceRules.Numbered([new("a", "USB Camera"), new("b", "USB Camera"), new("c", " Microphone "), new("d", "")]);
        Assert.Equal(["USB Camera", "USB Camera 2", "Microphone", "Device 4"], named.Select(d => d.Name));
        Assert.Equal(["a", "b", "c", "d"], named.Select(d => d.Id));
    }

    [Fact]
    public void TheEchoCancellerIsToldTheEndpointNotThePath()
    {
        Assert.Equal(
            "{0.0.0.00000000}.{b5d1f640-c2fa-4ca7-ab86-37e60be8bf0d}",
            DeviceRules.EndpointId(@"\\?\SWD#MMDEVAPI#{0.0.0.00000000}.{b5d1f640-c2fa-4ca7-ab86-37e60be8bf0d}#{e6327cad-dcec-4949-ae8a-991e976a79d2}"));
        Assert.Equal("{0.0.1.00000000}.{x}", DeviceRules.EndpointId("{0.0.1.00000000}.{x}"));
        Assert.Null(DeviceRules.EndpointId(@"\\?\USB#VID_203A&PID_FFF9#{e5323777-f976-4f5b-9b55-b94699c46e44}"));
        Assert.Null(DeviceRules.EndpointId(null));
    }
}

/*
 * The format a camera is opened in, and the size sent (share.rs
 * camera_capture: 1280x720 at thirty from a computer). What these guard: a
 * call sent at ten frames because the camera's 720p came only in YUY2 over
 * USB 2; a 4:3 picture sent where a 16:9 one was offered; a 4K camera sent
 * at 4K; a small camera blown up to 720p; and an MJPG decoder run where the
 * same picture came uncompressed.
 */
public class CameraFormatsTests
{
    private static CameraFormats.Offer O(uint w, uint h, double rate, string subtype = "NV12") => new(w, h, rate, subtype);

    [Fact]
    public void TakesTheSizeAskedForAtTheRateAskedFor()
    {
        var offers = new[] { O(640, 360, 30), O(1280, 720, 30), O(1920, 1080, 30) };
        Assert.Equal(1, CameraFormats.Choose(offers, 1280, 720, 30));
    }

    [Fact]
    public void PrefersALargerPictureAtFullRateToTheRightSizeAtTen()
    {
        // A typical USB 2 webcam: 720p uncompressed only at ten frames.
        var offers = new[] { O(1280, 720, 10, "YUY2"), O(1920, 1080, 30, "MJPG"), O(640, 360, 30, "YUY2") };
        Assert.Equal(1, CameraFormats.Choose(offers, 1280, 720, 30));
    }

    [Fact]
    public void AtTheSameSizeAndRateTakesTheUncompressedOne()
    {
        var offers = new[] { O(1280, 720, 30, "MJPG"), O(1280, 720, 30, "YUY2"), O(1280, 720, 30, "NV12") };
        Assert.Equal(2, CameraFormats.Choose(offers, 1280, 720, 30));
        Assert.Equal(1, CameraFormats.Choose(offers.Take(2).ToList(), 1280, 720, 30));
    }

    [Fact]
    public void PrefersWideToFourByThree()
    {
        var offers = new[] { O(1280, 960, 30), O(1920, 1080, 30) };
        Assert.Equal(1, CameraFormats.Choose(offers, 1280, 720, 30));
    }

    [Fact]
    public void TakesFourByThreeWhenThatIsAllThereIs()
    {
        var offers = new[] { O(640, 480, 30), O(1280, 960, 30), O(2592, 1944, 15) };
        Assert.Equal(1, CameraFormats.Choose(offers, 1280, 720, 30));
    }

    [Fact]
    public void TakesTheLargestBelowWhenNothingIsLargeEnough()
    {
        var offers = new[] { O(320, 180, 30), O(640, 360, 30) };
        Assert.Equal(1, CameraFormats.Choose(offers, 1280, 720, 30));
    }

    [Fact]
    public void TakesTheLowestRateThatReachesTheOneAskedFor()
    {
        // 29.97 is thirty; sixty would be paced down anyway, at twice the work.
        var offers = new[] { O(1280, 720, 60), O(1280, 720, 29.97), O(1280, 720, 15) };
        Assert.Equal(1, CameraFormats.Choose(offers, 1280, 720, 30));
    }

    [Fact]
    public void TakesTheFastestWhenNoneReachesIt()
    {
        var offers = new[] { O(1280, 720, 10), O(1280, 720, 15) };
        Assert.Equal(1, CameraFormats.Choose(offers, 1280, 720, 30));
    }

    [Fact]
    public void NothingWithAPictureIsNothing()
    {
        Assert.Equal(-1, CameraFormats.Choose([], 1280, 720, 30));
        Assert.Equal(-1, CameraFormats.Choose([O(0, 0, 30)], 1280, 720, 30));
    }

    [Fact]
    public void SendsALargerPictureScaledDownToTheSizeAskedFor()
    {
        Assert.Equal((1280u, 720u), CameraFormats.Sent(1920, 1080, 1280, 720));
        Assert.Equal((1280u, 720u), CameraFormats.Sent(3840, 2160, 1280, 720));
        // Its shape kept: a 4:3 picture fits within the height.
        Assert.Equal((960u, 720u), CameraFormats.Sent(1280, 960, 1280, 720));
    }

    [Fact]
    public void NeverScalesUpAndKeepsEvenSides()
    {
        Assert.Equal((640u, 360u), CameraFormats.Sent(640, 360, 1280, 720));
        Assert.Equal((1280u, 720u), CameraFormats.Sent(1280, 720, 1280, 720));
        var (w, h) = CameraFormats.Sent(1366, 769, 1280, 720);
        Assert.True(w % 2 == 0 && h % 2 == 0 && w <= 1280 && h <= 720, $"{w}x{h}");
    }
}

/*
 * Android's Analyzer pacing, case for case: a camera at thirty sends thirty
 * to a call at thirty, sixty is halved, and frames arriving with jitter are
 * not lost to it.
 */
public class FramePacerTests
{
    private static int Sent(uint rate, double cameraRate, double seconds, double jitterMicros = 0)
    {
        var pacer = new FramePacer(rate);
        var random = new Random(7);
        var sent = 0;
        var step = 1_000_000.0 / cameraRate;
        for (var i = 0; i < cameraRate * seconds; i++)
        {
            var at = (long)(i * step + (jitterMicros > 0 ? (random.NextDouble() - 0.5) * jitterMicros : 0));
            if (pacer.Take(at)) sent++;
        }
        return sent;
    }

    [Fact]
    public void ACameraAtTheRateSendsEveryFrame() => Assert.InRange(Sent(30, 30, 10), 299, 301);

    [Fact]
    public void ACameraAtTwiceTheRateSendsHalf() => Assert.InRange(Sent(30, 60, 10), 299, 302);

    [Fact]
    public void ACameraAtThirtyToTwentySendsTwenty() => Assert.InRange(Sent(20, 30, 10), 198, 202);

    [Fact]
    public void JitterLosesNothingAtTheRate() => Assert.InRange(Sent(30, 30, 10, jitterMicros: 4000), 297, 301);

    [Fact]
    public void ASlowCameraSendsAllItHas() => Assert.InRange(Sent(30, 15, 10), 149, 151);
}

/*
 * The voice's capture mode (Android's CallAudio openRecord), its samples as
 * the core takes them, and the meter's curve (MicLevel.tsx).
 */
public class VoiceCaptureTests
{
    [Fact]
    public void ACallsVoiceIsProcessed() => Assert.Equal(VoiceCapture.Mode.Communications, VoiceCapture.For(original: false, rawOffered: true));

    [Fact]
    public void OriginalSoundIsRawWhereOffered() => Assert.Equal(VoiceCapture.Mode.Raw, VoiceCapture.For(original: true, rawOffered: true));

    [Fact]
    public void OriginalSoundIsPlainWhereRawIsNot() => Assert.Equal(VoiceCapture.Mode.Plain, VoiceCapture.For(original: true, rawOffered: false));

    [Fact]
    public void SamplesAreClippedNotWrapped()
    {
        var pcm = new byte[10];
        VoiceCapture.ToPcm16([0f, 0.5f, 1.5f, -1.5f, float.NaN], pcm);
        short At(int i) => (short)(pcm[2 * i] | pcm[2 * i + 1] << 8);
        Assert.Equal(0, At(0));
        Assert.Equal(16383, At(1));
        Assert.Equal(short.MaxValue, At(2));
        Assert.Equal(short.MinValue, At(3));
        Assert.Equal(0, At(4));
    }

    [Fact]
    public void TheMeterIsDrawnInDecibels()
    {
        Assert.Equal(0, VoiceCapture.Shown(0));
        Assert.Equal(0, VoiceCapture.Shown(0.0005));
        // -26 dBFS speech sits above the middle; a quiet room's -55 under the line.
        Assert.InRange(VoiceCapture.Shown(0.05), 0.6, 0.75);
        Assert.True(VoiceCapture.Shown(Math.Pow(10, -55 / 20.0)) > 0);
        Assert.True(VoiceCapture.Shown(Math.Pow(10, -58 / 20.0)) < VoiceCapture.Heard);
        Assert.Equal(1, VoiceCapture.Shown(1));
    }
}
