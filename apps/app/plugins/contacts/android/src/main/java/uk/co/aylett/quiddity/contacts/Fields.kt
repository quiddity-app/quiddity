package uk.co.aylett.quiddity.contacts

import android.provider.ContactsContract.CommonDataKinds.Email
import android.provider.ContactsContract.CommonDataKinds.Phone
import android.provider.ContactsContract.CommonDataKinds.StructuredPostal

/**
 * How a label, which the graph writes as text, is one of the kinds a contacts
 * app offers, and back.
 *
 * Going there and back must give the label that went: a label that changed
 * on the way would look to the graph like the phone having edited it. So
 * `other` is no label at all (the graph reads it so too), a label that isn't
 * one of the kinds is the app's custom one, and a kind this doesn't know
 * reads as no label, though it is kept on the phone until the contact is put
 * again.
 */
internal class Kinds(
    private val named: Map<String, Int>,
    private val other: Int,
    private val extra: Map<Int, String> = emptyMap(),
) {
    /** The kind, and the label for the custom kind (0), for a label. */
    fun kindOf(label: String?): Pair<Int, String?> {
        val name = label?.trim()?.lowercase()
        if (name.isNullOrEmpty() || name == "other") return Pair(other, null)
        val kind = named[name] ?: return Pair(CUSTOM, label.trim())
        return Pair(kind, null)
    }

    /** The label for a kind, which for the custom kind is `custom`. */
    fun labelOf(kind: Int, custom: String?): String? {
        if (kind == CUSTOM) return custom?.trim()?.takeIf { it.isNotEmpty() }
        if (kind == other) return null
        return named.entries.firstOrNull { it.value == kind }?.key ?: extra[kind]
    }

    companion object {
        const val CUSTOM = 0

        val phones = Kinds(
            mapOf(
                "home" to Phone.TYPE_HOME,
                "mobile" to Phone.TYPE_MOBILE,
                "work" to Phone.TYPE_WORK,
            ),
            Phone.TYPE_OTHER,
            mapOf(
                Phone.TYPE_FAX_WORK to "work fax",
                Phone.TYPE_FAX_HOME to "home fax",
                Phone.TYPE_PAGER to "pager",
                Phone.TYPE_CAR to "car",
                Phone.TYPE_MAIN to "main",
                Phone.TYPE_WORK_MOBILE to "work mobile",
            ),
        )

        val emails = Kinds(
            mapOf(
                "home" to Email.TYPE_HOME,
                "work" to Email.TYPE_WORK,
                "mobile" to Email.TYPE_MOBILE,
            ),
            Email.TYPE_OTHER,
        )

        val addresses = Kinds(
            mapOf(
                "home" to StructuredPostal.TYPE_HOME,
                "work" to StructuredPostal.TYPE_WORK,
            ),
            StructuredPostal.TYPE_OTHER,
        )
    }
}
