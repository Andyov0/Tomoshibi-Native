using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Tomoshibi.Interop;

/// <summary>
/// What the computer plays, captured before it reaches the speakers: WASAPI's
/// process loopback (Windows 10 2004 and later), which can take every
/// process's sound but this one's, or one process and its children alone.
///
/// The sound a call shares has to leave out the call itself: an ordinary
/// loopback of the speakers would send everybody's own voices back to them
/// a beat late. Excluding this process tree is what makes a screen shared
/// with its sound bearable; including one tree is how somebody shares a
/// single app's sound.
///
/// Called through vtables, as ComCalls is, with a completion handler made by
/// hand: <c>ActivateAudioInterfaceAsync</c> wants an agile COM object to
/// call back, and a handler that says it is agile (IAgileObject) is called
/// on whatever thread finishes the activation, which here only sets an
/// event. Slots are the audioclient.h order: IAudioClient Initialize 3,
/// GetBufferSize 4, Start 10, Stop 11, SetEventHandle 13, GetService 14;
/// IAudioCaptureClient GetBuffer 3, ReleaseBuffer 4, GetNextPacketSize 5.
/// </summary>
internal sealed unsafe class Loopback : IDisposable
{
    public const int Rate = 48_000;
    public const int Channels = 2;

    private static readonly Guid AudioClientId = new("1cb9ad4c-dbfa-4c32-b178-c2f568a703b2");
    private static readonly Guid CaptureClientId = new("c8adbd64-e71e-48a0-a4de-185c395cd317");
    private static readonly Guid CompletionHandlerId = new("41d949ab-9862-444a-80f6-c261334da5eb");
    private static readonly Guid AgileId = new("94ea2b94-e9cc-49e0-c0ff-ee64ca8f5b90");
    private static readonly Guid UnknownId = new("00000000-0000-0000-c000-000000000046");

    private const uint LoopbackFlag = 0x00020000;
    private const uint EventCallback = 0x00040000;
    private const uint AutoConvert = 0x80000000;
    private const uint DefaultQuality = 0x08000000;
    private const uint Silent = 0x2;

    [StructLayout(LayoutKind.Sequential)]
    private struct ActivationParams
    {
        public int ActivationType;
        public uint TargetProcessId;
        public int ProcessLoopbackMode;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Blob
    {
        public ushort Type;
        public ushort Reserved1, Reserved2, Reserved3;
        public uint Size;
        public IntPtr Data;
    }

    [StructLayout(LayoutKind.Sequential, Pack = 2)]
    private struct WaveFormat
    {
        public ushort FormatTag;
        public ushort Channels;
        public uint SamplesPerSec;
        public uint AvgBytesPerSec;
        public ushort BlockAlign;
        public ushort BitsPerSample;
        public ushort Size;
    }

    [DllImport("mmdevapi.dll", ExactSpelling = true, CharSet = CharSet.Unicode)]
    private static extern int ActivateAudioInterfaceAsync(string path, in Guid riid, Blob* parameters, IntPtr handler, out IntPtr operation);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr CreateEventW(IntPtr attributes, int manual, int initial, IntPtr name);

    [DllImport("kernel32.dll")]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll")]
    private static extern int CloseHandle(IntPtr handle);

    private IntPtr _client;
    private IntPtr _capture;
    private readonly IntPtr _ready;
    private readonly Thread _thread;
    private volatile bool _stopped;
    private readonly Action<float[], int> _heard;

    private Loopback(IntPtr client, IntPtr capture, IntPtr ready, Action<float[], int> heard)
    {
        _client = client;
        _capture = capture;
        _ready = ready;
        _heard = heard;
        _thread = new Thread(Run) { IsBackground = true, Name = "sound capture", Priority = ThreadPriority.Highest };
    }

    private static IntPtr Slot(IntPtr com, int index) => (*(IntPtr**)com)[index];

