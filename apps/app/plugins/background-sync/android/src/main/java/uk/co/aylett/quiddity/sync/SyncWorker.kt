package uk.co.aylett.quiddity.sync

import android.content.Context
import android.util.Log
import androidx.work.Worker
import androidx.work.WorkerParameters

/**
 * Syncs the graph once, for [SyncScheduler]'s periodic job. The system gives a
 * job about ten minutes at most, and this uses a couple. What counts as worth
 * trying again is [SyncPolicy.verdict]'s.
 */
class SyncWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result {
        val context = applicationContext
        // Files can be large: on a metered network only the graph is synced.
        val metered = Sync.metered(context)
        val outcome = try {
            Sync.backend.syncOnce(
                context.dataDir.absolutePath,
                context.packageName,
                SyncPolicy.TIMEOUT_SECS,
                !metered,
            )
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "the library has no sync call: ${e.message}")
            return Result.failure()
        }
        Log.i(TAG, "synced: outcome $outcome${if (metered) " (metered, no files)" else ""}")
        return when (SyncPolicy.verdict(outcome, runAttemptCount)) {
            Verdict.RETRY -> Result.retry()
            Verdict.DONE -> Result.success()
        }
    }

    private companion object {
        const val TAG = "QuiddityBackgroundSync"
    }
}
