package uk.co.aylett.quiddity.sync

import android.app.Activity
import android.app.Application
import android.content.Context
import android.webkit.WebView
import androidx.test.core.app.ApplicationProvider
import androidx.work.WorkManager
import androidx.work.testing.WorkManagerTestInitHelper
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class BackgroundSyncPluginTest {
    @get:Rule
    val restore = SyncRule()

    private val app: Application = ApplicationProvider.getApplicationContext()
    private val context: Context = app

    @Before
    fun startWorkManager() {
        WorkManagerTestInitHelper.initializeTestWorkManager(context)
    }

    private fun plugin(): Pair<BackgroundSyncPlugin, Activity> {
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        return BackgroundSyncPlugin(activity) to activity
    }

    @Test
    fun schedulesTheSyncWhenTheActivityLoadsIt() {
        val (plugin, activity) = plugin()

        plugin.load(WebView(activity))

        val jobs = WorkManager.getInstance(context).getWorkInfosForUniqueWork(SyncScheduler.NAME).get()
        assertEquals(1, jobs.size)
    }

    @Suppress("DEPRECATION")
    @Test
    fun holdsTheProcessWhenTheUserLeavesWithASyncOutstanding() {
        val (plugin, _) = plugin()

        Sync.backend = FakeBackend(isOutstanding = { false })
        plugin.onPause()
        assertNull(shadowOf(app).nextStartedService)

        Sync.backend = FakeBackend(isOutstanding = { true })
        plugin.onPause()
        assertEquals(
            SyncHoldService::class.java.name,
            shadowOf(app).nextStartedService.component?.className,
        )
    }
}
