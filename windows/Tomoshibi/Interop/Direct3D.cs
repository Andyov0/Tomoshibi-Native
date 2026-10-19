using System.Runtime.InteropServices;
using Windows.Graphics.DirectX.Direct3D11;
using WinRT;

namespace Tomoshibi.Interop;

/// <summary>
/// The few Direct3D 11 calls a screen capture needs: a device for the
/// capture's frame pool, and each frame copied into a texture the processor
/// can read and read back. Called through their vtables, as ComCalls calls
/// the panel's: the runtime's built-in COM casts break under trimming, and a
/// whole Direct3D wrapper for six calls would be most of a library unused.
///
/// Vtable slots are the d3d11.h order: ID3D11Device CreateTexture2D 5,
/// GetImmediateContext 40; ID3D11DeviceContext Map 14, Unmap 15,
/// CopyResource 47; ID3D11Texture2D GetDesc 10.
/// </summary>
internal sealed unsafe class Direct3D : IDisposable
{
    private static readonly Guid DxgiDeviceId = new("54ec77fa-1377-44e6-8c32-88fd5f44c84c");
    private static readonly Guid Texture2DId = new("6f15aaf2-d208-4e89-9ab4-489535d34f9c");
    private static readonly Guid DxgiInterfaceAccessId = new("a9b3d012-3df2-4ee3-b8d1-8695f457d3c1");
    private static readonly Guid MultithreadId = new("9b7e4e00-342c-4106-a19f-4f2704f689f0");

    private const uint BgraSupport = 0x20;
    private const int HardwareDriver = 1;
    private const int WarpDriver = 5;
    private const uint SdkVersion = 7;

    [StructLayout(LayoutKind.Sequential)]
    public struct TextureDesc
    {
        public uint Width, Height, MipLevels, ArraySize, Format, SampleCount, SampleQuality, Usage, BindFlags, CpuAccessFlags, MiscFlags;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Mapped
    {
        public IntPtr Data;
        public uint RowPitch, DepthPitch;
    }

    [DllImport("d3d11.dll", ExactSpelling = true)]
    private static extern int D3D11CreateDevice(IntPtr adapter, int driverType, IntPtr software, uint flags, IntPtr featureLevels, uint levels, uint sdkVersion, out IntPtr device, out int featureLevel, out IntPtr context);

    [DllImport("d3d11.dll", ExactSpelling = true)]
    private static extern int CreateDirect3D11DeviceFromDXGIDevice(IntPtr dxgiDevice, out IntPtr inspectable);

    private IntPtr _device;
    private IntPtr _context;
    private IntPtr _staging;
    private TextureDesc _stagingDesc;
    private readonly object _gate = new();

    /// <summary>The device as WinRT sees it, for Direct3D11CaptureFramePool.</summary>
    public IDirect3DDevice Device { get; }

    private Direct3D(IntPtr device, IntPtr context, IDirect3DDevice winrt)
    {
        _device = device;
        _context = context;
        Device = winrt;
    }

    /// <summary>A device on the hardware, or on the processor where there is none (a virtual machine without a display driver).</summary>
    public static Direct3D? Make()
    {
        foreach (var driver in new[] { HardwareDriver, WarpDriver })
        {
            if (D3D11CreateDevice(IntPtr.Zero, driver, IntPtr.Zero, BgraSupport, IntPtr.Zero, 0, SdkVersion, out var device, out _, out var context) < 0) continue;
            IntPtr dxgi = IntPtr.Zero, inspectable = IntPtr.Zero;
            try
            {
                // The capture's frame pool works on the device from threads
                // of its own while frames are copied on another; the
                // immediate context is guarded for both.
                if (Marshal.QueryInterface(context, in MultithreadId, out var multithread) >= 0)
                {
                    var protect = (delegate* unmanaged[Stdcall]<IntPtr, int, int>)Slot(multithread, 5);
                    protect(multithread, 1);
                    Marshal.Release(multithread);
                }
                if (Marshal.QueryInterface(device, in DxgiDeviceId, out dxgi) < 0) throw new InvalidOperationException("no DXGI device");
                Marshal.ThrowExceptionForHR(CreateDirect3D11DeviceFromDXGIDevice(dxgi, out inspectable));
                var winrt = MarshalInterface<IDirect3DDevice>.FromAbi(inspectable);
                return new Direct3D(device, context, winrt);
            }
            catch (Exception)
            {
                Marshal.Release(context);
                Marshal.Release(device);
            }
            finally
            {
                if (dxgi != IntPtr.Zero) Marshal.Release(dxgi);
                if (inspectable != IntPtr.Zero) Marshal.Release(inspectable);
            }
        }
        return null;
    }

