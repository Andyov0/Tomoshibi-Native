package app.tomoshibi.model

import android.content.Context
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.BuildConfig
import app.tomoshibi.basics.NoticeTimes
import app.tomoshibi.basics.t
import app.tomoshibi.media.CallMedia
import java.net.URI
import java.net.URLDecoder
import java.time.Instant
import java.time.OffsetDateTime
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.tomoshibi_core.CallOptions
import uniffi.tomoshibi_core.IceServer
import uniffi.tomoshibi_protocol.Account
import uniffi.tomoshibi_protocol.Arrangement
import uniffi.tomoshibi_protocol.Deployment
import uniffi.tomoshibi_protocol.DeploymentInfo
import uniffi.tomoshibi_protocol.DoorState
import uniffi.tomoshibi_protocol.InviteInfo
import uniffi.tomoshibi_protocol.MadeInvite
import uniffi.tomoshibi_protocol.ProtocolException
import uniffi.tomoshibi_protocol.RelayList
import uniffi.tomoshibi_protocol.forgetRelayMeasurement
import uniffi.tomoshibi_protocol.homeDeployment
import uniffi.tomoshibi_protocol.hostNotComingAfterMs
import uniffi.tomoshibi_protocol.knockAskEveryMs
import uniffi.tomoshibi_protocol.knockGiveUpAfterMs
import uniffi.tomoshibi_protocol.meetingAskEveryMs
import uniffi.tomoshibi_protocol.scopeOf

/** A guest link refused, as a sentence. */
class GuestLinkRefusal(val sentence: String) : Exception(sentence)

/**
 * Where the app is, and everything the screens before a call need (the Apple
 * apps' AppModel).
 *
 * Follows the web's App.tsx and doorway.ts, with one step the web never needs:
 * a browser is already at its deployment, while an app has to be told which
 * one. After that the order is the web's: who is signed in and what the
 * deployment allows decide between the lobby and the join form, an invitation
 * in a link wins over everything, and a call that the server moves is joined
 * again on its own.
 *
 * Its fields are Compose snapshot state, so a screen that reads one redraws
 * when it changes, as SwiftUI does from an @Observable. Every method is for the
 * main thread.
 */
