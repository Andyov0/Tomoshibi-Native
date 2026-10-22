package app.tomoshibi.model

import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.tomoshibi.basics.Pace
import app.tomoshibi.basics.t
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The app's own updates: looking for a newer build, saying so, and with the
 * person's agreement downloading and installing it (Updates.swift on the Mac).
 *
 * The asking and the fetching are the core's (core/protocol/src/api/updates.rs),
 * which compares the versions and refuses a file that is not the build; what
 * is here is when to ask, what to say, and the one rule the owner set above
 * the rest: nothing about an update interrupts a call. An offer is shown only
 * in the lobby; one found during a call waits for the lobby; a download is
 * held when the person leaves the lobby and carries on, where it stopped,
 * when they are back; nothing is installed anywhere but in the lobby, since
 * installing ends the app.
 *
 * Asked once at start and every six hours after. A check that fails is kept
 * quiet and asked again in ten minutes: a phone on a train fails checks all
 * day, and a notice for each would teach somebody to ignore the one that
 * matters. "Later" puts that version away until the app is next started; a
 * newer one is offered all the same. A check somebody asks for in the
 * settings always answers, "You have the latest version." included, and
 * brings back a version put away.
 *
 * A download's bar moves at one speed ([downloadBar]), and what follows a
 * download that arrived (asking to be allowed to install, the system's
 * confirmation) waits until it has been drawn full, a second or so at the
 * most: on a fast connection the build arrived while the bar stood partway,
 * and the system's window was put up over it as though the download had been
 * cut short. A Cancel in that second keeps the build that arrived, for
 * "Update" to install without downloading it again, rather than installing
 * it; so does leaving the lobby in it.
 *
 * The deployment can say how often to look ([checkEvery], its
 * update_check_hours), and switch the whole of it off ([allow], Remote's
 * app-updates): then nothing is looked for or offered and the settings say
 * nothing about updates, but a download or an install already under way is
 * seen through, since one stopped half way is a download somebody agreed to
 * and lost.
 *
 * Kept apart from the core and the platform (UpdateSource, UpdateInstaller)
 * so that the rules can be tested on the JVM, where neither can be loaded.
 * Every method is for the main thread.
 */
