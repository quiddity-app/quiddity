package uk.co.aylett.quiddity.contacts

import android.accounts.Account
import android.accounts.AccountManager
import android.content.ContentProviderOperation
import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.provider.ContactsContract
import android.provider.ContactsContract.CommonDataKinds.Email
import android.provider.ContactsContract.CommonDataKinds.Event
import android.provider.ContactsContract.CommonDataKinds.Nickname
import android.provider.ContactsContract.CommonDataKinds.Note
import android.provider.ContactsContract.CommonDataKinds.Organization
import android.provider.ContactsContract.CommonDataKinds.Phone
import android.provider.ContactsContract.CommonDataKinds.StructuredName
import android.provider.ContactsContract.CommonDataKinds.StructuredPostal
import android.provider.ContactsContract.CommonDataKinds.Website
import android.provider.ContactsContract.Data
import android.provider.ContactsContract.RawContacts
import android.provider.ContactsContract.Settings
import org.json.JSONArray
import org.json.JSONObject

/**
 * Quiddity's contacts in the system's, under an account of their own.
 *
 * A contact is a raw contact of that account, whose `SOURCE_ID` is the
 * entity's ID, and whose `SYNC1` is the contact as it was handed over, as
 * JSON, so that an edit can be told from what was already there. A contact
 * the user edited, made or deleted is *dirty*, which the system sets and we
 * clear when we put it back. Contacts are JSON here as they are in
 * `quiddity-contacts`: `{id, name, entries: [{field, label, value}]}`.
 *
 * Everything is written as a sync adapter, which is how the system is told
 * the change isn't the user's: it doesn't mark the row dirty, and a delete
 * really deletes.
 */
class PhoneBook(private val context: Context) {
    private val resolver = context.contentResolver
    private val account = Account(ACCOUNT_NAME, ACCOUNT_TYPE)

    fun enabled(): Boolean =
        AccountManager.get(context).getAccountsByType(ACCOUNT_TYPE).isNotEmpty()

    /** Makes the account, and says its contacts are to be shown. */
    fun enable() {
        AccountManager.get(context).addAccountExplicitly(account, null, null)
        android.content.ContentResolver.setIsSyncable(account, ContactsContract.AUTHORITY, 1)
        android.content.ContentResolver.setSyncAutomatically(account, ContactsContract.AUTHORITY, true)
        // Without this a contact in no group isn't listed, and ours aren't in any.
        val visible = ContentValues().apply {
            put(Settings.ACCOUNT_NAME, ACCOUNT_NAME)
            put(Settings.ACCOUNT_TYPE, ACCOUNT_TYPE)
            put(Settings.UNGROUPED_VISIBLE, 1)
            put(Settings.SHOULD_SYNC, 1)
        }
        try {
            resolver.insert(asSyncAdapter(Settings.CONTENT_URI), visible)
        } catch (e: Exception) {
            // Already there from an earlier time.
            resolver.update(
                asSyncAdapter(Settings.CONTENT_URI),
                visible,
                "${Settings.ACCOUNT_NAME}=? AND ${Settings.ACCOUNT_TYPE}=?",
                arrayOf(ACCOUNT_NAME, ACCOUNT_TYPE),
            )
        }
    }

    /** Removes the account, and with it every contact kept under it. */
    fun disable() {
        AccountManager.get(context).removeAccountExplicitly(account)
    }

    /** The contacts that were edited, made or deleted since they were put. */
    fun edited(): JSONArray {
        val rows = JSONArray()
        rawContacts("(${RawContacts.DIRTY}=1 OR ${RawContacts.DELETED}=1)") { raw ->
            val contact = if (raw.deleted) {
                emptyContact(raw.sourceId)
            } else {
                readContact(raw.id, raw.sourceId)
            }
            rows.put(
                JSONObject()
                    .put("row", raw.id.toString())
                    .put("id", raw.sourceId ?: JSONObject.NULL)
                    .put("base", raw.base ?: JSONObject.NULL)
                    .put("contact", contact)
                    .put("deleted", raw.deleted),
            )
        }
        return rows
    }

    /** Every contact that was handed over, as it was. */
    fun held(): JSONArray {
        val rows = JSONArray()
        rawContacts("${RawContacts.DELETED}=0") { raw ->
            val base = raw.base ?: return@rawContacts
            rows.put(JSONObject().put("row", raw.id.toString()).put("contact", base))
        }
        return rows
    }

