namespace Tomoshibi.Basics;

/// <summary>
/// The identity a deployment gave this app on its last join, and the proof it
/// gave with it, kept together and sent back together (api.ts keeps
/// <c>meet-live.identity</c> and <c>meet-live.identity-proof</c> side by side
/// in the tab's storage; Android's KeptIdentity.kt and the Apple apps' are the
/// same).
///
/// A guest's identity is issued by the server and is on every roster in the
/// room, so anybody could send it back as their own and be seated in the
/// guest's place; the server keeps an issued identity only for whoever shows
/// the proof it handed out with it, and issues a fresh one otherwise. Kept
/// apart, or not at all, an app guest was a new participant at every
/// reconnect, move and "Join again", lost a pin an operator had set for it,
/// and read to everybody else as somebody leaving and a stranger arriving. So
/// the proof lives exactly where the identity lives and for exactly as long,
/// and an answer that comes without one (a passphrase or an account keeps its
/// identity by itself; an older server hands none out) clears it, as api.ts
/// clears it, so a proof is never sent beside an identity it is not for.
/// </summary>
public sealed record KeptIdentity(string? Identity = null, string? Proof = null)
{
    /// <summary>Where the two are kept for the deployment at <paramref name="baseUrl"/>: the identity under the name it always had, the proof beside it.</summary>
    public static (string Identity, string Proof) Keys(string baseUrl) =>
        ("tomoshibi.identity." + baseUrl, "tomoshibi.identity-proof." + baseUrl);

    public static KeptIdentity Read(IDefaults defaults, string baseUrl)
    {
        var keys = Keys(baseUrl);
        var identity = NonEmpty(defaults.String(keys.Identity));
        // A proof outliving its identity is about nobody.
        var proof = identity == null ? null : NonEmpty(defaults.String(keys.Proof));
        return new KeptIdentity(identity, proof);
    }

    /// <summary>What a join answered, kept whole: the identity, and its proof or none.</summary>
    public static void Keep(KeptIdentity answer, IDefaults defaults, string baseUrl)
    {
        var keys = Keys(baseUrl);
        defaults.Set(keys.Identity, answer.Identity);
        var proof = answer.Identity == null ? null : NonEmpty(answer.Proof);
        if (proof != null) defaults.Set(keys.Proof, proof);
        else defaults.Remove(keys.Proof);
    }

    private static string? NonEmpty(string? value) => string.IsNullOrEmpty(value) ? null : value;
}
