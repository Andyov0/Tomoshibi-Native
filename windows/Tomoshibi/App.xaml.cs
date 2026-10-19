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
        var launch = Launch.FromEnvironment();
        Log.Open(launch.LogFile);
        Log.Line($"Tomoshibi for Windows, core {uniffi.tomoshibi_core.TomoshibiCoreMethods.CoreVersion()}, {System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture}");
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

        var media = new CallMedia(launch);
        var app = new AppModel(launch, media, kept, secrets);
        _window = new MainWindow(app);
        _window.Activate();
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