class Updates(
    private val source: UpdateSource,
    private val installer: UpdateInstaller,
    /** This build's version, dotted as the core and the download page write it. */
    val running: String,
    private val scope: CoroutineScope,
    /**
     * The download's bar, which the banner and the About page draw as it is:
     * moved by the display's frames in the app (Pace.onTheDisplay), by hand or
     * not at all in the tests.
     */
    val downloadBar: Pace,
    every: Long = EVERY,
    private val retry: Long = RETRY,
) {
    /** How long between looks; the deployment can say ([checkEvery]). */
    private var every = every
    /** Where an update has got to. */
    sealed interface Step {
        data object None : Step

        /** A newer build, and what went wrong with the last try at it, if anything did. */
        data class Offered(val offer: Offer, val problem: String? = null) : Step

        data class Downloading(val offer: Offer, val received: Long, val total: Long) : Step

        /** A download the person left the lobby during: stopped, kept, and carried on when they are back. */
        data class Held(val offer: Offer, val received: Long, val total: Long) : Step

        /**
         * The build is here and checked, and Android wants this app allowed
         * to install apps first. [asked] once the system's page has been
         * opened; [refused] once they came back from it without allowing it.
         */
        data class Permission(val offer: Offer, val file: File, val asked: Boolean = false, val refused: Boolean = false) : Step

        /** Handed to the system's installer, which asks the person to confirm. */
        data class Installing(val offer: Offer) : Step
    }

    /** What a check somebody asked for found, for the settings to say. */
    enum class Said { Checking, Current, Failed }

    var step by mutableStateOf<Step>(Step.None)
        private set

    /** The answer to the last check asked for in the settings; null where none was asked or it found a build. */
    var said by mutableStateOf<Said?>(null)
        private set

    /** Versions put away with "Later", for this run of the app only. */
    private var later by mutableStateOf(emptySet<String>())

    /** In the lobby, where an update may be shown, downloaded and installed. */
    private var free by mutableStateOf(false)

    /** Whether updates are looked for and offered at all ([allow]). */
    var allowed by mutableStateOf(true)
        private set

    /**
     * Whether the banner is up: something to say, in the lobby, and not put
     * away; with updates switched off, only a download or an install that
     * was already under way.
     */
    val shown: Boolean
        get() {
            if (!free) return false
            return when (val now = step) {
                Step.None -> false
                is Step.Offered -> allowed && now.offer.version !in later
                is Step.Held -> false
                else -> true
            }
        }

    /** Whether the settings say anything about updates: unless they are switched off, and then still while one is under way, for its progress and its Cancel. */
    val speaks: Boolean
        get() = allowed || when (step) {
            is Step.Downloading, is Step.Held, is Step.Permission, is Step.Installing -> true
            else -> false
        }

    /** The build being offered, downloaded or installed, whatever the step. */
    val offer: Offer?
        get() = when (val now = step) {
            Step.None -> null
            is Step.Offered -> now.offer
            is Step.Downloading -> now.offer
            is Step.Held -> now.offer
            is Step.Permission -> now.offer
            is Step.Installing -> now.offer
        }

    private var started = false
    private var schedule: Job? = null
    private var transfer: Transfer? = null
    private var transferJob: Job? = null
    /** Set when the transfer is stopped because the person left the lobby, not because they cancelled. */
    private var holding = false
    /** Set when the person left the lobby during an install, which is then given up without a word. */
    private var walkedAway = false
    /** A build that arrived and was not installed (the confirmation declined), kept for a second try. */
    private var ready: Pair<String, File>? = null
    /** A build that has arrived while its bar is still being drawn full, kept by Cancel in that second as [ready]. */
    private var filling: Pair<String, File>? = null
    /** Until a check has answered: whether a build kept by the run before ([UpdateSource.keep]) is still to be looked at. */
    private var carrying = false

    /** Starts asking: now, and every six hours after. Once for the life of the model. */
    fun start() {
        if (started) return
        started = true
        // Whatever an earlier run downloaded was installed, failed, or was
        // left behind; none of it is trusted to be the build offered now.
        // The one exception is a build the person was sent to allow
        // installing (allow), which the first check looks at again.
        runCatching { source.clear(keepingKept = true) }
        carrying = true
        if (allowed) arrange(lookingFirst = true)
    }

    /**
     * The looking, every [every], from now ([lookingFirst]) or from one wait
     * from now. Any schedule before is given up, a wait in it included.
     */
    private fun arrange(lookingFirst: Boolean) {
        schedule?.cancel()
        schedule = scope.launch {
            if (!lookingFirst) delay(every)
            while (true) {
                val answered = check(asked = false)
                delay(if (answered) every else retry)
            }
        }
    }

    /**
     * Whether to look for and offer updates (the deployment's app-updates).
     * Switched off, the looking stops and an offer is taken off the screen;
     * a download or an install under way goes on to its end. Switched on
     * again, the app looks at once, since the offer it missed may be days
     * old.
     */
    fun allow(on: Boolean) {
        if (on == allowed) return
        allowed = on
        if (!started) return
        if (on) {
            arrange(lookingFirst = true)
        } else {
            schedule?.cancel()
            schedule = null
        }
    }

    /**
     * How long between looks, in milliseconds (the deployment's
     * update_check_hours, the app's six hours where it says none). A change
     * counts from now: the next look is one new wait away, not at once,
     * since it is said at every start, moments after the first look.
     */
    fun checkEvery(interval: Long) {
        if (interval == every) return
        every = interval
        if (started && allowed) arrange(lookingFirst = false)
    }

    /** Told whenever the screen changes: [inLobby] is the only place an update is shown or worked on. */
    fun lobby(inLobby: Boolean) {
        if (inLobby == free) return
        free = inLobby
        val now = step
        if (!inLobby && now is Step.Downloading && keepWhatArrived(now.offer)) {
            // Arrived as the person left, while its bar was filling: kept for
            // Update back in the lobby, as one arriving a moment later is.
        } else if (!inLobby && now is Step.Downloading) {
            holding = true
            step = Step.Held(now.offer, now.received, now.total)
            transfer?.cancel()
        } else if (!inLobby && now is Step.Installing) {
            // The system's confirmation is given up rather than left to be
            // answered, or to be put up when the app is next in front, in
            // whatever the person went on to: a call is ended by the
            // install. The build is kept for Update back in the lobby.
            walkedAway = true
            installer.stop()
        } else if (inLobby && now is Step.Held) {
            download(now.offer)
        } else if (inLobby && now is Step.Permission && now.asked && installer.allowed) {
            // Allowed in a run of the app before this one (carriedOver),
            // which reached the lobby only after the check had answered.
            install(now.offer, now.file)
        }
    }

    /** The settings' "Check for updates". */
    fun checkNow() {
        if (!allowed || said == Said.Checking) return
        said = Said.Checking
        scope.launch { check(asked = true) }
    }

    /** "Update": downloads the build offered, or installs it if it is already here. */
    fun accept() {
        val now = step as? Step.Offered ?: return
        if (!free || !allowed) return
        val kept = ready
        if (kept != null && kept.first == now.offer.version && kept.second.exists()) {
            arrived(now.offer, kept.second)
        } else {
            download(now.offer)
        }
    }

    /** "Later": puts this version away until the app is next started, and lets go of anything it left. */
    fun later() {
        val offer = offer ?: return
        when (step) {
            is Step.Downloading, is Step.Held, is Step.Installing -> return
            is Step.Permission -> forget()
            else -> {}
        }
        if (ready?.first == offer.version) forget()
        later = later + offer.version
        step = Step.Offered(offer)
    }

    /**
     * "Cancel", on a download. What arrived is kept, so "Update" again
     * carries on from there; all of it, where it had all arrived and only its
     * bar was still filling, which "Update" then installs (keepWhatArrived).
     */
    fun cancel() {
        val now = step as? Step.Downloading ?: return
        if (keepWhatArrived(now.offer)) return
        holding = false
        transfer?.cancel()
    }

    /**
     * Where all of [offer] has arrived and its bar is still filling: the
     * build kept for "Update" and the offer made again, and nothing
     * installed. The person said to stop, and a download they stopped is not
     * to go on to the system's confirmation a moment later.
     */
    private fun keepWhatArrived(offer: Offer): Boolean {
        val arrived = filling?.takeIf { it.first == offer.version } ?: return false
        ready = arrived
        filling = null
        transfer = null
        holding = false
        downloadBar.reset()
        step = Step.Offered(offer)
        return true
    }

    /** "Cancel" while the system asks for confirmation. */
    fun stopInstalling() {
        if (step is Step.Installing) installer.stop()
    }

    /** "Continue" on the explanation: opens the system's page for allowing this app to install apps. */
    fun allow() {
        val now = step as? Step.Permission ?: return
        step = now.copy(asked = true, refused = false)
        // Android 11 ends the app when the permission is given (the system
        // mounts its storage again), and a phone short of memory may end it
        // while the page is up: the app the person comes back to is a new
        // run, which would otherwise have deleted the build at its start and
        // offered it to be downloaded all over again.
        runCatching { source.keep(now.file) }
        installer.askToBeAllowed()
    }

    /**
     * The app is in front again: where the person was sent to the system's
     * page, whether they allowed it decides between installing and saying
     * that it was not allowed.
     */
    fun resumed() {
        val now = step as? Step.Permission ?: return
        if (!now.asked) return
        // Back in the same run: the build is not needed by another, and kept
        // for one it would be installed at its start without being asked.
        runCatching { source.keep(null) }
        if (installer.allowed) {
            install(now.offer, now.file)
        } else {
            step = now.copy(asked = false, refused = true)
        }
    }

    private suspend fun check(asked: Boolean): Boolean {
        val found = try {
            source.latest(running)
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            Log.i(TAG, "update check failed: ${fault.javaClass.simpleName}")
            if (asked) said = Said.Failed
            return false
        }
        if (found == null) {
            if (asked) said = Said.Current
            // A build offered and since taken back is not offered any more.
            if (step is Step.Offered) step = Step.None
            if (carrying) {
                carrying = false
                runCatching { source.clear() }
            }
            return true
        }
        if (asked) {
            said = null
            later = later - found.version
        }
        if (carrying) {
            carrying = false
            if (carriedOver(found)) return true
        }
        when (val now = step) {
            Step.None -> step = Step.Offered(found)
            is Step.Offered -> if (now.offer.version != found.version) step = Step.Offered(found)
            // Whatever is under way goes on; a newer build is offered once it is over.
            else -> {}
        }
        return true
    }

    /**
     * The person was sent to allow installs in the run of the app before
     * this one, which ended there: the build it kept, if it is the one on
     * offer now, is taken up where that run left it, as though they had
     * just come back to it (resumed). Anything else it kept is let go.
     */
    private suspend fun carriedOver(found: Offer): Boolean {
        val file = try {
            source.kept(found)
        } catch (fault: CancellationException) {
            throw fault
        } catch (fault: Exception) {
            null
        }
        val now = step
        // Nothing is under way before a check has answered; were something,
        // its files would be left alone and only the kept one let go.
        if (now != Step.None && now !is Step.Offered) {
            runCatching { source.keep(null) }
            return false
        }
        if (file == null) {
            runCatching { source.clear() }
            return false
        }
        // Taken up once: a build kept past this run as well would be
        // installed at the next start whatever the person said here.
        runCatching { source.keep(null) }
        if (installer.allowed) {
            step = Step.Permission(found, file, asked = true)
            if (free) install(found, file)
        } else {
            step = Step.Permission(found, file, refused = true)
        }
        return true
    }

    private fun download(offer: Offer) {
        // The one before is waited for rather than cancelled. Cancelling the
        // coroutine lets go of the core's future without stopping the task
        // the core runs it on, which would go on appending to the same part
        // as the new download did; a transfer is stopped by its own cancel,
        // and its coroutine ends when the core says it has.
        val previous = transferJob
        this.transfer?.cancel()
        val transfer = source.download(offer)
        this.transfer = transfer
        holding = false
        // A download carried on from where it was held keeps its bar, and
        // what it had (the core goes on from there); any other starts empty.
        val held = (step as? Step.Held)?.takeIf { it.offer.version == offer.version }
        if (held == null) downloadBar.reset()
        step = if (held != null) Step.Downloading(offer, held.received, held.total) else Step.Downloading(offer, 0, offer.size)
        transferJob = scope.launch {
            previous?.join()
            val outcome = try {
                transfer.run { received, total ->
                    scope.launch {
                        val now = step
                        if (now is Step.Downloading && now.offer.version == offer.version && this@Updates.transfer === transfer) {
                            step = now.copy(received = received, total = total)
                            downloadBar.follow(if (total > 0) received.toFloat() / total else 0f)
                        }
                    }
                }
                null
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Exception) {
                fault
            }
            if (this@Updates.transfer !== transfer) return@launch
            if (outcome == null) {
                // All of it, checked: drawn whole, and what follows waits for
                // the bar to get there.
                (step as? Step.Downloading)?.takeIf { it.offer.version == offer.version }?.let { step = it.copy(received = it.total) }
                downloadBar.follow(1f)
                filling = offer.version to transfer.file
                downloadBar.filled()
                // Cancelled, or the lobby left, while it filled (keepWhatArrived).
                if (this@Updates.transfer !== transfer) return@launch
                filling = null
            }
            this@Updates.transfer = null
            when (outcome) {
                null -> if (free) arrived(offer, transfer.file) else {
                    // Arrived in the moment the person left the lobby: kept,
                    // and installed only once they are back and say so.
                    ready = offer.version to transfer.file
                    step = Step.Offered(offer)
                }
                is Failure.Cancelled -> {
                    val now = step
                    if (holding && now is Step.Held) return@launch
                    step = Step.Offered(offer)
                }
                is Failure.Superseded -> {
                    // The deployment has a newer build than this one now; ask
                    // what it is rather than offer one nobody is handing out.
                    step = Step.None
                    scope.launch { check(asked = false) }
                }
                else -> step = Step.Offered(offer, explain(outcome))
            }
        }
    }

    private fun arrived(offer: Offer, file: File) {
        if (installer.allowed) {
            install(offer, file)
        } else {
            step = Step.Permission(offer, file)
        }
    }

    private fun install(offer: Offer, file: File) {
        if (!free) {
            ready = offer.version to file
            step = Step.Offered(offer)
            return
        }
        step = Step.Installing(offer)
        walkedAway = false
        scope.launch {
            val installed = try {
                installer.install(file)
            } catch (fault: CancellationException) {
                throw fault
            } catch (fault: Exception) {
                Log.w(TAG, "the installer would not start: ${fault.javaClass.simpleName}")
                Installed.Failed(t("The update could not be installed."))
            }
            when (installed) {
                // The system replaces the app and ends this process as it
                // does; were this ever reached, nothing is left to say.
                Installed.Done -> {
                    forget()
                    step = Step.None
                }
                // Declined at the system's confirmation: the build is kept,
                // so that "Update" again asks again without downloading it.
                Installed.Declined -> {
                    ready = offer.version to file
                    step = if (walkedAway) Step.Offered(offer) else Step.Offered(offer, t("The update was not installed."))
                }
                is Installed.Failed -> {
                    forget()
                    step = Step.Offered(offer, installed.sentence)
                }
            }
        }
    }

    /** Lets go of a build that arrived: once it is installed, has failed, or has been put away. */
    private fun forget() {
        ready = null
        runCatching { source.clear() }
    }

    private fun explain(fault: Exception): String = when (fault) {
        is Failure.Broken -> t("The download stopped. Check your connection and try again.")
        is Failure.Damaged -> t("The download was damaged and has been deleted. Try again.")
        is Failure.Storage -> t("The update could not be saved on this device.")
        else -> t("The update could not be downloaded. Try again later.")
    }

    companion object {
        private const val TAG = "Tomoshibi"
        const val EVERY = 6 * 60 * 60 * 1000L
        const val RETRY = 10 * 60 * 1000L
    }
}