class AppModel(
    /** The app's preferences, where the web's names are kept. */
    defaults: Defaults,
    /** The Keystore, for the account's cookie and the join passphrase. */
    secrets: Secrets,
    /** What a call is made of on this device; null makes every join fail, which only a test wants. */
    private val media: CallMedia?,
    /** A test driving this launch; honoured only in a debug build (see [rehearsal]). */
    private val launch: DebugLaunch? = null,
    /** The deployment this app is for, from the core, which holds every client to one address. */
    home: String = homeDeployment(),
    context: CoroutineContext = Dispatchers.Main.immediate,
    /**
     * Told whether the person's own choices may be kept: false in a
     * rehearsal. The language and the look are the owner's too, and a test
     * that pressed a language tile would leave the app opening in it.
     */
    keepChoices: (Boolean) -> Unit = {},
) {
    enum class Screen {
        /** The deployment the app opens on is being asked about; nothing to fill in yet. */
        Starting,
        Unreachable, Lobby, Prejoin, Knocking, Waiting, Call, Ended,
    }

    /**
     * Why the last call ended (App.tsx, the `ended` handler): three endings
     * that look alike from the inside, a call that simply stopped, and want
     * three different sentences. Somebody told only "You left the call."
     * after the host closed the room reads the call as having broken.
     */
    enum class Ending {
        /** They pressed leave. */
        Left,
        /** The room was closed for everybody (ROOM_DELETED, which the web calls DISCONNECT_ROOM_CLOSED). */
        Closed,
        /** They alone were taken out (PARTICIPANT_REMOVED). */
        Removed,
        /** Anything else: the connection failed and the client gave up. */
        Lost,
    }

    /**
     * A test is driving the app (a debug build launched with a deployment of
     * its own), and nothing it does is kept: not the server, the name, the
     * passphrase, the session or the identity. The tests run the very app
     * somebody uses, and the server they pointed it at was once kept as the
     * last one used, so the app opened by hand went to a loopback test
     * deployment where no account existed, and signing in could only fail.
     */
    val rehearsal: Boolean = BuildConfig.DEBUG && launch != null

    /** Where everything is kept: the person's own stores, or memory alone in a rehearsal. */
    private val defaults: Defaults = if (rehearsal) MemoryDefaults() else defaults
    private val secrets: Secrets = if (rehearsal) MemorySecrets() else secrets

    private val job = SupervisorJob()
    private val scope = CoroutineScope(job + context)

    private var screenState by mutableStateOf(Screen.Starting)
    var screen: Screen
        get() = screenState
        private set(value) {
            if (value != screenState) Log.i(TAG, "screen ${value.name.lowercase()}")
            screenState = value
        }

    var deployment by mutableStateOf<Deployment?>(null)
        private set
    var info by mutableStateOf<DeploymentInfo?>(null)
        private set
    var account by mutableStateOf<Account?>(null)
        private set
    var call by mutableStateOf<CallModel?>(null)
        private set

    private var relaysState by mutableStateOf<RelayList?>(null)
    var relays: RelayList?
        get() = relaysState
        private set(value) {
            relaysState = value
            dropRelayNotOffered()
        }

    private var asking by mutableIntStateOf(0)
    /**
     * Something is being asked of the deployment on the person's behalf, and
     * the buttons that would ask it again wait. Counted rather than set and
     * cleared, because the asking overlaps: a join that was called off is
     * still out until it comes back.
     */
    val busy: Boolean get() = asking > 0

    var ending by mutableStateOf(Ending.Left)
        private set

    private var problemState by mutableStateOf<String?>(null)
    private var telling = false
    /**
     * Something to tell the person before a call, at the foot of the screen.
     * Most of them fade (notices.ts `actionFailed`); [problemStays] marks the
     * ones about why they are not in a meeting, which stay until dismissed
     * (notices.ts `joinFailed`).
     */
    var problem: String?
        get() = problemState
        set(value) {
            problemState = value
            if (!telling) problemStays = false
        }
    var problemStays by mutableStateOf(false)
        private set

    /** Says something that stays until the person closes it. */
    fun tell(sentence: String) {
        telling = true
        problem = sentence
        problemStays = true
        telling = false
    }

    /**
     * The deployment this app talks to: always [home], except to a test that
     * names its own. Nobody can choose it, and no screen shows it: the apps
     * are for this deployment and not for anybody's own (the core holds a
     * release build to the same address).
     */
    internal var serverAddress by mutableStateOf(home)
        private set

    var room by mutableStateOf("")

    private var nameState by mutableStateOf(this.defaults.string(NAME_KEY) ?: "")
    /**
     * The web's own storage name, so the habit carries over. Only ever what
     * the person typed: an invitation's name is [invitedName] and is never
     * written here (PreJoin.tsx: "Not a name somebody else chose for them").
     */
    var name: String
        get() = nameState
        set(value) {
            nameState = value
            this.defaults.set(NAME_KEY, value)
        }

    private var passphraseState by mutableStateOf("")
    var passphrase: String
        get() = passphraseState
        set(value) {
            passphraseState = value
            this.secrets.write(passphraseKey, value)
        }

    /** Never stored, as the web never stores it (secrecy.ts). */
    var sealingWord by mutableStateOf("")

    private var relayChoiceState by mutableStateOf(this.defaults.string(RELAY_KEY))
    /**
     * Null picks whichever relay answers fastest. Kept under the web's own
     * name (api.ts `rememberRelay`), so that a machine somebody picked is the
     * one the next screen and the next launch say, and dropped again whenever
     * the deployment stops offering it ([dropRelayNotOffered]).
     */
    var relayChoice: String?
        get() = relayChoiceState
        set(value) {
            if (value == relayChoiceState) return
            relayChoiceState = value
            this.defaults.set(RELAY_KEY, value)
        }

    /**
     * Where the device choices are kept (DeviceMemory): nowhere in a
     * rehearsal, so a test that turns the camera on does not leave the
     * owner's next call starting that way.
     */
    val deviceMemory = DeviceMemory(if (rehearsal) null else this.defaults)

    /**
     * The choices a call reads that have no model of their own (the watch
     * ceiling, the music's quality and ducking, the devices last picked):
     * one for the app, as on Apple, so that the settings and a call read and
     * write the same values rather than copies that disagree.
     */
    val preferences: CallPreferences by lazy {
        CallPreferences(keeps = !rehearsal, defaults = this.defaults, deviceDefaults = deviceMemory.defaults)
    }

    private var startWithMicrophoneState by mutableStateOf(deviceMemory.joining.microphone)
    private var startWithCameraState by mutableStateOf(deviceMemory.joining.camera)
    /**
     * What starts on when somebody joins (preferences.ts `meet-live.devices`):
     * the microphone on and the camera off unless somebody chose otherwise.
     * Written as it is switched, so the settings and the join screen are one
     * choice and not two that disagree.
     */
    var startWithMicrophone: Boolean
        get() = startWithMicrophoneState
        set(value) {
            startWithMicrophoneState = value
            rememberDevices()
        }
    var startWithCamera: Boolean
        get() = startWithCameraState
        set(value) {
            startWithCameraState = value
            rememberDevices()
        }

    var invite by mutableStateOf<String?>(null)
        private set
    var invitation by mutableStateOf<InviteInfo?>(null)
        private set
    private var knockTask: Job? = null

    /** The arranged meeting a `?meeting=` link points at, while it is waited for (meeting.ts, Waiting.tsx). */
    var arrangement by mutableStateOf<Arrangement?>(null)
        private set
    var waiting by mutableStateOf(Waiting())
        private set
    private var meetingToken: String? = null
    private var waitTask: Job? = null

    /** Moved on by every new picture, so the old one is not shown from the image cache under the same address. */
    var pictureVersion by mutableIntStateOf(0)
        private set

    /** The arranged meetings, recent rooms and measured relays the screens before a call show. */
    val lobby: LobbyModel by lazy { LobbyModel(this, this.defaults) }

    /**
     * A rehearsal's identity, for its own joins only, as a fresh browser tab
     * has one of its own: two test apps in one room that shared the
     * machine's kept identity joined as the same participant, and the server
     * put the first out as a duplicate the moment the second arrived.
     */
    private var rehearsalIdentity: String? = null

    init {
        Rehearsal.install(rehearsal, this.defaults)
        keepChoices(!rehearsal)
        // Where the address somebody had typed used to be kept on the Apple
        // apps: never read here, and cleared, so that no copy of another
        // server lingers.
        if (!rehearsal) this.defaults.remove(SERVER_KEY)
        // A relay the person picked on their own deployment means nothing on
        // a test's.
        if (rehearsal) relayChoiceState = null
    }

    private val base: String get() = deployment?.baseUrl() ?: serverAddress
    private val passphraseKey: String get() = "passphrase:$base"
    private val cookieKey: String get() = "account:$base"
    private val identityKey: String get() = "tomoshibi.identity.$base"

    private fun rememberDevices() {
        deviceMemory.remember(DeviceMemory.Joining(camera = startWithCamera, microphone = startWithMicrophone))
    }

    /** sound.ts `rememberedOriginal`: whether the voice goes out as it is. */
    val originalSoundWanted: Boolean get() = deviceMemory.originalSound

    // MARK: Reaching a deployment

    private var started = false

    /**
     * Picks up where the last launch left off. A test's launch ([DebugLaunch])
     * takes the app straight into a call without a finger, microphone and
     * camera off so that no permission is asked for, unless it asks for the
     * microphone.
     *
     * Once for the life of the model, however many activities ask: an
     * activity made again would otherwise ask the deployment about itself all
     * over again and put the join screen up over a call that was still live.
     */
    suspend fun start() {
        if (started) return
        started = true
        scope.launch { keepRelaysFresh() }
        val launch = launch
        if (rehearsal && launch != null) {
            serverAddress = launch.base
            name = launch.name ?: "Native App"
            startWithMicrophone = launch.microphone
            startWithCamera = false
            connect()
            if (launch.name != null && launch.pass != null) signIn(launch.name, launch.pass)
            launch.passphrase?.let { passphrase = it }
            launch.sealingWord?.let { sealingWord = it }
            when {
                launch.autojoin != null -> {
                    prepare(launch.autojoin)
                    join()
                }
                launch.prepare != null -> prepare(launch.prepare)
                launch.open != null -> open(launch.open)
                launch.ended != null -> {
                    ending = when (launch.ended) {
                        "closed" -> Ending.Closed
                        "removed" -> Ending.Removed
                        "lost" -> Ending.Lost
                        else -> Ending.Left
                    }
                    screen = Screen.Ended
                }
            }
            return
        }
        connect()
    }

    suspend fun connect() {
        // Nothing here may take the screen off a call that is live; the only
        // way from a call to another deployment is to leave it first.
        if (call != null || movingTo != null) return
        asking++
        try {
            var address = serverAddress.trim()
            if (!address.contains("://")) address = "https://$address"
            // Before the deployment is made, which reads it then.
            ClientDescription.tell()
            val deployment = Deployment(address)
            secrets.read("account:" + deployment.baseUrl())?.let { cookie ->
                try {
                    deployment.restoreAccountCookie(cookie)
                } catch (fault: Exception) {
                    Log.w(TAG, "the kept session could not be restored")
                }
            }
            val info = deployment.deploymentInfo()
            this.deployment = deployment
            this.info = info
            serverAddress = deployment.baseUrl()
            passphraseState = secrets.read(passphraseKey) ?: ""
            account = try {
                deployment.me()
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Exception) {
                null
            }
            Log.i(TAG, if (account != null) "reached the deployment, signed in" else "reached the deployment")
            problem = null
            screen = if (room.isEmpty()) Screen.Lobby else Screen.Prejoin
            scope.launch {
                relays = try {
                    deployment.relays()
                } catch (fault: CancellationException) {
                    throw fault
                } catch (fault: Exception) {
                    null
                }
            }
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            Log.w(TAG, "the deployment did not answer: ${fault.javaClass.simpleName}")
            tell(explain(fault))
            // Opening on a deployment that will not answer leaves somebody
            // where they can try again, rather than on the mark.
            if (screen == Screen.Starting) screen = Screen.Unreachable
        } finally {
            asking--
        }
    }

    /** Asks again how much of the fleet is answering, for the light in the corner (Lobby.tsx `Fleet`). */
    suspend fun refreshRelays() {
        val deployment = deployment ?: return
        try {
            relays = deployment.relays()
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
        }
    }

    /** The light in the corner, asked again every minute while no call is showing: a reassurance rather than a monitor. */
    private suspend fun keepRelaysFresh() {
        while (scope.isActive) {
            delay(60_000)
            if (screen != Screen.Call) refreshRelays()
        }
    }

    /**
     * Whether there is a machine to choose at all (PreJoin.tsx `offerChoice`):
     * more than one relay, on a deployment that acts on the choice.
     */
    val offersRelayChoice: Boolean
        get() {
            val relays = relays ?: return false
            return relays.measure && relays.relays.size > 1
        }

    /**
     * A relay remembered from before that this deployment does not offer, or
     * no longer takes calls on, would be sent with every join and refused.
     */
    private fun dropRelayNotOffered() {
        val choice = relayChoice ?: return
        val relays = relays ?: return
        if (relays.relays.none { it.name == choice && !it.maintenance }) relayChoice = null
    }

    // MARK: Links

    /**
     * An invitation, an arranged meeting or a room, from a link opened with
     * the app (`https://<deployment>/?invite=T#/room` and its kin, or the same
     * under the `tomoshibi:` scheme).
     *
     * Not over a call. A link opened while one was live took the screen to the
     * link's join screen and left the call publishing underneath. The call
     * stays, and says how to get to the link. A join still out is called off
     * instead: the link is where they now mean to go.
     */
    suspend fun open(link: String) {
        call?.let {
            it.announce(CallToast(t("Leave this call to open that link."), lasts = NoticeTimes.AT_MOST))
            return
        }
        // Between the two rooms of a move there is no call to say it in, and
        // the move is that call coming back.
        if (movingTo != null) return
        cancelJoin()
        val parts = OpenedLink.parse(link) ?: return
        // A link opened at launch can arrive before the deployment has
        // answered; it is this deployment's or nobody's.
        if (deployment == null) connect()
        val deployment = deployment ?: return
        // A link to another server is refused rather than followed: the app
        // is for this deployment alone.
        if (!parts.isFor(deployment.baseUrl())) {
            tell(t("That link is for another server."))
            return
        }
        if (parts.meeting != null) {
            if (parts.room.isNotEmpty()) room = parts.room
            wait(parts.meeting, deployment)
            return
        }
        if (parts.invite != null) {
            val said = try {
                deployment.inviteInfo(parts.invite)
            } catch (fault: CancellationException) {
                throw fault
            } catch (_: Exception) {
                null
            }
            // An invitation that is no good is still somebody sent here on
            // purpose, and they are told which of three things it is, because
            // they are three different next moves (App.tsx): come back later,
            // the meeting is over, or ask for another link.
            val error = said?.error ?: if (said == null) "no_such_invite" else null
            if (error != null) {
                invite = null
                invitation = null
                val from = said?.from?.let(::instant)
                when {
                    error == "invite_not_yet" && from != null ->
                        tell(t("That invitation opens {when}. Come back then.", mapOf("when" to LobbyModel.said(from))))
                    error == "invite_expired" -> tell(t("That meeting has ended."))
                    error == "rate_limited" -> tell(t("Too many attempts. Try again in a moment."))
                    else -> tell(t("That invitation is no longer good. Ask for another."))
                }
                screen = Screen.Lobby
                return
            }
            invite = parts.invite
            invitation = said
            room = said?.room ?: parts.room
        } else if (parts.room.isNotEmpty()) {
            // A link with a room and nothing else is a room name, and only
            // somebody signed in is taken anywhere by one of those.
            if (account == null) {
                tell(t("{room} is by invitation. Ask the organiser for a link.", mapOf("room" to parts.room)))
                screen = Screen.Lobby
                return
            }
            room = parts.room
        }
        if (room.isNotEmpty()) screen = Screen.Prejoin
    }

    /** The name an invitation was made for, which the guest will wear whatever is typed (PreJoin.tsx `locked`). */
    val invitedName: String? get() = invitation?.name?.takeIf { it.isNotEmpty() }

    /** Somebody who arrived on an invitation link, who has a name to choose and nothing else to answer. */
    val arrivedInvited: Boolean get() = invitation != null

    /** The name this person will be called in the room: the account's, the invitation's, or what they typed. */
    val joiningName: String get() = account?.name ?: invitedName ?: name.trim()

    // MARK: Account

    suspend fun signIn(name: String, passphrase: String) {
        val deployment = deployment ?: return
        asking++
        try {
            account = deployment.signIn(name, passphrase)
            secrets.write(cookieKey, deployment.accountCookie())
            Log.i(TAG, "signed in")
            problem = null
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            Log.w(TAG, "sign-in refused: ${fault.javaClass.simpleName}")
            problem = explain(fault)
        } finally {
            asking--
        }
    }

    suspend fun signOut() {
        val deployment = deployment ?: return
        quietly { deployment.signOut() }
        // A rehearsal never wrote the session it signs out of; its secrets
        // are its own, in memory.
        secrets.write(cookieKey, null)
        account = null
        Log.i(TAG, "signed out")
    }

    /** Account.tsx `Picture`: the picture as a data URL, already scaled; the sentence to say if it did not take. */
    suspend fun setPicture(dataUrl: String): String? {
        val deployment = deployment ?: return null
        return try {
            account = deployment.setAvatar(dataUrl)
            pictureVersion++
            null
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            t("That picture could not be used.")
        }
    }

    suspend fun removePicture(): String? {
        val deployment = deployment ?: return null
        return try {
            account = deployment.removeAvatar()
            pictureVersion++
            null
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            t("That picture could not be used.")
        }
    }

    /** Account.tsx `Passphrase`, with its `explain`; null when it took. */
    suspend fun changePassphrase(current: String, next: String): String? {
        val deployment = deployment ?: return null
        return try {
            account = deployment.changePassphrase(current, next)
            null
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            explainPassphrase(fault)
        }
    }

    /** Account.tsx `GuestLinks`: a standing link into a scope's room; a failure is a [GuestLinkRefusal]. */
    suspend fun makeGuestLink(room: String, name: String, from: Instant?, until: Instant?): Result<MadeInvite> {
        val deployment = deployment ?: return Result.failure(GuestLinkRefusal(t("That link could not be made. Try again.")))
        return try {
            Result.success(deployment.createStandingInvite(room, name.trim(), from?.toEpochMilli(), until?.toEpochMilli()))
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            Result.failure(GuestLinkRefusal(explainLink(fault)))
        }
    }

    /** Stops every link into a room, returning how many stopped. */
    suspend fun stopGuestLinks(room: String): Result<UInt> {
        val deployment = deployment ?: return Result.failure(GuestLinkRefusal(t("That link could not be made. Try again.")))
        return try {
            Result.success(deployment.revokeInvites(room, null))
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            Result.failure(GuestLinkRefusal(explainLink(fault)))
        }
    }

    /**
     * Whether the account holds a permission, by what the server said; a
     * server older than permissions said nothing, which reads as the defaults
     * (manage/grants.ts `DEFAULTS`: everything but Emby).
     */
    fun holds(permission: String): Boolean {
        val account = account ?: return false
        account.can?.let { return permission in it }
        return permission != "emby"
    }

    // MARK: Joining

    /**
     * Only somebody signed in, or holding an invitation, is taken to a room by
     * its name: the app is a way in for the people who belong to the
     * deployment and for the people they send links to (the owner's rule,
     * 2026-10-15).
     */
    fun prepare(room: String) {
        // Never over a call that is live (`open`).
        if (call != null || movingTo != null) return
        if (account == null && invite == null && !rehearsal) {
            tell(t("Rooms here are by invitation. Open the link you were sent, or sign in."))
            return
        }
        this.room = room.trim().lowercase()
        problem = null
        screen = Screen.Prejoin
    }

    /** Straight into a room by name from the start and join sheet, which has already taken the choices the join screen would. */
    suspend fun enter(room: String) {
        if (account == null && invite == null && !rehearsal) {
            tell(t("Rooms here are by invitation. Open the link you were sent, or sign in."))
            return
        }
        this.room = room.trim().lowercase()
        problem = null
        join()
    }

    /**
     * A join is out and has not yet landed in its call, or was called off and
     * has not yet come back. One at a time: "Join again" pressed twice ran two
     * joins as the same participant, and the server put out whichever arrived
     * first.
     */
    var joining by mutableStateOf(false)
        private set

    /** Moved on by [cancelJoin]; a join compares the value it started with after every wait. */
    private var joinAttempt = 0

    /**
     * Calls off a join that has not yet landed: Back on the join screen,
     * Cancel on the start and join sheet, leaving the door or the wait. Those
     * buttons stayed live and did nothing to the join, and somebody who
     * pressed Cancel was put in the call a moment later with the microphone
     * on. Nothing once the call is showing: that is left with Leave.
     */
    fun cancelJoin() {
        if (!joining) return
        joinAttempt++
    }

    suspend fun join() = join(relay = null)

    /** [relay] is the relay to ask for on this join alone, in place of the one chosen; a move sends "" (App.tsx ~620). */
    private suspend fun join(relay: String?) {
        val deployment = deployment ?: return
        if (joining) return
        joining = true
        asking++
        val attempt = joinAttempt
        var landed = false
        try {
            // A join while a call is live replaces it; the call it replaces is
            // left first, rather than going on publishing out of sight.
            call?.let { old ->
                call = null
                old.leave()
            }
            rememberDevices()
            val media = media ?: throw IllegalStateException("no media for a call")
            // Signed in, the web sends the account's name and no passphrase:
            // the session is the proof (api.ts). A guest of a room held under
            // a scope is minted a mark of their own and a passphrase is
            // ignored, so none is sent (PreJoin.tsx `scopedGuest`).
            val signed = account != null
            val scopedGuest = arrivedInvited && scopeOf(room).isNotEmpty()
            Log.i(TAG, "joining $room")
            val grant = deployment.join(
                room, joiningName,
                if (signed || scopedGuest) "" else passphrase,
                if (rehearsal) rehearsalIdentity else defaults.string(identityKey),
                relay ?: relayChoice ?: "",
                invite,
            )
            // Every join's identity is kept: signing in or out makes the
            // server issue a new one, and a stale one sent back is refused.
            if (rehearsal) rehearsalIdentity = grant.identity else defaults.set(identityKey, grant.identity)
            if (attempt != joinAttempt) return
            val options = CallOptions(
                grant.url, grant.token, grant.room,
                sealingWord.ifEmpty { null },
                grant.forward?.let { IceServer(it.url, it.username, it.credential) },
            )
            val model = CallModel.join(options, deployment, grant.region, media)
            // Called off while the room was being entered: the room was
            // entered all the same, and is left at once.
            if (attempt != joinAttempt) {
                model.leave()
                return
            }
            model.relay = grant.relay
            model.holding = grant.holding
            // Where the choices are kept, before anything is chosen: the
            // original sound below remembers itself.
            model.memory = deviceMemory
            // Before the microphone is published, so it goes out the way it
            // was asked for the first time rather than being republished.
            if (originalSoundWanted) quietly { model.setOriginalSound(true) }
            if (attempt != joinAttempt) {
                model.leave()
                return
            }
            model.onMoved = { to -> scope.launch { rejoin(model, to) } }
            model.onEnded = { reason -> scope.launch { ended(reason, model) } }
            call = model
            landed = true
            joining = false
            room = grant.room
            lobby.remember(grant.room)
            // The invitation is kept for as long as this call is: a move to
            // another relay and "Join again" both join once more.
            problem = null
            screen = Screen.Call
            Log.i(TAG, "in call ${grant.room} as ${model.localIdentity} with ${model.people.size} others")
            // After joining rather than before, so somebody is in the room
            // the moment they join and their devices come up a beat later,
            // each on its own, never through the catch below (App.tsx
            // `onJoin`): a refused microphone is one person's problem with
            // their own device, not a room that would not open. Each only
            // while this is still the call.
            model.restoreDevices()
            if (startWithMicrophone && call === model) switchMicrophone(true)
            if (startWithCamera && call === model) quietly { model.setCamera(true) }
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: ProtocolException) {
            // Why they are not in the meeting, which stays until read; a join
            // that was called off has nothing to say.
            if (attempt != joinAttempt) return
            Log.w(TAG, "the join was refused: ${fault.message}")
            tell(explain(fault))
        } catch (fault: Exception) {
            if (attempt != joinAttempt) return
            Log.w(TAG, "the join failed: ${fault.javaClass.simpleName} ${fault.message}")
            tell(t("Could not join {room}.", mapOf("room" to room)))
        } finally {
            if (!landed) joining = false
            asking--
        }
    }

    /**
     * The server moved the call to another relay; the web rejoins at once with
     * the same passphrase and word, and lets the server choose, for this join
     * only: a machine chosen by hand stays chosen (App.tsx ~620 and ~676).
     *
     * Only for the call that is showing: a call already replaced or left that
     * is told of a move afterwards has nothing to come back to.
     */
    private suspend fun rejoin(moved: CallModel, to: String) {
        if (call !== moved) return
        movingTo = to
        try {
            call = null
            moved.leave()
            join(relay = "")
            // A join that failed left the screen on the call with no call in
            // it, which drew nothing at all. Back to the join screen, with the
            // reason, and the Join button that tries again.
            if (call == null && screen == Screen.Call) screen = Screen.Prejoin
        } finally {
            movingTo = null
        }
    }

    // MARK: The switches outside the call's own bar

    /** The microphone being switched by the join or a shortcut, while it is: the shortcut waits. */
    var switchingMicrophone by mutableStateOf(false)
        private set

    /** What a keyboard's microphone shortcut does. */
    suspend fun toggleMicrophone() {
        val call = call ?: return
        if (switchingMicrophone) return
        switchMicrophone(!call.microphoneOn)
    }

    private suspend fun switchMicrophone(on: Boolean) {
        val call = call ?: return
        if (switchingMicrophone) return
        switchingMicrophone = true
        try {
            // A refusal says so itself, in the call (CallModel `deviceFailed`).
            quietly { call.setMicrophone(on) }
        } finally {
            switchingMicrophone = false
        }
    }

    /**
     * Where a move is taking the meeting while there is no call to show it
     * over: between leaving the old room and entering the new one the room
     * view is gone, and the screen was blank for those seconds.
     */
    var movingTo by mutableStateOf<String?>(null)
        private set

    suspend fun leave() = end(Ending.Left)

    /**
     * The call ended without anybody here pressing leave: the reason is the
     * core's, the media server's disconnect reason by name. [model] is the
     * call that ended, which ends the screen only while it is the call
     * showing: one already replaced (put out as a duplicate by the join that
     * replaced it) would otherwise end the one still working.
     */
    suspend fun ended(reason: String, model: CallModel? = null) {
        if (model != null && model !== call) return
        // The core says the call ended while a leave pressed here is still
        // leaving it: that leave is the ending, and says it was left.
        if (model != null && model === leaving) return
        end(endingFor(reason))
    }

    /** The call being left, until it has gone. */
    private var leaving: CallModel? = null

    private suspend fun end(how: Ending) {
        ending = how
        // Left before it is let go, so that nothing can start another join
        // under the same identity while this one is still in the room.
        val model = call
        leaving = model
        try {
            model?.leave()
        } finally {
            leaving = null
        }
        // A call that replaced this one in the meantime is not ended by it.
        if (call === model) {
            call = null
            screen = Screen.Ended
        }
        // A relay that has just dropped a call is the one thing known about
        // the fleet; without forgetting the measurement it would go on being
        // named the fastest for minutes (App.tsx, the lost connection).
        forgetRelayMeasurement()
    }

    fun backToLobby() {
        if (call != null) return
        cancelJoin()
        screen = Screen.Lobby
        problem = null
        invite = null
        invitation = null
        stopWaitingTasks()
        arrangement = null
    }

    // MARK: An arranged meeting

    /**
     * Waiting.tsx, which is a join screen whose button waits. Nothing is
     * asked of the meeting but what it is until somebody presses: the web's
     * first draft joined the moment the host arrived, and somebody still
     * typing their name was joined with none.
     */
    data class Waiting(
        /** The button has been pressed and the door is being asked. */
        val asking: Boolean = false,
        val gone: Gone? = null,
        /** The last read failed; said, and tried again. */
        val unreachable: Boolean = false,
    ) {
        enum class Gone { Cancelled, Ended, NotComing }
    }

    private fun wait(token: String, deployment: Deployment) {
        stopWaitingTasks()
        meetingToken = token
        arrangement = null
        waiting = Waiting()
        problem = null
        screen = Screen.Waiting
        // Read on arrival so the screen can say what it is for, and again,
        // slowly, until it can (Waiting.tsx `RETRY_EVERY`).
        waitTask = scope.launch {
            while (isActive) {
                if (look() != null) return@launch
                if (waiting.gone != null) return@launch
                delay(30_000)
            }
        }
    }

    /** Reads the meeting once. */
    private suspend fun look(): Arrangement? {
        val deployment = deployment ?: return null
        val token = meetingToken ?: return null
        return try {
            val said = deployment.arranged(token)
            arrangement = said
            room = said.room
            waiting = waiting.copy(unreachable = false, gone = if (said.ended) Waiting.Gone.Ended else waiting.gone)
            said
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: ProtocolException.Refused) {
            if (fault.code == "no_such_meeting") waiting = waiting.copy(gone = Waiting.Gone.Cancelled)
            else waiting = waiting.copy(unreachable = true)
            null
        } catch (_: Exception) {
            waiting = waiting.copy(unreachable = true)
            null
        }
    }

    /** Whether the meeting has begun and carries the way in, so the button is the ordinary Join. */
    val arrangedReady: Boolean
        get() {
            val arrangement = arrangement ?: return false
            return arrangement.started && arrangement.invite != null
        }

    /** When the host may begin it: the server begins a meeting only from `from`. */
    val arrangedOpens: Instant?
        get() {
            val arrangement = arrangement ?: return null
            return instant(arrangement.from ?: arrangement.at)
        }

    /** The one button: join where it has begun, begin it where it is the host's, otherwise wait for the host. */
    suspend fun goArranged() {
        val arrangement = arrangement ?: return
        val invite = arrangement.invite
        if (arrangedReady && invite != null) {
            this.invite = invite
            join()
            return
        }
        // The host does not wait for the host.
        if (arrangement.mine) {
            join()
            return
        }
        askForArranged()
    }

    /** Asks at the web's pace until the meeting begins (then joins with the invitation it carries), ends, or the host is two hours late. */
    private fun askForArranged() {
        waitTask?.cancel()
        waiting = waiting.copy(asking = true, gone = null)
        waitTask = scope.launch {
            while (isActive) {
                val said = look()
                if (!isActive) return@launch
                if (said?.ended == true || waiting.gone != null) {
                    waiting = waiting.copy(asking = false)
                    return@launch
                }
                val invite = said?.invite
                if (said != null && said.started && invite != null) {
                    waiting = waiting.copy(asking = false)
                    this@AppModel.invite = invite
                    join()
                    return@launch
                }
                val at = said?.let { instant(it.at) }
                if (at != null && Instant.now().toEpochMilli() - at.toEpochMilli() > hostNotComingAfterMs().toLong()) {
                    waiting = waiting.copy(asking = false, gone = Waiting.Gone.NotComing)
                    return@launch
                }
                // A read that failed is not a reason to ask faster.
                val pause = said?.let { meetingAskEveryMs(it.at, Instant.now().toEpochMilli()).toLong() } ?: 30_000L
                delay(pause)
            }
        }
    }

    /** "Ask again", after the host was said not to be coming. */
    fun askAgain() = askForArranged()

    fun stopWaiting() {
        // The button that waits may already have turned into a join.
        cancelJoin()
        stopWaitingTasks()
        arrangement = null
        meetingToken = null
        waiting = Waiting()
        screen = Screen.Lobby
    }

    private fun stopWaitingTasks() {
        waitTask?.cancel()
        waitTask = null
        waiting = waiting.copy(asking = false)
    }

    // MARK: The door

    /**
     * Asks the room's host to let this person in, then waits. Under the name
     * the room will know them by: somebody signed in knocked as the guest
     * name this device last typed, and was let in as somebody else.
     */
    suspend fun knock() {
        val deployment = deployment ?: return
        val from = screen
        try {
            val ticket = deployment.knock(room, joiningName)
            // Somebody who went back while the knock was being sent is not
            // put at the door afterwards.
            if (screen != from) return
            screen = Screen.Knocking
            knockTask?.cancel()
            val giveUpAfter = knockGiveUpAfterMs().toLong()
            knockTask = scope.launch {
                val started = System.nanoTime()
                while (isActive) {
                    delay(knockAskEveryMs().toLong())
                    // A round that did not answer is not the end of the
                    // waiting: the person inside has not gone anywhere.
                    val answer = try {
                        deployment.atTheDoor(room, ticket.token)
                    } catch (fault: CancellationException) {
                        throw fault
                    } catch (_: Exception) {
                        null
                    }
                    if (answer != null) {
                        when (answer.state) {
                            DoorState.ADMITTED -> {
                                invite = answer.invite
                                join()
                                // Let in and then refused at the join: the
                                // reason is on the join screen, with its
                                // button, rather than under a door nobody is
                                // answering any more.
                                if (call == null && screen == Screen.Knocking) screen = Screen.Prejoin
                                return@launch
                            }
                            DoorState.REFUSED -> {
                                tell(t("Nobody let you in. Ask for a link."))
                                screen = Screen.Prejoin
                                return@launch
                            }
                            DoorState.KNOCKING -> {}
                        }
                    }
                    // knock.ts `GIVE_UP_AFTER`, counted whatever the rounds
                    // said: a server that never answered at all, the one case
                    // the door cannot end, left the person waiting for as
                    // long as the app was open.
                    if ((System.nanoTime() - started) / 1_000_000 > giveUpAfter) {
                        tell(t("Nobody let you in. Ask for a link."))
                        screen = Screen.Prejoin
                        return@launch
                    }
                }
            }
        } catch (fault: CancellationException) {
            throw fault
        } catch (_: Exception) {
            if (screen != from) return
            problem = t("That could not be sent.")
        }
    }

    fun stopKnocking() {
        knockTask?.cancel()
        knockTask = null
        // Let in a moment ago, and already joining.
        cancelJoin()
        screen = Screen.Prejoin
    }

    // MARK: Words

    /** What the server refused, in the reader's language (api.ts `explain`). */
    fun explain(error: Throwable): String = explain(error, room)

    /** Whether the person could ask to be let in instead (PreJoin.tsx ~521). */
    val canKnock: Boolean get() = canKnock(problem, room)

    companion object {
        const val NAME_KEY = "meet-live.name"
        const val RELAY_KEY = "meet-live.relay"
        /** Where the Apple apps kept a typed address; cleared wherever it is found. */
        const val SERVER_KEY = "tomoshibi.server"

        /**
         * The app's model for this process, made with what the device keeps.
         * [launch] is passed only by a debug build.
         */
        fun create(context: Context, launch: DebugLaunch?, keepChoices: (Boolean) -> Unit = {}): AppModel {
            val media = CallMedia(context, muted = BuildConfig.DEBUG && launch?.mute == true)
            return AppModel(
                SharedDefaults(context),
                KeystoreSecrets(context),
                media,
                if (BuildConfig.DEBUG) launch else null,
                keepChoices = keepChoices,
            // The media layer follows the model's calls from here on: the
            // service that keeps a call alive behind the screen, its
            // notification and the small window (CallMedia.follow). After
            // the model is made, never from inside it, so a test's model has
            // none of it.
            ).also(media::follow)
        }

        /** api.ts `explain`, for the room the person was trying. */
        fun explain(error: Throwable, room: String): String = when (error) {
            is ProtocolException.Network -> t("Could not reach the server.")
            is ProtocolException.Refused -> when (error.code) {
                "rate_limited" -> t("Too many attempts. Try again in a moment.")
                "invalid_room" -> t("Room names can only use lowercase letters, numbers and dashes.")
                "room_not_open" -> t("{room} isn't open. Ask the organiser for the link.", mapOf("room" to room))
                "not_invited" -> t("{room} is by invitation. Ask the organiser for a link.", mapOf("room" to room))
                "not_in_scope" -> t("Only members of {scope} can join {room}.", mapOf("room" to room, "scope" to scopeOf(room)))
                "relay_not_allowed" -> t("Access denied. That server is for administrators.")
                "blocked" -> t("This passphrase cannot join. Ask the organiser.")
                "not_yours" -> t("That name and passphrase do not go together.")
                "server_error" -> t("Something went wrong. Try again.")
                else -> t("Could not join {room}.", mapOf("room" to room))
            }
            else -> t("Something went wrong. Try again.")
        }

        /** Whether [problem] is one of the two refusals a knock answers. */
        fun canKnock(problem: String?, room: String): Boolean {
            if (problem == null) return false
            return problem == t("{room} is by invitation. Ask the organiser for a link.", mapOf("room" to room)) ||
                problem == t("Only members of {scope} can join {room}.", mapOf("room" to room, "scope" to scopeOf(room)))
        }

        /** Account.tsx `explainLink`. */
        fun explainLink(error: Throwable): String {
            val code = (error as? ProtocolException.Refused)?.code ?: return t("That link could not be made. Try again.")
            return when (code) {
                "not_yours" -> t("You can make links only into rooms of your own scopes.")
                "name_too_long" -> t("That name is too long.")
                "bad_time" -> t("That window cannot be used. It has to end after it starts, and after now.")
                "invalid_room" -> t("Room names can only use lowercase letters, numbers and dashes.")
                "rate_limited" -> t("Too many attempts. Try again in a moment.")
                "not_allowed" -> t("Guest links have not been opened to you.")
                else -> t("That link could not be made. Try again.")
            }
        }

        /** Account.tsx `Passphrase`'s `explain`. */
        fun explainPassphrase(error: Throwable): String {
            val code = (error as? ProtocolException.Refused)?.code ?: return t("That could not be changed. Try again.")
            return when (code) {
                "not_yours" -> t("That is not your current passphrase.")
                "passphrase_too_short" -> t("At least eight characters.")
                "passphrase_unchanged" -> t("That is the passphrase you already have.")
                "passphrase_in_use" -> t("Somebody else already uses that passphrase. Choose another.")
                else -> t("That could not be changed. Try again.")
            }
        }

        /** How a call that ended for the media server's [reason] reads to the person (App.tsx, the `ended` handler). */
        fun endingFor(reason: String): Ending = when {
            reason.contains("RoomDeleted") || reason.contains("RoomClosed") -> Ending.Closed
            reason.contains("ParticipantRemoved") -> Ending.Removed
            reason.contains("ClientInitiated") -> Ending.Left
            else -> Ending.Lost
        }

        /** An instant the server wrote, with or without fractions of a second, or null. */
        fun instant(text: String): Instant? = try {
            Instant.parse(text)
        } catch (_: Exception) {
            try {
                OffsetDateTime.parse(text).toInstant()
            } catch (_: Exception) {
                null
            }
        }
    }
}

