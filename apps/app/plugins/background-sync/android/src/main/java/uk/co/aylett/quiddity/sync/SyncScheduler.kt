package uk.co.aylett.quiddity.sync

import android.content.Context
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit

/**
 * Keeps one periodic sync scheduled. The system batches it with others and
 * delays it in Doze, so "every six hours" is roughly, and a device that is
 * never on a network is never synced: this is for a phone that hasn't been
 * opened for a while, not for being reachable.
 */
object SyncScheduler {
    const val NAME = "quiddity-background-sync"

    fun schedule(context: Context) {
        val request = PeriodicWorkRequestBuilder<SyncWorker>(SyncPolicy.PERIOD_HOURS, TimeUnit.HOURS)
            .setConstraints(
                Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build(),
            )
            .build()
        // UPDATE rather than KEEP, so that a new period or constraint reaches
        // phones that already have the old one scheduled, without restarting
        // the clock each time the app opens.
        WorkManager.getInstance(context)
            .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.UPDATE, request)
    }
}
