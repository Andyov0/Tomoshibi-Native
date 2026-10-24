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
            kept = new FileDefaults(Path.Combine(folder, "preferences.json"), Log.Line);
            secrets = new DpapiSecrets(Path.Combine(folder, "secrets"));
        }
        else
        {
            kept = launch.Rehearsal ? new MemoryDefaults() : new FileDefaults(Path.Combine(Folders.Local, "preferences.json"), Log.Line);
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
        // What the deployment says between releases (Remote), asked of the
        // same deployment as the updates, or read from a test's file; kept in
        // the app's preferences (a rehearsal's in memory), and what was kept
        // applied now, before the window's first frame, so that the first
        // screen is drawn with the phrases and the switches it will have.
        IRemoteSource? remoteSource = launch.RemoteFile is { } remoteFile ? new FileRemoteSource(remoteFile)
            : asksForUpdates ? new CoreRemoteSource(launch.Base ?? uniffi.tomoshibi_protocol.TomoshibiProtocolMethods.HomeDeployment())
            : null;
        app.Remote = new Remote(remoteSource, app.Defaults, new RealUpdateClock());
        app.Remote.Restore();
        if (asksForUpdates)
        {
            var source = new CoreUpdateSource(launch.Base ?? uniffi.tomoshibi_protocol.TomoshibiProtocolMethods.HomeDeployment(), UpdatesFolder(launch),
                Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread());
            var quitOn = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
            IUpdateInstaller installer = ClientDescription.Packaged
                ? new PackageInstaller()
                : new SetupInstaller(() => quitOn.TryEnqueue(() => Current.Exit()));
            app.Updates = new Updates(source, installer, ClientDescription.Version, new RealUpdateClock());
            // Each answer heard said once in the log, which a test reads.
            var heard = "";
            app.Remote.Changed += () =>
            {
                var answer = app.Remote.Answer;
                var now = $"{answer.Revision} off [{string.Join(",", answer.Off.Order())}] notice {answer.Notice?.Id ?? "none"}";
                if (now == heard) return;
                heard = now;
                Log.Line($"remote: {now}");
            };
            // Each step said once in the log, which a test reads.
            var said = "";
            app.Updates.Changed += () =>
            {
                var now = app.Updates.Now switch
                {
                    Updates.Step.Offered o => $"offered {o.Offer.Version}{(o.Problem != null ? $" ({o.Problem})" : "")}",
                    Updates.Step.Downloading d => $"downloading {d.Offer.Version}",
                    Updates.Step.Held h => $"held {h.Offer.Version}",
                    Updates.Step.Installing i => $"installing {i.Offer.Version}{(i.Started ? ", started" : "")}",
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
        // The setup that opened this app keeps its own window up until this
        // one's first frame is on the screen, so that nothing is left on it in
        // between (SetupSignals). Told only where a setup is waiting.
        if (SetupSignals.AppShownAwaited())
        {
            void Drawn(object? sender, object e)
            {
                Microsoft.UI.Xaml.Media.CompositionTarget.Rendering -= Drawn;
                _ = SayShownOnceOnScreen(_window?.Content);
            }
            Microsoft.UI.Xaml.Media.CompositionTarget.Rendering += Drawn;
        }
        // A link handed over by a later launch: the window comes forward and
        // the link is opened here, on the UI thread.
        var ui = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        Program.Handed.Listen(link => ui.TryEnqueue(() =>
        {
            _window?.ComeForward();
            if (link == null) return;
            Log.Line("handed a link");
            _ = app.Open(link);
        }));
        _ = app.Start();
    }

    /// <summary>
    /// Tells the setup that opened this app that its window is on the screen,
    /// once the frame its first turn of rendering drew has reached it. Told
    /// at the second turn of rendering, the setup's window went while this
    /// one stood black for another second, on the first run after its files
    /// had been replaced, when the system reads over every file the app
    /// loads: a frame the window's thread has rendered is not yet one the
    /// desktop shows. So the compositor's commit of it is waited for
    /// (RequestCommitAsync), then the desktop compositor's presenting of what
    /// was committed (SetupSignals.WaitForThePresent). Told all the same if
    /// any of it fails, rather than leave the setup waiting its thirty seconds.
    /// </summary>
    private static async Task SayShownOnceOnScreen(UIElement? root)
    {
        try
        {
            if (root != null) await Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(root).Compositor.RequestCommitAsync();
            await Task.Run(SetupSignals.WaitForThePresent);
            Log.Line("the window's first frame is on the screen; told the setup");
        }
        catch (Exception fault)
        {
            Log.Line($"the window's first frame could not be waited for: {fault.Message}");
        }
        SetupSignals.SayAppShown();
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
    /// downloading, installing, installing-started, failed or current), with the notes a release
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
            "installing-started" => new Updates.Step.Installing(offer, Started: true),
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
