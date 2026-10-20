using System.Text;

namespace Tomoshibi.Basics;

/// <summary>
/// Which of the monogram's colours a person is (TomoshibiBasics'
/// <c>Monograms.index</c>; Android's kit/Notices.kt <c>Monograms</c>):
/// FNV-1a over the identity's UTF-8, in 64-bit unsigned arithmetic that wraps
/// as Swift's <c>&amp;*</c> does, modulo the number of colours. Any other
/// hash puts the same person in another colour here than on the iPhone and
/// Android, which the tests hold to the same thirteen answers as theirs.
/// </summary>
public static class Monograms
{
    public static ulong Hash(string text)
    {
        var hash = 1469598103934665603UL;
        foreach (var b in Encoding.UTF8.GetBytes(text))
        {
            hash ^= b;
            unchecked
            {
                hash *= 1099511628211UL;
            }
        }
        return hash;
    }

    public static int Index(string identity, int count) => (int)(Hash(identity) % (ulong)count);
}
