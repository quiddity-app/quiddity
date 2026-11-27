package uk.co.aylett.quiddity.sync

import android.content.Context
import androidx.core.net.ConnectivityManagerCompat

/**
 * The app's library, which is the graph and the network. The names are what
 * `apps/app/src-tauri/src/android.rs` exports (`Java_uk_co_aylett_quiddity_sync_Native_*`),
 * so changing one means changing the other.
 *
 * Nothing but [NativeBackend] calls it, so that everything else can be tested
 * without the library, which only an Android process has.
 */
object Native {
    init {
        // Loaded here as well as by the activity: a background run may be the
        // first thing in a process, with no activity to have loaded it.
        System.loadLibrary("quiddity_app_lib")
    }

    /**
     * Syncs the graph once and returns how it went, one of [Outcome]. Blocks
     * until it is done or [timeoutSecs] have passed. [blobs] is whether to
     * fetch files too. Never makes a graph: a device with none has to be told
     * how to begin, by the app.
     */
    @JvmStatic
    external fun syncOnce(dataDir: String, identifier: String, timeoutSecs: Int, blobs: Boolean): Int

    /** Whether a sync the user shouldn't interrupt is under way in this process. */
    @JvmStatic
    external fun outstanding(): Boolean
}

/** What the plugin asks of the library, so that tests can stand in for it. */
interface SyncBackend {
    fun syncOnce(dataDir: String, identifier: String, timeoutSecs: Int, blobs: Boolean): Int

    fun outstanding(): Boolean
}

/** The library itself, through JNI. */
class NativeBackend : SyncBackend {
    override fun syncOnce(
        dataDir: String,
        identifier: String,
        timeoutSecs: Int,
        blobs: Boolean,
    ): Int = Native.syncOnce(dataDir, identifier, timeoutSecs, blobs)

    override fun outstanding(): Boolean = Native.outstanding()
}

/**
 * What the plugin's classes use, which a test replaces. Replacing is the only
 * reason these are variables.
 */
object Sync {
    @Volatile
    var backend: SyncBackend = NativeBackend()

    /** Whether the active network is metered, which is not known to be false. */
    @Volatile
    var metered: (Context) -> Boolean = { context ->
        val manager = context.getSystemService(android.net.ConnectivityManager::class.java)
        manager == null || ConnectivityManagerCompat.isActiveNetworkMetered(manager)
    }
}
