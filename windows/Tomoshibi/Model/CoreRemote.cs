using Tomoshibi.Basics;
using uniffi.tomoshibi_protocol;

namespace Tomoshibi.Model;

/// <summary>
/// What the deployment says to the apps, through the core (api/remote.rs),
/// which asks as the app (the platform and version in its user agent decide
/// the answer), reads it a section at a time, and works out each language's
/// share of it (Android's CoreRemote.kt, CoreRemote.swift on Apple).
///
/// A deployment of its own rather than the app's, as the updates have
/// (CoreUpdateSource): what is said is said to whoever has the app, signed in
/// or not, and is needed before the app's own deployment has answered, or
/// when it never does. ClientDescription is told before it is made
/// (App.OnLaunched), so its user agent names the version the answer is for.
/// </summary>
internal sealed class CoreRemoteSource : IRemoteSource
{
    private readonly Lazy<Deployment> _deployment;

    public CoreRemoteSource(string baseUrl) => _deployment = new Lazy<Deployment>(() => new Deployment(baseUrl));

    public async Task<RemoteAnswer> Fetch() => Answer(await _deployment.Value.RemoteConfig());

    public RemoteAnswer? Read(string kept) => TomoshibiProtocolMethods.ParseRemoteConfig(kept) is { } config ? Answer(config) : null;

    /// <summary>
    /// The core's answer, worked out for each of the app's four languages by
    /// the core's own rules: the phrases in that language or in it without
    /// its region or script, with no English stand-in (<c>remote_phrases</c>);
    /// the notice in that language, else without its suffix, else English
    /// (<c>remote_notice_text</c>).
    /// </summary>
    internal static RemoteAnswer Answer(RemoteConfig config)
    {
        var phrases = new Dictionary<Language, IReadOnlyDictionary<string, string>>();
        foreach (var language in Languages.All)
        {
            var laid = TomoshibiProtocolMethods.RemotePhrases(config, language.Code());
            if (laid.Count > 0) phrases[language] = laid;
        }
        LobbyNotice? notice = null;
        if (config.Notice is { } said)
        {
            var text = new Dictionary<Language, string>();
            foreach (var language in Languages.All)
            {
                if (TomoshibiProtocolMethods.RemoteNoticeText(said, language.Code()) is { } words) text[language] = words;
            }
            notice = new LobbyNotice(said.Id, said.Warning, text, said.Until is { } until ? DateTimeOffset.FromUnixTimeSeconds(until) : null);
        }
        return new RemoteAnswer
        {
            Revision = config.Revision,
            Phrases = phrases,
            Off = config.Off.ToHashSet(),
            Params = config.Params,
            Notice = notice,
            Kept = config.Kept,
        };
    }
}

/// <summary>
/// A debug launch's stand-in for the deployment (TOMOSHIBI_REMOTE=file): the
/// file is read whenever the deployment would be asked, as the answer the
/// deployment would give, through the core's own reading of one. For
/// pictures of a notice and of features switched off: the kit's deployment
/// has no /api/apps/remote, and a test never asks the home one.
/// </summary>
internal sealed class FileRemoteSource(string file) : IRemoteSource
{
    public async Task<RemoteAnswer> Fetch()
    {
        var text = await File.ReadAllTextAsync(file);
        return Read(text) ?? throw new InvalidDataException("the file is not an answer the core can read");
    }

    public RemoteAnswer? Read(string kept) => TomoshibiProtocolMethods.ParseRemoteConfig(kept) is { } config ? CoreRemoteSource.Answer(config) : null;
}