    /// <summary>
    /// Starts capturing every process's sound but this process tree's
    /// (<paramref name="include"/> false), or one process tree's alone, as
    /// interleaved stereo floats at 48 kHz, handed to <paramref name="heard"/>
    /// on a capture thread with the frame count. Throws where the system
    /// will not.
    /// </summary>
    public static Loopback Start(uint process, bool include, Action<float[], int> heard)
    {
        var client = Activate(process, include);
        IntPtr capture = IntPtr.Zero, ready = IntPtr.Zero;
        try
        {
            var format = new WaveFormat
            {
                FormatTag = 3, // WAVE_FORMAT_IEEE_FLOAT
                Channels = Channels,
                SamplesPerSec = Rate,
                AvgBytesPerSec = Rate * Channels * 4,
                BlockAlign = Channels * 4,
                BitsPerSample = 32,
                Size = 0,
            };
            // A fifth of a second in hand: the capture is drained every ten
            // milliseconds, and a share that falls behind is worse than one
            // that loses a few.
            var initialize = (delegate* unmanaged[Stdcall]<IntPtr, int, uint, long, long, WaveFormat*, IntPtr, int>)Slot(client, 3);
            Marshal.ThrowExceptionForHR(initialize(client, 0, LoopbackFlag | EventCallback | AutoConvert | DefaultQuality, 2_000_000, 0, &format, IntPtr.Zero));
            ready = CreateEventW(IntPtr.Zero, 0, 0, IntPtr.Zero);
            var setEvent = (delegate* unmanaged[Stdcall]<IntPtr, IntPtr, int>)Slot(client, 13);
            Marshal.ThrowExceptionForHR(setEvent(client, ready));
            var id = CaptureClientId;
            IntPtr service;
            var getService = (delegate* unmanaged[Stdcall]<IntPtr, Guid*, IntPtr*, int>)Slot(client, 14);
            Marshal.ThrowExceptionForHR(getService(client, &id, &service));
            capture = service;
            var loopback = new Loopback(client, capture, ready, heard);
            var start = (delegate* unmanaged[Stdcall]<IntPtr, int>)Slot(client, 10);
            Marshal.ThrowExceptionForHR(start(client));
            loopback._thread.Start();
            return loopback;
        }
        catch (Exception)
        {
            if (capture != IntPtr.Zero) Marshal.Release(capture);
            if (ready != IntPtr.Zero) CloseHandle(ready);
            Marshal.Release(client);
            throw;
        }
    }

    /// <summary>The audio client for a process loopback, activated and waited for.</summary>
    private static IntPtr Activate(uint process, bool include)
    {
        var parameters = new ActivationParams { ActivationType = 1, TargetProcessId = process, ProcessLoopbackMode = include ? 0 : 1 };
        var blob = new Blob { Type = 65 /* VT_BLOB */, Size = (uint)sizeof(ActivationParams), Data = (IntPtr)(&parameters) };
        var handler = Completion.Make();
        try
        {
            Marshal.ThrowExceptionForHR(ActivateAudioInterfaceAsync(@"VAD\Process_Loopback", in AudioClientId, &blob, handler, out var operation));
            try
            {
                if (!Completion.Wait(handler, 5000)) throw new TimeoutException("the sound capture was not activated");
                int result;
                IntPtr activated;
                var get = (delegate* unmanaged[Stdcall]<IntPtr, int*, IntPtr*, int>)Slot(operation, 3);
                Marshal.ThrowExceptionForHR(get(operation, &result, &activated));
                Marshal.ThrowExceptionForHR(result);
                try
                {
                    var id = AudioClientId;
                    Marshal.ThrowExceptionForHR(Marshal.QueryInterface(activated, in id, out var client));
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

    private void Run()
    {
        var floats = new float[Rate / 10 * Channels];
        var getNext = (delegate* unmanaged[Stdcall]<IntPtr, uint*, int>)Slot(_capture, 5);
        var getBuffer = (delegate* unmanaged[Stdcall]<IntPtr, float**, uint*, uint*, ulong*, ulong*, int>)Slot(_capture, 3);
        var release = (delegate* unmanaged[Stdcall]<IntPtr, uint, int>)Slot(_capture, 4);
        while (!_stopped)
        {
            WaitForSingleObject(_ready, 100);
            if (_stopped) break;
            while (true)
            {
                uint packet;
                if (getNext(_capture, &packet) < 0 || packet == 0) break;
                float* data;
                uint frames, flags;
                ulong position, counter;
                if (getBuffer(_capture, &data, &frames, &flags, &position, &counter) < 0) break;
                var count = (int)frames * Channels;
                if (floats.Length < count) floats = new float[count];
                if ((flags & Silent) != 0) Array.Clear(floats, 0, count);
                else new ReadOnlySpan<float>(data, count).CopyTo(floats);
                release(_capture, frames);
                try
                {
                    _heard(floats, (int)frames);
                }
                catch (Exception fault)
                {
                    Diagnostics.Log.Line($"captured sound not sent: {fault.Message}");
                }
            }
        }
    }

    public void Dispose()
    {
        _stopped = true;
        if (_thread.IsAlive) _thread.Join(500);
        if (_client != IntPtr.Zero)
        {
            var stop = (delegate* unmanaged[Stdcall]<IntPtr, int>)Slot(_client, 11);
            stop(_client);
        }
        if (_capture != IntPtr.Zero) Marshal.Release(_capture);
        if (_client != IntPtr.Zero) Marshal.Release(_client);
        _capture = _client = IntPtr.Zero;
        CloseHandle(_ready);
    }

    /// <summary>
    /// An IActivateAudioInterfaceCompletionHandler made by hand in unmanaged
    /// memory: a vtable of four functions and a reference count beside the
    /// event it sets, answering IAgileObject as the activation requires.
    /// </summary>
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

        [DllImport("kernel32.dll")]
        private static extern int SetEvent(IntPtr handle);
    }
}