/**
 * The parts of a link opened with the app that say where it goes: the origin
 * it is for, and the room, invitation and arranged meeting it carries
 * (`https://<deployment>/?invite=T#/room`, `?meeting=T#/room`, or the same
 * under the `tomoshibi:` scheme).
 */
data class OpenedLink(val origin: String, val room: String, val invite: String?, val meeting: String?) {
    /**
     * Whether this is a link into the deployment at [base]: compared without
     * the trailing slash the deployment's address always has and a link's
     * origin never does, and without a default port, which the deployment's
     * address never carries.
     */
    fun isFor(base: String): Boolean = origin == base.trimEnd('/')

    companion object {
        fun parse(link: String): OpenedLink? {
            val uri = try {
                URI(link.trim())
            } catch (_: Exception) {
                return null
            }
            var scheme = uri.scheme?.lowercase() ?: return null
            if (scheme == "tomoshibi") scheme = "https"
            val host = uri.host?.lowercase() ?: return null
            val port = uri.port.takeIf { it >= 0 && !(scheme == "https" && it == 443) && !(scheme == "http" && it == 80) }
            val origin = "$scheme://$host" + (port?.let { ":$it" } ?: "")
            val fragment = uri.fragment ?: ""
            val room = if (fragment.startsWith("/")) fragment.drop(1) else fragment
            val query = (uri.rawQuery ?: "").split("&").filter { it.isNotEmpty() }.map { item ->
                val at = item.indexOf('=')
                val key = decode(if (at < 0) item else item.substring(0, at))
                val value = if (at < 0) "" else decode(item.substring(at + 1))
                key to value
            }
            fun first(name: String) = query.firstOrNull { it.first == name }?.second
            return OpenedLink(origin, room, first("invite"), first("meeting"))
        }

        /** Percent-decoding as a URL's query items are read: a plus is a plus, not a space. */
        private fun decode(text: String): String = try {
            URLDecoder.decode(text.replace("+", "%2B"), Charsets.UTF_8)
        } catch (_: Exception) {
            text
        }
    }
}