    private static IntPtr Slot(IntPtr com, int index) => (*(IntPtr**)com)[index];

    /// <summary>The ID3D11Texture2D behind a capture frame's surface, with a reference the caller releases.</summary>
    public static IntPtr TextureOf(IDirect3DSurface surface)
    {
        var unknown = ((IWinRTObject)surface).NativeObject.ThisPtr;
        Marshal.ThrowExceptionForHR(Marshal.QueryInterface(unknown, in DxgiInterfaceAccessId, out var access));
        try
        {
            IntPtr texture;
            var id = Texture2DId;
            var get = (delegate* unmanaged[Stdcall]<IntPtr, Guid*, IntPtr*, int>)Slot(access, 3);
            Marshal.ThrowExceptionForHR(get(access, &id, &texture));
            return texture;
        }
        finally
        {
            Marshal.Release(access);
        }
    }

    public static TextureDesc DescOf(IntPtr texture)
    {
        TextureDesc desc;
        var get = (delegate* unmanaged[Stdcall]<IntPtr, TextureDesc*, void>)Slot(texture, 10);
        get(texture, &desc);
        return desc;
    }

    /// <summary>
    /// Copies a frame's texture into the staging texture kept for its size,
    /// on the GPU: cheap enough for every frame the capture hands over,
    /// however many more there are than the share sends, so that the last
    /// change before the screen goes still is the one read back.
    /// </summary>
    public bool Stage(IntPtr texture)
    {
        lock (_gate)
        {
            if (_device == IntPtr.Zero) return false;
            var desc = DescOf(texture);
            if (_staging == IntPtr.Zero || _stagingDesc.Width != desc.Width || _stagingDesc.Height != desc.Height || _stagingDesc.Format != desc.Format)
            {
                if (_staging != IntPtr.Zero) Marshal.Release(_staging);
                _staging = IntPtr.Zero;
                var staging = desc;
                staging.MipLevels = 1;
                staging.ArraySize = 1;
                staging.SampleCount = 1;
                staging.SampleQuality = 0;
                staging.Usage = 3; // D3D11_USAGE_STAGING
                staging.BindFlags = 0;
                staging.CpuAccessFlags = 0x20000; // D3D11_CPU_ACCESS_READ
                staging.MiscFlags = 0;
                IntPtr made;
                var create = (delegate* unmanaged[Stdcall]<IntPtr, TextureDesc*, IntPtr, IntPtr*, int>)Slot(_device, 5);
                if (create(_device, &staging, IntPtr.Zero, &made) < 0) return false;
                _staging = made;
                _stagingDesc = staging;
            }
            var copy = (delegate* unmanaged[Stdcall]<IntPtr, IntPtr, IntPtr, void>)Slot(_context, 47);
            copy(_context, _staging, texture);
            return true;
        }
    }

    /// <summary>
    /// The staged picture into <paramref name="into"/>, rows of
    /// <c>width * 4</c> bytes one under another: what the core's
    /// <c>push_bgra</c> takes. Returns the width and height, or nothing where
    /// nothing is staged or it could not be read.
    /// </summary>
    public (int Width, int Height)? Read(ref byte[] into)
    {
        lock (_gate)
        {
            if (_device == IntPtr.Zero || _staging == IntPtr.Zero) return null;
            var desc = _stagingDesc;
            Mapped mapped;
            var map = (delegate* unmanaged[Stdcall]<IntPtr, IntPtr, uint, int, uint, Mapped*, int>)Slot(_context, 14);
            if (map(_context, _staging, 0, 1 /* D3D11_MAP_READ */, 0, &mapped) < 0) return null;
            try
            {
                var (width, height) = ((int)desc.Width, (int)desc.Height);
                var row = width * 4;
                if (into.Length < row * height) into = new byte[row * height];
                fixed (byte* target = into)
                {
                    for (var y = 0; y < height; y++)
                    {
                        Buffer.MemoryCopy((byte*)mapped.Data + (long)y * mapped.RowPitch, target + (long)y * row, row, row);
                    }
                }
                return (width, height);
            }
            finally
            {
                var unmap = (delegate* unmanaged[Stdcall]<IntPtr, IntPtr, uint, void>)Slot(_context, 15);
                unmap(_context, _staging, 0);
            }
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            if (_staging != IntPtr.Zero) Marshal.Release(_staging);
            if (_context != IntPtr.Zero) Marshal.Release(_context);
            if (_device != IntPtr.Zero) Marshal.Release(_device);
            _staging = _context = _device = IntPtr.Zero;
        }
    }
}