    /**
     * Makes a contact, or replaces the one in `row`, with `contact`, and says
     * it was handed over as that. Only the kinds of data Quiddity has are
     * replaced: a photo or a ringtone the user gave it stays.
     */
    fun put(row: String?, contact: JSONObject): String {
        val ops = ArrayList<ContentProviderOperation>()
        val rawId = row?.toLong()
        val sourceId = contact.optString("id").takeIf { it.isNotEmpty() }
        if (rawId == null) {
            ops.add(
                ContentProviderOperation.newInsert(asSyncAdapter(RawContacts.CONTENT_URI))
                    .withValue(RawContacts.ACCOUNT_NAME, ACCOUNT_NAME)
                    .withValue(RawContacts.ACCOUNT_TYPE, ACCOUNT_TYPE)
                    .withValue(RawContacts.SOURCE_ID, sourceId)
                    .withValue(RawContacts.SYNC1, contact.toString())
                    .build(),
            )
        } else {
            val managed = MANAGED.joinToString(",") { "?" }
            ops.add(
                ContentProviderOperation.newDelete(asSyncAdapter(Data.CONTENT_URI))
                    .withSelection(
                        "${Data.RAW_CONTACT_ID}=? AND ${Data.MIMETYPE} IN ($managed)",
                        arrayOf(rawId.toString()) + MANAGED,
                    )
                    .build(),
            )
            ops.add(
                ContentProviderOperation.newUpdate(asSyncAdapter(RawContacts.CONTENT_URI))
                    .withSelection("${RawContacts._ID}=?", arrayOf(rawId.toString()))
                    .withValue(RawContacts.SOURCE_ID, sourceId)
                    .withValue(RawContacts.SYNC1, contact.toString())
                    .withValue(RawContacts.DIRTY, 0)
                    .withValue(RawContacts.DELETED, 0)
                    .build(),
            )
        }
        for (values in dataRows(contact)) {
            val insert = ContentProviderOperation.newInsert(asSyncAdapter(Data.CONTENT_URI))
            if (rawId == null) {
                insert.withValueBackReference(Data.RAW_CONTACT_ID, 0)
            } else {
                insert.withValue(Data.RAW_CONTACT_ID, rawId)
            }
            ops.add(insert.withValues(values).build())
        }
        val results = resolver.applyBatch(ContactsContract.AUTHORITY, ops)
        return (rawId ?: ContentUris.parseId(results[0].uri!!)).toString()
    }

    /** Deletes a contact, for good. */
    fun remove(row: String) {
        resolver.delete(
            asSyncAdapter(RawContacts.CONTENT_URI),
            "${RawContacts._ID}=?",
            arrayOf(row),
        )
    }

    private class Raw(val id: Long, val sourceId: String?, val deleted: Boolean, val base: JSONObject?)

    private fun rawContacts(where: String, each: (Raw) -> Unit) {
        val cursor = resolver.query(
            asSyncAdapter(RawContacts.CONTENT_URI),
            arrayOf(RawContacts._ID, RawContacts.SOURCE_ID, RawContacts.DELETED, RawContacts.SYNC1),
            "${RawContacts.ACCOUNT_TYPE}=? AND ${RawContacts.ACCOUNT_NAME}=? AND $where",
            arrayOf(ACCOUNT_TYPE, ACCOUNT_NAME),
            null,
        ) ?: return
        // Read them all before calling back, which writes.
        val raws = ArrayList<Raw>()
        cursor.use {
            while (it.moveToNext()) {
                val base = it.getString(3)?.let { json ->
                    try {
                        JSONObject(json)
                    } catch (e: org.json.JSONException) {
                        null
                    }
                }
                raws.add(Raw(it.getLong(0), it.getString(1), it.getInt(2) != 0, base))
            }
        }
        raws.forEach(each)
    }

    private fun emptyContact(id: String?) =
        JSONObject().put("id", id ?: "").put("name", "").put("entries", JSONArray())

    /** What the user sees of one raw contact, as a contact. */
    private fun readContact(rawId: Long, sourceId: String?): JSONObject {
        var name = ""
        val entries = JSONArray()
        val titles = ArrayList<String>()
        val cursor = resolver.query(
            asSyncAdapter(Data.CONTENT_URI),
            arrayOf(Data.MIMETYPE, Data.DATA1, Data.DATA2, Data.DATA3, Data.DATA4),
            "${Data.RAW_CONTACT_ID}=?",
            arrayOf(rawId.toString()),
            "${Data._ID} ASC",
        )
        cursor?.use {
            while (it.moveToNext()) {
                val data1 = it.getString(1)?.trim().orEmpty()
                // DATA2 is a kind for some data and a name for others.
                val kind = { it.getInt(2) }
                val custom = { it.getString(3) }
                when (it.getString(0)) {
                    StructuredName.CONTENT_ITEM_TYPE -> if (name.isEmpty()) name = data1
                    Phone.CONTENT_ITEM_TYPE ->
                        entry(entries, "phone", Kinds.phones.labelOf(kind(), custom()), data1)
                    Email.CONTENT_ITEM_TYPE ->
                        entry(entries, "email", Kinds.emails.labelOf(kind(), custom()), data1)
                    StructuredPostal.CONTENT_ITEM_TYPE ->
                        entry(entries, "address", Kinds.addresses.labelOf(kind(), custom()), data1)
                    Organization.CONTENT_ITEM_TYPE -> {
                        entry(entries, "organisation", null, data1)
                        it.getString(4)?.trim()?.let { title -> titles.add(title) }
                    }
                    Nickname.CONTENT_ITEM_TYPE -> entry(entries, "nickname", null, data1)
                    Website.CONTENT_ITEM_TYPE -> entry(entries, "website", null, data1)
                    Note.CONTENT_ITEM_TYPE -> entry(entries, "note", null, data1)
                    Event.CONTENT_ITEM_TYPE ->
                        if (kind() == Event.TYPE_BIRTHDAY) entry(entries, "birthday", null, data1)
                }
            }
        }
        titles.forEach { entry(entries, "job-title", null, it) }
        return JSONObject().put("id", sourceId ?: "").put("name", name).put("entries", entries)
    }

