package uk.co.aylett.quiddity.sync

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.work.NetworkType
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.testing.WorkManagerTestInitHelper
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class SyncSchedulerTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    @Before
    fun startWorkManager() {
        WorkManagerTestInitHelper.initializeTestWorkManager(context)
    }

    private fun scheduled(): List<WorkInfo> =
        WorkManager.getInstance(context).getWorkInfosForUniqueWork(SyncScheduler.NAME).get()

    @Test
    fun schedulesOnePeriodicJobThatNeedsANetwork() {
        SyncScheduler.schedule(context)

        val job = scheduled().single()
        assertEquals(WorkInfo.State.ENQUEUED, job.state)
        assertEquals(NetworkType.CONNECTED, job.constraints.requiredNetworkType)
        assertEquals(
            TimeUnit.HOURS.toMillis(SyncPolicy.PERIOD_HOURS),
            job.periodicityInfo?.repeatIntervalMillis,
        )
    }

    @Test
    fun schedulingAgainEachTimeTheAppOpensKeepsOneJob() {
        SyncScheduler.schedule(context)
        SyncScheduler.schedule(context)

        assertEquals(1, scheduled().size)
    }
}
