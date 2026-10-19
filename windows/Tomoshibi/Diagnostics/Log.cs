using System.Diagnostics;
using System.Text;

namespace Tomoshibi.Diagnostics;

/// <summary>
/// One line at a time to the debugger and, when a test names one
/// (TOMOSHIBI_DEBUG_LOG), to a file it reads back. Never a passphrase, a
/// token or a server address: the file is read over a shared folder.
/// </summary>
public static class Log
{
    private static readonly object Gate = new();
    private static StreamWriter? _file;

    public static void Open(string? path)
    {
        if (path == null) return;
        lock (Gate)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            _file = new StreamWriter(new FileStream(path, FileMode.Create, FileAccess.Write, FileShare.ReadWrite), new UTF8Encoding(false))
            {
                AutoFlush = true,
            };
        }
    }

    public static void Line(string text)
    {
        var line = $"{DateTime.Now:HH:mm:ss.fff} {text}";
        Debug.WriteLine(line);
        lock (Gate)
        {
            _file?.WriteLine(line);
        }
    }
}
