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
        // Updates are asked of the deployment the app talks to: a test's in a
        // debug build given one, and in a release build home, the only one
        // its core will speak to. A debug build started by hand asks nobody.
#if DEBUG
        var asksForUpdates = launch.Base != null;
#else
        var asksForUpdates = true;
#endif
        if (asksForUpdates)
        {
            var source = new CoreUpdateSource(launch.Base ?? uniffi.tomoshibi_protocol.TomoshibiProtocolMethods.HomeDeployment(), UpdatesFolder(launch),
                Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread());
            app.Updates = new Updates(source, new PackageInstaller(), ClientDescription.Version, new RealUpdateClock());
            // Each step said once in the log, which a test reads.
            var said = "";
            app.Updates.Changed += () =>
            {
                var now = app.Updates.Now switch
                {
                    Updates.Step.Offered o => $"offered {o.Offer.Version}{(o.Problem != null ? $" ({o.Problem})" : "")}",
                    Updates.Step.Downloading d => $"downloading {d.Offer.Version}",
                    Updates.Step.Held h => $"held {h.Offer.Version}",
                    Updates.Step.Installing i => $"installing {i.Offer.Version}",
                    _ => "none",
                };
                if (now == said) return;
                said = now;
                Log.Line($"update: {now}");
            };
            if (launch.Show is { } show && show.StartsWith("update:", StringComparison.Ordinal)) PoseUpdate(app.Updates, show["update:".Length..].Replace(":about", ""));
        }
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

    /// <summary>
    /// A step of an update to take a picture of (TOMOSHIBI_SHOW=update:offered,
    /// downloading, installing, failed or current), with the notes a release
    /// writes (update-kit's publish.py), as Android's screenshot tests pose
    /// theirs; nothing is asked of the deployment.
    /// </summary>
    private static void PoseUpdate(Updates updates, string step)
    {
        var notes = new Dictionary<string, string>
        {
            ["en"] = "- In-app updates: a newer version is offered in the lobby, then downloaded and installed once you agree.\n- Several fixes.",
            ["zh-Hans"] = "· 新增应用内更新：发现新版本会在大厅提示，同意后自动下载并安装。\n· 修复若干问题。",
            ["zh-Hant"] = "· 新增 App 內更新：發現新版本會在大廳提示，同意後自動下載並安裝。\n· 修正若干問題。",
            ["ja"] = "・アプリ内アップデートに対応：新しいバージョンがあるとロビーでお知らせし、同意するとダウンロードしてインストールします。\n・いくつかの問題を修正しました。",
        };
        var offer = new Offer("0.1.0.8", 56_093_610, language => notes.GetValueOrDefault(language));
        updates.Pose(step switch
        {
            "downloading" => new Updates.Step.Downloading(offer, 23_400_000, 56_093_610),
            "installing" => new Updates.Step.Installing(offer),
            "failed" => new Updates.Step.Offered(offer, Words.T("The download was damaged and has been deleted. Try again.")),
            "current" => new Updates.Step.None(),
            _ => new Updates.Step.Offered(offer),
        }, step == "current" ? Updates.Said.Current : null);
    }

    /// <summary>
    /// Where downloaded builds wait: the package's own cache when the app is
    /// installed from one, which Windows clears with the app; beside a
    /// test's store otherwise, never the owner's.
    /// </summary>
    private static string UpdatesFolder(Launch launch)
    {
        if (ClientDescription.Packaged) return Path.Combine(Windows.Storage.ApplicationData.Current.LocalCacheFolder.Path, "updates");
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return launch.Rehearsal ? Path.Combine(local, "Tomoshibi-tests", "updates") : Path.Combine(local, "Tomoshibi", "updates");
    }
}
