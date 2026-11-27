package uk.co.aylett.quiddity.sync

import android.content.Context
import android.net.ConnectivityManager
import androidx.core.net.ConnectivityManagerCompat
import androidx.test.core.app.ApplicationProvider
import androidx.work.ListenableWorker.Result
import androidx.work.testing.TestWorkerBuilder
import java.util.concurrent.Executors
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class SyncWorkerTest {
    @get:Rule
    val restore = SyncRule()

    private val context: Context = ApplicationProvider.getApplicationContext()

    private fun worker(attempt: Int = 0): SyncWorker =
        TestWorkerBuilder.from(context, SyncWorker::class.java, Executors.newSingleThreadExecutor())
            .setRunAttemptCount(attempt)
            .build()

    @Test
    fun syncsTheAppsGraphWithItsFilesOnAnUnmeteredNetwork() {
        val backend = FakeBackend()
        Sync.backend = backend
        Sync.metered = { false }

        assertEquals(Result.success(), worker().doWork())

        val expected = FakeBackend.Call(
            dataDir = context.dataDir.absolutePath,
            identifier = context.packageName,
            timeoutSecs = SyncPolicy.TIMEOUT_SECS,
            blobs = true,
        )
        assertEquals(listOf(expected), backend.calls)
    }

    @Test
    fun syncsOnlyTheGraphOnAMeteredNetwork() {
        val backend = FakeBackend()
        Sync.backend = backend
        Sync.metered = { true }

        assertEquals(Result.success(), worker().doWork())

        assertEquals(listOf(false), backend.calls.map { it.blobs })
    }

    @Test
    fun aRunThatBrokeIsRetriedUntilItHasHadItsAttempts() {
        Sync.backend = FakeBackend(outcome = Outcome.FAILED)
        Sync.metered = { false }

        assertEquals(Result.retry(), worker(attempt = 0).doWork())
        assertEquals(Result.success(), worker(attempt = SyncPolicy.MAX_ATTEMPTS).doWork())
    }

    @Test
    fun reachingNobodyIsNotAFailure() {
        Sync.backend = FakeBackend(outcome = Outcome.OFFLINE)
        Sync.metered = { false }

        assertEquals(Result.success(), worker().doWork())
    }

    @Test
    fun aLibraryThatIsNotThereFailsTheRunAndDoesNotCrash() {
        Sync.backend = FakeBackend(failure = UnsatisfiedLinkError("no library"))
        Sync.metered = { false }

        assertEquals(Result.failure(), worker().doWork())
    }

    @Test
    fun theRealMeteringCheckAsksTheSystem() {
        val manager = context.getSystemService(ConnectivityManager::class.java)
        assertEquals(ConnectivityManagerCompat.isActiveNetworkMetered(manager), Sync.metered(context))
    }
}
