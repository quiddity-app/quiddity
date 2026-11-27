package uk.co.aylett.quiddity.sync

import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/**
 * Holds the process for a little while after the user leaves the app, so that a
 * sync that was under way finishes instead of being cut off when the system
 * stops the process. It watches whether anything is outstanding ([HoldWatcher])
 * and ends as soon as there is not, and in any case after
 * [SyncPolicy.HOLD_LIMIT_MS]: it is never a
 * reason to stay connected, only a way to finish what had started.
 *
 * A `dataSync` foreground service, which Android 15 limits to a few hours a
 * day: a minute at a time is far inside that.
 */
class SyncHoldService : Service() {
    @Volatile
    private var watcher: Thread? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // The type is a parameter from Android 10, and required from 14.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                NOTIFICATION_ID,
                notification(),
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification())
        }
        if (watcher == null) {
            watcher = Thread {
                HoldWatcher(Sync.backend::outstanding).run()
                stopSelf()
            }.also { it.start() }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        watcher?.interrupt()
        watcher = null
        super.onDestroy()
    }

    private fun notification(): Notification {
        NotificationManagerCompat.from(this).createNotificationChannel(
            NotificationChannelCompat.Builder(CHANNEL, NotificationManagerCompat.IMPORTANCE_LOW)
                .setName("Syncing")
                .build(),
        )
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setContentTitle("Finishing a sync")
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    companion object {
        private const val TAG = "QuiddityBackgroundSync"
        private const val CHANNEL = "quiddity-sync"
        private const val NOTIFICATION_ID = 0x51

        /**
         * Starts the service if a sync is outstanding. The app is in the
         * foreground when the user is leaving it, which is when Android allows
         * this; if it refuses anyway, the sync simply isn't held.
         */
        fun holdIfOutstanding(context: Context) {
            try {
                if (Sync.backend.outstanding()) {
                    ContextCompat.startForegroundService(
                        context,
                        Intent(context, SyncHoldService::class.java),
                    )
                }
            } catch (e: Exception) {
                Log.w(TAG, "couldn't hold the sync: ${e.message}")
            } catch (e: UnsatisfiedLinkError) {
                Log.w(TAG, "the library has no outstanding call: ${e.message}")
            }
        }
    }
}
