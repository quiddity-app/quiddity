package uk.co.aylett.quiddity.sync

import android.app.Activity
import android.webkit.WebView
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Plugin

/**
 * Starts background sync with the activity: schedules the periodic job, and,
 * when the user leaves the app with a sync unfinished, holds the process a
 * little while. See `docs/Background sync.md`.
 */
@TauriPlugin
class BackgroundSyncPlugin(private val activity: Activity) : Plugin(activity) {
    override fun load(webView: WebView) {
        SyncScheduler.schedule(activity.applicationContext)
    }

    override fun onPause() {
        SyncHoldService.holdIfOutstanding(activity.applicationContext)
    }
}
