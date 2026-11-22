package uk.co.aylett.quiddity.contacts

import android.Manifest
import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONArray

/**
 * The phone's side of `quiddity-contacts`: the shell calls these and the
 * page never does. Commands return objects, since that is what a result is.
 */
@TauriPlugin(
    permissions = [
        Permission(
            strings = [Manifest.permission.READ_CONTACTS, Manifest.permission.WRITE_CONTACTS],
            alias = ALIAS,
        ),
    ],
)
class ContactsPlugin(private val activity: Activity) : Plugin(activity) {
    private val book = PhoneBook(activity.applicationContext)

    private fun granted(): Boolean = getPermissionState(ALIAS)?.toString() == "granted"

    private fun status(): JSObject = JSObject()
        .put("available", true)
        .put("permission", getPermissionState(ALIAS)?.toString() ?: "prompt")
        .put("enabled", granted() && book.enabled())

    @Command
    fun status(invoke: Invoke) {
        invoke.resolve(status())
    }

    /** Asks for permission if it hasn't been given, then makes the account. */
    @Command
    fun enable(invoke: Invoke) {
        if (granted()) {
            makeAccount(invoke)
        } else {
            requestPermissionForAlias(ALIAS, invoke, "enableAfterPermission")
        }
    }

    @PermissionCallback
    fun enableAfterPermission(invoke: Invoke) {
        if (granted()) {
            makeAccount(invoke)
        } else {
            invoke.reject("Quiddity wasn't allowed to use contacts")
        }
    }

    private fun makeAccount(invoke: Invoke) {
        try {
            book.enable()
            invoke.resolve(status())
        } catch (e: Exception) {
            invoke.reject(e.message ?: "couldn't turn contacts on", e)
        }
    }

    @Command
    fun disable(invoke: Invoke) {
        try {
            book.disable()
            invoke.resolve(status())
        } catch (e: Exception) {
            invoke.reject(e.message ?: "couldn't turn contacts off", e)
        }
    }

    @Command
    fun edited(invoke: Invoke) = rows(invoke) { book.edited() }

    @Command
    fun held(invoke: Invoke) = rows(invoke) { book.held() }

    @Command
    fun put(invoke: Invoke) {
        try {
            val args = invoke.getArgs()
            val row = if (args.isNull("row")) null else args.getString("row")
            val contact = args.getJSONObject("contact")
            invoke.resolve(JSObject().put("row", book.put(row, contact)))
        } catch (e: Exception) {
            invoke.reject(e.message ?: "couldn't put the contact", e)
        }
    }

    @Command
    fun remove(invoke: Invoke) {
        try {
            book.remove(invoke.getArgs().getString("row"))
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject(e.message ?: "couldn't remove the contact", e)
        }
    }

    /** Resolves with `{rows: [...]}`. */
    private fun rows(invoke: Invoke, read: () -> JSONArray) {
        try {
            invoke.resolve(JSObject().put("rows", read()))
        } catch (e: Exception) {
            invoke.reject(e.message ?: "couldn't read the contacts", e)
        }
    }

    companion object {
        const val ALIAS = "contacts"
    }
}
