package app.tomoshibi.model

import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import app.tomoshibi.basics.Preferences
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Plain choices kept between launches, under the web's own storage names
 * where the web keeps the same thing (`meet-live.name`, `meet-live.relay`,
 * `meet-live.devices`...): the Apple apps' UserDefaults.
 *
 * Everything is a string or a boolean, as a browser's storage holds it, so
 * that what is kept reads the same as what the web keeps.
 */
interface Defaults {
    fun string(key: String): String?
    fun set(key: String, value: String?)
    fun bool(key: String): Boolean?
    fun set(key: String, value: Boolean)
    fun remove(key: String)
}

/** The app's preferences file, which the settings and the models share. */
class SharedDefaults(private val preferences: SharedPreferences) : Defaults {
    constructor(context: Context) : this(context.getSharedPreferences(Preferences.FILE, Context.MODE_PRIVATE))

    override fun string(key: String): String? = try {
        preferences.getString(key, null)
    } catch (_: ClassCastException) {
        null
    }

    override fun set(key: String, value: String?) {
        if (value == null) preferences.edit().remove(key).apply() else preferences.edit().putString(key, value).apply()
    }

    override fun bool(key: String): Boolean? = try {
        if (preferences.contains(key)) preferences.getBoolean(key, false) else null
    } catch (_: ClassCastException) {
        null
    }

    override fun set(key: String, value: Boolean) {
        preferences.edit().putBoolean(key, value).apply()
    }

    override fun remove(key: String) {
        preferences.edit().remove(key).apply()
    }
}

/**
 * Kept in memory only: what a rehearsal writes to, so that nothing a test does
 * outlives the process, and what the unit tests read back.
 */
class MemoryDefaults : Defaults {
    private val values = HashMap<String, Any>()

    override fun string(key: String): String? = values[key] as? String
    override fun set(key: String, value: String?) {
        if (value == null) values.remove(key) else values[key] = value
    }
    override fun bool(key: String): Boolean? = values[key] as? Boolean
    override fun set(key: String, value: Boolean) {
        values[key] = value
    }
    override fun remove(key: String) {
        values.remove(key)
    }
    fun has(key: String) = key in values
}

/**
 * Secrets kept between launches: the account's session cookie and the
 * passphrase typed at the join form (the Apple apps' Keychain, under the same
 * names, `account:<base>` and `passphrase:<base>`).
 *
 * The web keeps the passphrase in local storage in the clear, on purpose, so
 * that the field is filled next time. An app has a better home for it than a
 * browser does. The sealing word for an encrypted call is never kept
 * anywhere, here as on the web.
 */
interface Secrets {
    fun read(name: String): String?
    /** Nothing, or an empty value, forgets it. */
    fun write(name: String, value: String?)
}

/**
 * Each secret encrypted with an AES key that lives in the Android Keystore and
 * never leaves it, the ciphertext beside the app's other files.
 *
 * A secret that will not decrypt is forgotten rather than reported: the key
 * goes when the app's data is cleared or restored onto another device (a
 * backup carries the ciphertext and never the key), and the only remedy then
 * is to sign in again, which is what an absent cookie already asks for.
 */
class KeystoreSecrets(context: Context) : Secrets {
    private val store = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    override fun read(name: String): String? {
        val sealed = store.getString(name, null) ?: return null
        return try {
            val bytes = Base64.decode(sealed, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, IV))
            String(cipher.doFinal(bytes, IV, bytes.size - IV), Charsets.UTF_8)
        } catch (fault: Exception) {
            Log.w(TAG, "a kept secret could not be read and is forgotten: ${fault.javaClass.simpleName}")
            store.edit().remove(name).apply()
            null
        }
    }

    override fun write(name: String, value: String?) {
        if (value.isNullOrEmpty()) {
            store.edit().remove(name).apply()
            return
        }
        try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, key())
            val iv = cipher.iv
            check(iv.size == IV) { "unexpected IV length ${iv.size}" }
            val sealed = iv + cipher.doFinal(value.toByteArray(Charsets.UTF_8))
            store.edit().putString(name, Base64.encodeToString(sealed, Base64.NO_WRAP)).apply()
        } catch (fault: Exception) {
            // Not kept is the same as never typed: the field is empty next
            // time. Kept in the clear instead would be the one thing worse.
            Log.w(TAG, "a secret could not be kept: ${fault.javaClass.simpleName}")
            store.edit().remove(name).apply()
        }
    }

    private fun key(): SecretKey {
        val keystore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (keystore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "app.tomoshibi.secrets"
        const val FILE = "tomoshibi.secrets"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV = 12
    }
}

/** Secrets held for the life of the process: a rehearsal's and the tests'. */
class MemorySecrets : Secrets {
    private val values = HashMap<String, String>()
    override fun read(name: String): String? = values[name]
    override fun write(name: String, value: String?) {
        if (value.isNullOrEmpty()) values.remove(name) else values[name] = value
    }
}
