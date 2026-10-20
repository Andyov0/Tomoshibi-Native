using Microsoft.UI.Xaml;
using Tomoshibi.Basics;
using Tomoshibi.Diagnostics;
using Tomoshibi.Media;
using Tomoshibi.Model;
using Tomoshibi.UI;
using Tomoshibi.UI.Kit;

namespace Tomoshibi;

public partial class App : Application
{
    private MainWindow? _window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) => Log.Line($"unhandled: {e.Exception}");
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var launch = Program.Launch;
        Log.Open(launch.LogFile);
        Log.Line($"Tomoshibi {ClientDescription.Version} for Windows{(ClientDescription.Packaged ? ", packaged" : "")}, core {uniffi.tomoshibi_core.TomoshibiCoreMethods.CoreVersion()}, {System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture}");
        // Before the first deployment is made, which reads it then.
        ClientDescription.Tell();

        // A test's choices are shown while it runs and kept nowhere: what it
        // would write over is the owner's (AppModel.rehearsal).
        IDefaults kept;
        ISecrets secrets;
        if (launch.KeepIn is { } keep)
        {
            // A test's own store, never the owner's (Launch.KeepIn).
            var folder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Tomoshibi-tests", keep);
            Directory.CreateDirectory(folder);
            kept = new FileDefaults(Path.Combine(folder, "preferences.json"));
            secrets = new DpapiSecrets(Path.Combine(folder, "secrets"));
        }
        else
        {
            kept = launch.Rehearsal ? new MemoryDefaults() : FileDefaults.App();
            secrets = launch.Rehearsal ? new MemorySecrets() : DpapiSecrets.App();
        }

        var tables = LoadPhrases();
        Localization.Shared = new Localization(tables, kept, Shared.SystemLanguages) { Keeps = !launch.Rehearsal };
        if (launch.Language is { } code && Languages.Of(code) is { } language) Localization.Shared.Choose(language);
        Appearance.Shared = new Appearance(kept) { Keeps = !launch.Rehearsal };
        Ink.ApplyAccent();
        // The sound panel's record: in a rehearsal the memory store, which
        // keeps nothing past the launch, so that a test's mute is never
        // written into the owner's book.
        Hearing.Shared = new Hearing(kept);

        if (launch.SegmentCheck is { } portraits)
        {
            _ = Task.Run(() =>
            {
                try
                {
                    Log.Line(Segmenter.Check(portraits));
                }
                catch (Exception fault)
                {
                    Log.Line($"segment check: {fault.GetType().Name}: {fault.Message}");
                }
            });
        }
        var media = new CallMedia(launch);
        var app = new AppModel(launch, media, kept, secrets) { OpenedWith = Program.OpenedWith };
        if (Program.OpenedWith != null) Log.Line("opened with a link");
        _window = new MainWindow(app);
        _window.Activate();
        // A link handed over by a later launch: the window comes forward and
        // the link is opened here, on the UI thread.
        var ui = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        Program.Handed += link => ui.TryEnqueue(() =>
        {
            _window?.Activate();
            if (link == null) return;
            Log.Line("handed a link");
            _ = app.Open(link);
        });
        _ = app.Start();
    }

    /// <summary>
    /// The phrase table, beside the app: android/app/src/main/assets/phrases.json,
    /// which android/i18n.mjs writes from the catalog apple/i18n/generate.mjs
    /// builds, linked into the build so that the three platforms read one
    /// table rather than copies that drift.
    /// </summary>
    private static IReadOnlyDictionary<Language, IReadOnlyDictionary<string, string>> LoadPhrases()
    {
        try
        {
            return Localization.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "phrases.json")));
        }
        catch (Exception fault)
        {
            // English, which every phrase is keyed by, rather than no app.
            Log.Line($"no phrase table: {fault.GetType().Name}");
            return new Dictionary<Language, IReadOnlyDictionary<string, string>>();
        }
    }
}
