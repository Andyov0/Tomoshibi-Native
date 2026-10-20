using Tomoshibi.Basics;
using Xunit;

namespace Tomoshibi.Tests;

/*
 * The preferences file is written on the UI thread whenever a choice
 * changes, each letter of a name typed among them. A virus scanner or a
 * backup holding the file, or the one written beside it, made the write
 * throw into the text box's handler, and the app closed with what was
 * typed. A held file is a write put off, never an exception.
 */
public sealed class FileDefaultsTests : IDisposable
{
    private readonly string _folder = Path.Combine(Path.GetTempPath(), "tomoshibi-defaults-" + Guid.NewGuid().ToString("N"));
    private string Kept => Path.Combine(_folder, "preferences.json");

    public FileDefaultsTests() => Directory.CreateDirectory(_folder);

    public void Dispose() => Directory.Delete(_folder, recursive: true);

    [Fact]
    public void AFileHeldByAnotherProgramPutsTheWriteOffAndTheNextChangeWritesEverything()
    {
        var said = new List<string>();
        var defaults = new FileDefaults(Kept, said.Add);
        defaults.Set("meet-live.name", "Lee");
        using (new FileStream(Kept, FileMode.Open, FileAccess.Read, FileShare.None))
        {
            defaults.Set("meet-live.name", "Lee Kim");
            defaults.Set("meet-live.camera-on", true);
            Assert.Equal("Lee Kim", defaults.String("meet-live.name"));
        }
        Assert.NotEmpty(said);
        defaults.Set("meet-live.relay", "tokyo");
        var read = new FileDefaults(Kept);
        Assert.Equal("Lee Kim", read.String("meet-live.name"));
        Assert.True(read.Bool("meet-live.camera-on"));
        Assert.Equal("tokyo", read.String("meet-live.relay"));
    }

    [Fact]
    public void AFileHeldBesideItIsNoErrorEither()
    {
        var defaults = new FileDefaults(Kept);
        // What is written first and moved over the choices.
        using (new FileStream(Kept + ".new", FileMode.Create, FileAccess.ReadWrite, FileShare.None))
        {
            defaults.Set("meet-live.name", "Lee");
            defaults.Remove("meet-live.name");
            defaults.Set("meet-live.name", "Kim");
        }
        defaults.Set("meet-live.locale", "zh-Hans");
        var read = new FileDefaults(Kept);
        Assert.Equal("Kim", read.String("meet-live.name"));
        Assert.Equal("zh-Hans", read.String("meet-live.locale"));
    }

    [Fact]
    public void AFileHalfWrittenIsAFreshStart()
    {
        File.WriteAllText(Kept, "{\"meet-live.name\": \"Le");
        var said = new List<string>();
        var defaults = new FileDefaults(Kept, said.Add);
        Assert.Null(defaults.String("meet-live.name"));
        Assert.Single(said);
    }
}