/** A newer build, as the app speaks of it. */
class Offer(
    /** Dotted, as the deployment and the update check write it: 0.1.4.2 for 0.1.4's second build. */
    val version: String,
    /** Bytes. */
    val size: Long,
    private val notesIn: (String) -> String? = { null },
    /** What the source needs to download it again; nobody else reads it. */
    internal val handle: Any? = null,
    /**
     * The version as people are told it, "0.1.4 (2)" for 0.1.4.2: what every
     * banner and line that names the build says. Written by the core's
     * version_label where the offer is made (CoreUpdateSource), since the
     * views are drawn in tests where the core cannot be loaded; [version]
     * itself, a fourth part and all, is for comparing and naming files.
     */
    val label: String = version,
) {
    /** What changed, for somebody reading [language] ("zh-Hans"); null where there is nothing to say in a language they read. */
    fun notes(language: String): String? = notesIn(language)

    override fun equals(other: Any?): Boolean = other is Offer && other.version == version && other.size == size
    override fun hashCode(): Int = version.hashCode() * 31 + size.hashCode()
    override fun toString(): String = "Offer($version, $size)"
}

/** Where builds come from: the core, or a test's stand-in. */
interface UpdateSource {
    /** The newer build there is, null where this one is the newest; throws where the deployment could not be asked. */
    suspend fun latest(running: String): Offer?

