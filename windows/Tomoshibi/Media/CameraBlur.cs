using Tomoshibi.Basics;
using Tomoshibi.Interop;
using Windows.Graphics.Imaging;

namespace Tomoshibi.Media;

/// <summary>The camera's frames with the background blurred (BackgroundBlur), apart from the rest of the camera.</summary>
internal sealed partial class CameraFeed
{
    /// <summary>The background blur the frames go through before they are sent and shown, or none.</summary>
    public volatile BackgroundBlur? Blur;

    private byte[] _y = [], _u = [], _v = [], _preview = [];

    /// <summary>
    /// A frame with its background blurred: the NV12 frame taken apart into
    /// packed planes (the blur's I420), blurred in place, sent, and the
    /// picture of yourself drawn from what was sent rather than from the
    /// camera, so that what a person sees of themselves is what the room sees.
    /// </summary>
    private unsafe void Blurred(SoftwareBitmap bitmap, BackgroundBlur blur, long micros)
    {
        using var nv12 = bitmap.BitmapPixelFormat == BitmapPixelFormat.Nv12 ? null : SoftwareBitmap.Convert(bitmap, BitmapPixelFormat.Nv12);
        var picture = nv12 ?? bitmap;
        var width = picture.PixelWidth / 2 * 2;
        var height = picture.PixelHeight / 2 * 2;
        var cw = width / 2;
        var ch = height / 2;
        if (_y.Length != width * height)
        {
            _y = new byte[width * height];
            _u = new byte[cw * ch];
            _v = new byte[cw * ch];
            _preview = new byte[width * height * 4];
        }
        using (var buffer = picture.LockBuffer(BitmapBufferAccessMode.Read))
        {
            var luma = buffer.GetPlaneDescription(0);
            var chroma = buffer.GetPlaneDescription(1);
            using var reference = buffer.CreateReference();
            var (data, _) = ComCalls.Bytes(reference);
            var bytes = (byte*)data;
            for (var row = 0; row < height; row++)
            {
                new ReadOnlySpan<byte>(bytes + luma.StartIndex + row * luma.Stride, width).CopyTo(_y.AsSpan(row * width, width));
            }
            // NV12's chroma is U and V side by side; the blur's planes are apart.
            for (var row = 0; row < ch; row++)
            {
                var line = bytes + chroma.StartIndex + row * chroma.Stride;
                var at = row * cw;
                for (var column = 0; column < cw; column++)
                {
                    _u[at + column] = line[2 * column];
                    _v[at + column] = line[2 * column + 1];
                }
            }
        }
        blur.Process(_y, _u, _v, width, height);
        if (Input is { } input)
        {
            input.PushPlanes(_y, _u, _v, (uint)width, (uint)cw, 1, (uint)width, (uint)height, micros, 0);
            Interlocked.Increment(ref FramesPushed);
        }
        if (Previewed is { } previewed && Interlocked.Increment(ref _arrived) % 3 == 0)
        {
            Planes.ToBgra(_y, _u, _v, width, height, 1, _preview);
            previewed(_preview, width, height);
        }
    }
}
