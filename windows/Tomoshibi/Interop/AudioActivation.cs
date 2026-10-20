using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Tomoshibi.Interop;

/// <summary>
/// An audio interface activated by <c>ActivateAudioInterfaceAsync</c> and
/// waited for: the one way to a WASAPI client that a packaged app, a process
/// loopback and the default device that follows the system's choice all
/// share. Shared by the shared sound's loopback (Loopback) and the
/// microphone (WasapiCapture).
///
/// The function wants an agile COM object to call back, and a handler that
/// says it is agile (IAgileObject) is called on whatever thread finishes the
/// activation, which here only sets an event; the handler is made by hand in
/// unmanaged memory, a vtable of four functions and a reference count beside
/// the event it sets.
/// </summary>
internal static unsafe class AudioActivation
{
    private static readonly Guid CompletionHandlerId = new("41d949ab-9862-444a-80f6-c261334da5eb");
    private static readonly Guid AgileId = new("94ea2b94-e9cc-49e0-c0ff-ee64ca8f5b90");
    private static readonly Guid UnknownId = new("00000000-0000-0000-c000-000000000046");

    [StructLayout(LayoutKind.Sequential)]
    public struct Blob
    {
        public ushort Type;
        public ushort Reserved1, Reserved2, Reserved3;
        public uint Size;
        public IntPtr Data;
    }

    [DllImport("mmdevapi.dll", ExactSpelling = true, CharSet = CharSet.Unicode)]
    private static extern int ActivateAudioInterfaceAsync(string path, in Guid riid, Blob* parameters, IntPtr handler, out IntPtr operation);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr CreateEventW(IntPtr attributes, int manual, int initial, IntPtr name);

    [DllImport("kernel32.dll")]
    public static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll")]
    public static extern int CloseHandle(IntPtr handle);

    [DllImport("kernel32.dll")]
    private static extern int SetEvent(IntPtr handle);

    public static IntPtr Slot(IntPtr com, int index) => (*(IntPtr**)com)[index];

    /// <summary>
    /// The interface <paramref name="interfaceId"/> of the audio device at
    /// <paramref name="path"/> (a device interface path, a device class's
    /// GUID for its default, or a virtual device's name), activated and
    /// waited for, with a reference the caller releases. Throws where the
    /// system will not.
    /// </summary>
    public static IntPtr Activate(string path, Guid interfaceId, Blob* parameters = null)
    {
        var handler = Completion.Make();
        try
        {
            Marshal.ThrowExceptionForHR(ActivateAudioInterfaceAsync(path, in interfaceId, parameters, handler, out var operation));
            try
            {
                if (!Completion.Wait(handler, 5000)) throw new TimeoutException("the audio device was not activated");
                int result;
                IntPtr activated;
                var get = (delegate* unmanaged[Stdcall]<IntPtr, int*, IntPtr*, int>)Slot(operation, 3);
                Marshal.ThrowExceptionForHR(get(operation, &result, &activated));
                Marshal.ThrowExceptionForHR(result);
                try
                {
                    Marshal.ThrowExceptionForHR(Marshal.QueryInterface(activated, in interfaceId, out var client));
                    return client;
                }
                finally
                {
                    Marshal.Release(activated);
                }
            }
            finally
            {
                Marshal.Release(operation);
            }
        }
        finally
        {
            Completion.Release(handler);
        }
    }

    private static class Completion
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct Object
        {
            public IntPtr Vtable;
            public int References;
            public IntPtr Done;
        }

        private static readonly IntPtr Vtable = MakeVtable();

        private static IntPtr MakeVtable()
        {
            var table = (IntPtr*)NativeMemory.Alloc((nuint)(4 * sizeof(IntPtr)));
            table[0] = (IntPtr)(delegate* unmanaged[Stdcall]<IntPtr, Guid*, IntPtr*, int>)&QueryInterface;
            table[1] = (IntPtr)(delegate* unmanaged[Stdcall]<IntPtr, uint>)&AddRef;
            table[2] = (IntPtr)(delegate* unmanaged[Stdcall]<IntPtr, uint>)&ReleaseOne;
            table[3] = (IntPtr)(delegate* unmanaged[Stdcall]<IntPtr, IntPtr, int>)&ActivateCompleted;
            return (IntPtr)table;
        }

        public static IntPtr Make()
        {
            var made = (Object*)NativeMemory.Alloc((nuint)sizeof(Object));
            made->Vtable = Vtable;
            made->References = 1;
            made->Done = CreateEventW(IntPtr.Zero, 1, 0, IntPtr.Zero);
            return (IntPtr)made;
        }

        public static bool Wait(IntPtr handler, uint milliseconds) => WaitForSingleObject(((Object*)handler)->Done, milliseconds) == 0;

        public static void Release(IntPtr handler) => Drop(handler);

        // An entry point the system calls may not be called from managed
        // code, so each is a door onto one of these.
        private static uint Add(IntPtr self) => (uint)Interlocked.Increment(ref ((Object*)self)->References);

        private static uint Drop(IntPtr self)
        {
            var left = Interlocked.Decrement(ref ((Object*)self)->References);
            if (left == 0)
            {
                CloseHandle(((Object*)self)->Done);
                NativeMemory.Free((void*)self);
            }
            return (uint)left;
        }

        [UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvStdcall) })]
        private static int QueryInterface(IntPtr self, Guid* id, IntPtr* found)
        {
            if (*id == UnknownId || *id == CompletionHandlerId || *id == AgileId)
            {
                Add(self);
                *found = self;
                return 0;
            }
            *found = IntPtr.Zero;
            return unchecked((int)0x80004002); // E_NOINTERFACE
        }

        [UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvStdcall) })]
        private static uint AddRef(IntPtr self) => Add(self);

        [UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvStdcall) })]
        private static uint ReleaseOne(IntPtr self) => Drop(self);

        [UnmanagedCallersOnly(CallConvs = new[] { typeof(CallConvStdcall) })]
        private static int ActivateCompleted(IntPtr self, IntPtr operation)
        {
            SetEvent(((Object*)self)->Done);
            return 0;
        }
    }
}
