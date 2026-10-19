using Tomoshibi.Diagnostics;
using Tomoshibi.Interop;
using Windows.Media;
using Windows.Media.Audio;
using Windows.Media.MediaProperties;
using Windows.Media.Render;

namespace Tomoshibi.Media;

/// <summary>
/// Plays every remote sound through one audio graph to the default output:
/// a frame input node per sound, so each keeps its own channel count and a
/// level of its own later (the web's per-person volume, hearing.ts).
///
/// Never made in a muted launch (TOMOSHIBI_MUTE): the graph is not even
/// opened, so a test cannot reach the speakers by any path.
/// </summary>
internal sealed class Playback : IDisposable
{
    /// <summary>
    /// The most a sound may queue before what arrives is dropped: a fifth of
    /// a second. Past that the voice is late enough to talk over.
    /// </summary>
    private const double MostQueuedSeconds = 0.2;

    private readonly AudioGraph _graph;
    private readonly AudioDeviceOutputNode _output;
    private readonly Dictionary<string, AudioFrameInputNode> _inputs = new();
    private readonly object _gate = new();

    private Playback(AudioGraph graph, AudioDeviceOutputNode output)
    {
        _graph = graph;
        _output = output;
    }

    public static async Task<Playback?> Open()
    {
        var made = await AudioGraph.CreateAsync(new AudioGraphSettings(AudioRenderCategory.Communications));
        if (made.Status != AudioGraphCreationStatus.Success)
        {
            Log.Line($"no audio graph for playback: {made.Status}");
            return null;
        }
        var output = await made.Graph.CreateDeviceOutputNodeAsync();
        if (output.Status != AudioDeviceNodeCreationStatus.Success)
        {
            Log.Line($"no output device for playback: {output.Status}");
            made.Graph.Dispose();
            return null;
        }
        made.Graph.Start();
        return new Playback(made.Graph, output.DeviceOutputNode);
    }

    /// <summary>Interleaved little-endian 16-bit samples for one sound, from the core's thread.</summary>
    public unsafe void Play(string sid, byte[] pcm, uint rate, uint channels)
    {
        AudioFrameInputNode node;
        lock (_gate)
        {
            if (!_inputs.TryGetValue(sid, out node!))
            {
                var format = AudioEncodingProperties.CreatePcm(rate, channels, 32);
                format.Subtype = MediaEncodingSubtypes.Float;
                node = _graph.CreateFrameInputNode(format);
                node.AddOutgoingConnection(_output);
                _inputs[sid] = node;
            }
        }
        if (node.QueuedSampleCount > rate * MostQueuedSeconds) return;
        var samples = pcm.Length / 2;
        var frame = new AudioFrame((uint)(samples * sizeof(float)));
        using (var buffer = frame.LockBuffer(AudioBufferAccessMode.Write))
        using (var reference = buffer.CreateReference())
        {
            var (data, _) = ComCalls.Bytes(reference);
            var floats = (float*)data;
            for (var i = 0; i < samples; i++)
            {
                floats[i] = (short)(pcm[2 * i] | pcm[2 * i + 1] << 8) / 32768f;
            }
        }
        node.AddFrame(frame);
    }

    public void Forget(string sid)
    {
        lock (_gate)
        {
            if (_inputs.Remove(sid, out var node)) node.Dispose();
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            foreach (var node in _inputs.Values) node.Dispose();
            _inputs.Clear();
        }
        _graph.Stop();
        _graph.Dispose();
    }
}
