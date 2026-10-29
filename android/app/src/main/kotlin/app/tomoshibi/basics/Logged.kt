package app.tomoshibi.basics

import app.tomoshibi.BuildConfig
import java.security.MessageDigest
import java.security.SecureRandom

/**
 * Who and where, as the log may say them. A debug build says them as they
 * are, for the tests and whoever reads its log; a release build says a short
 * mark of each instead, the same mark for the same thing while the app runs,
 * so that a log still shows one person arriving and leaving without saying
 * who. A participant's identity carries the name they chose and the trip
 * that proves it, stable from one visit to the next, and the room is the
 * room: a release build wrote both to logcat, where a bug report, adb and a
 * phone maker's log collection read them (the health check of 2026-10-29).
 * The Windows app writes the same lines only in a debug build, and Apple's
 * log keeps interpolated values private.
 *
 * The mark is salted afresh by every process, so it cannot be worked back
 * from a list of likely room names, nor matched across two runs.
 */
object Logged {
    private val salt = ByteArray(16).also { SecureRandom().nextBytes(it) }

    /** [text] in a debug build, its mark in a release one. */
    fun private(text: String, debug: Boolean = BuildConfig.DEBUG): String = if (debug) text else mark(text)

    /** Eight hexadecimal digits of the salted text's SHA-256, after a hash sign. */
    fun mark(text: String): String {
        val digest = MessageDigest.getInstance("SHA-256")
        digest.update(salt)
        val sum = digest.digest(text.toByteArray())
        return "#" + sum.take(4).joinToString("") { "%02x".format(it) }
    }
}