    private fun entry(into: JSONArray, field: String, label: String?, value: String) {
        if (value.isEmpty()) return
        into.put(
            JSONObject()
                .put("field", field)
                .put("label", label ?: JSONObject.NULL)
                .put("value", value),
        )
    }

    /** The rows of data a contact is, for a raw contact to be given. */
    private fun dataRows(contact: JSONObject): List<ContentValues> {
        val rows = ArrayList<ContentValues>()
        val name = contact.optString("name")
        if (name.isNotEmpty()) {
            rows.add(row(StructuredName.CONTENT_ITEM_TYPE).apply { put(StructuredName.DISPLAY_NAME, name) })
        }
        val entries = contact.optJSONArray("entries") ?: JSONArray()
        val companies = ArrayList<String>()
        val titles = ArrayList<String>()
        for (i in 0 until entries.length()) {
            val e = entries.getJSONObject(i)
            val value = e.optString("value")
            val label = if (e.isNull("label")) null else e.optString("label")
            when (e.optString("field")) {
                "phone" -> {
                    val (kind, custom) = Kinds.phones.kindOf(label)
                    rows.add(
                        row(Phone.CONTENT_ITEM_TYPE).apply {
                            put(Phone.NUMBER, value)
                            put(Phone.TYPE, kind)
                            put(Phone.LABEL, custom)
                        },
                    )
                }
                "email" -> {
                    val (kind, custom) = Kinds.emails.kindOf(label)
                    rows.add(
                        row(Email.CONTENT_ITEM_TYPE).apply {
                            put(Email.ADDRESS, value)
                            put(Email.TYPE, kind)
                            put(Email.LABEL, custom)
                        },
                    )
                }
                "address" -> {
                    val (kind, custom) = Kinds.addresses.kindOf(label)
                    rows.add(
                        row(StructuredPostal.CONTENT_ITEM_TYPE).apply {
                            put(StructuredPostal.FORMATTED_ADDRESS, value)
                            put(StructuredPostal.TYPE, kind)
                            put(StructuredPostal.LABEL, custom)
                        },
                    )
                }
                "organisation" -> companies.add(value)
                "job-title" -> titles.add(value)
                "nickname" ->
                    rows.add(row(Nickname.CONTENT_ITEM_TYPE).apply { put(Nickname.NAME, value) })
                "website" ->
                    rows.add(row(Website.CONTENT_ITEM_TYPE).apply { put(Website.URL, value) })
                "note" -> rows.add(row(Note.CONTENT_ITEM_TYPE).apply { put(Note.NOTE, value) })
                "birthday" ->
                    rows.add(
                        row(Event.CONTENT_ITEM_TYPE).apply {
                            put(Event.START_DATE, value)
                            put(Event.TYPE, Event.TYPE_BIRTHDAY)
                        },
                    )
            }
        }
        // A company and a job title are one row, as the contacts app has them:
        // the nth company with the nth title, and what is left over alone.
        for (i in 0 until maxOf(companies.size, titles.size)) {
            rows.add(
                row(Organization.CONTENT_ITEM_TYPE).apply {
                    put(Organization.COMPANY, companies.getOrNull(i))
                    put(Organization.TITLE, titles.getOrNull(i))
                },
            )
        }
        return rows
    }

    private fun row(mimeType: String) = ContentValues().apply { put(Data.MIMETYPE, mimeType) }

    private fun asSyncAdapter(uri: Uri): Uri = uri.buildUpon()
        .appendQueryParameter(ContactsContract.CALLER_IS_SYNCADAPTER, "true")
        .appendQueryParameter(RawContacts.ACCOUNT_NAME, ACCOUNT_NAME)
        .appendQueryParameter(RawContacts.ACCOUNT_TYPE, ACCOUNT_TYPE)
        .build()

    companion object {
        const val ACCOUNT_TYPE = "uk.co.aylett.quiddity.contacts"
        const val ACCOUNT_NAME = "Quiddity"

        /** The data Quiddity has, and so replaces; any other it leaves alone. */
        private val MANAGED = arrayOf(
            StructuredName.CONTENT_ITEM_TYPE,
            Phone.CONTENT_ITEM_TYPE,
            Email.CONTENT_ITEM_TYPE,
            StructuredPostal.CONTENT_ITEM_TYPE,
            Organization.CONTENT_ITEM_TYPE,
            Nickname.CONTENT_ITEM_TYPE,
            Website.CONTENT_ITEM_TYPE,
            Note.CONTENT_ITEM_TYPE,
            Event.CONTENT_ITEM_TYPE,
        )
    }
}
