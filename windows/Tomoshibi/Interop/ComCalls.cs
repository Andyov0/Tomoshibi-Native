using System.Runtime.InteropServices;
using WinRT;

namespace Tomoshibi.Interop;

/// <summary>
/// The two classic COM interfaces the app reaches behind WinRT objects,
/// called through their vtables rather than through [ComImport] casts, which
/// need the runtime's built-in COM and break under trimming.
/// </summary>
internal static unsafe class ComCalls
{
    /// <summary>ISwapChainPanelNative (microsoft.ui.xaml.media.dxinterop.h).</summary>
    private static readonly Guid SwapChainPanelNative = new("63aad0b8-7c24-40ff-85a8-640d944cc325");

    /// <summary>IMemoryBufferByteAccess (memorybuffer.h).</summary>
    private static readonly Guid MemoryBufferByteAccess = new("5b0d3235-4dba-4d44-865e-8f1d0e4fd04d");

    private static IntPtr Query(object winrt, Guid iid)
    {
        var unknown = ((IWinRTObject)winrt).NativeObject.ThisPtr;
        Marshal.ThrowExceptionForHR(Marshal.QueryInterface(unknown, in iid, out var native));
        return native;
    }

    /// <summary>
    /// Shows <paramref name="swapChain"/> (an IDXGISwapChain1, or zero to
    /// clear it) in a SwapChainPanel. UI thread only. The panel takes its own
    /// reference; the caller keeps whatever reference it had.
    /// </summary>
    public static void SetSwapChain(Microsoft.UI.Xaml.Controls.SwapChainPanel panel, IntPtr swapChain)
    {
        var native = Query(panel, SwapChainPanelNative);
        try
        {
            var set = (delegate* unmanaged[Stdcall]<IntPtr, IntPtr, int>)(*(IntPtr**)native)[3];
            Marshal.ThrowExceptionForHR(set(native, swapChain));
        }
        finally
        {
            Marshal.Release(native);
        }
    }

    /// <summary>The bytes behind an IMemoryBufferReference, valid while it is open.</summary>
    public static (IntPtr Data, uint Capacity) Bytes(Windows.Foundation.IMemoryBufferReference reference)
    {
        var native = Query(reference, MemoryBufferByteAccess);
        try
        {
            byte* data;
            uint capacity;
            var get = (delegate* unmanaged[Stdcall]<IntPtr, byte**, uint*, int>)(*(IntPtr**)native)[3];
            Marshal.ThrowExceptionForHR(get(native, &data, &capacity));
            return ((IntPtr)data, capacity);
        }
        finally
        {
            Marshal.Release(native);
        }
    }
}