    /** A download of [offer], which starts when it is run. */
    fun download(offer: Offer): Transfer

    /** Removes every build and part of one that was downloaded, and the build [keep] kept unless [keepingKept]. */
    fun clear(keepingKept: Boolean = false)

    /**
     * Keeps [file], a build that arrived, past the end of this run of the
     * app for the next one to find ([kept]); null lets go of it, to go with
     * everything else at the next start.
     */
    fun keep(file: File?)

    /** The build kept by [keep], if it is still there and is [offer]'s, byte for byte; null otherwise. */
    suspend fun kept(offer: Offer): File?
}

/** One download, run once. */
interface Transfer {
    /** Where the build is once [run] returns. */
    val file: File

    /** Downloads the build, throwing a [Failure] where it does not arrive whole. */
    suspend fun run(progress: (received: Long, total: Long) -> Unit)

    fun cancel()
}

/** Why a download did not finish (the core's UpdateError, as the app tells them apart). */
sealed class Failure(message: String) : Exception(message) {
    class Cancelled : Failure("cancelled")
    class Broken(detail: String) : Failure(detail)
    class Damaged(detail: String) : Failure(detail)
    class Superseded : Failure("superseded")
    class Storage(detail: String) : Failure(detail)
    class Refused(val code: String) : Failure(code)
    class Other(detail: String) : Failure(detail)
}

/** Where a build is installed: the system's package installer, or a test's stand-in. */
interface UpdateInstaller {
    /** Whether this app may install apps ("Install unknown apps"). */
    val allowed: Boolean

    /** Opens the system's page where the person can allow it. */
    fun askToBeAllowed()

    /** Hands the build to the system, which asks the person to confirm; returns once it has an answer. */
    suspend fun install(file: File): Installed

    /**
     * Gives up on the install waiting for an answer, which then returns
     * [Installed.Declined]: a confirmation put behind another app by Home
     * stays open where the person may never find it, and the banner would
     * otherwise say "confirm it" for ever.
     */
    fun stop()
}

sealed interface Installed {
    data object Done : Installed
    /** The person said no at the system's confirmation. */
    data object Declined : Installed
    data class Failed(val sentence: String) : Installed
}
