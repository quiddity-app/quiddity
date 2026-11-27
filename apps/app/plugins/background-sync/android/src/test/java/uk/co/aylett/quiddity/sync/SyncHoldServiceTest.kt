package uk.co.aylett.quiddity.sync

import android.app.Application
import androidx.test.core.app.ApplicationProvider
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class SyncHoldServiceTest {
    @get:Rule
    val restore = SyncRule()

    private val app: Application = ApplicationProvider.getApplicationContext()

    @Test
    fun holdsTheProcessWithANotificationAndLetsGoWhenNothingIsOutstanding() {
        Sync.backend = FakeBackend(isOutstanding = { false })
        val controller = Robolectric.buildService(SyncHoldService::class.java).create()
        val service = controller.get()

        controller.startCommand(0, 1)

        assertNotNull(shadowOf(service).lastForegroundNotification)
        waitUntil("the service to stop itself") { shadowOf(service).isStoppedBySelf }
    }

    /** The type of a foreground service is a parameter from Android 10, so before it there is none to pass. */
    @Test
    @Config(sdk = [28])
    fun holdsTheProcessOnAndroid9TooWithoutAType() {
        Sync.backend = FakeBackend(isOutstanding = { false })
        val controller = Robolectric.buildService(SyncHoldService::class.java).create()
        val service = controller.get()

        controller.startCommand(0, 1)

        assertNotNull(shadowOf(service).lastForegroundNotification)
        waitUntil("the service to stop itself") { shadowOf(service).isStoppedBySelf }
    }

    @Test
    fun beingDestroyedEndsTheWatching() {
        val asked = CountDownLatch(1)
        Sync.backend = FakeBackend(isOutstanding = {
            asked.countDown()
            true
        })
        val controller = Robolectric.buildService(SyncHoldService::class.java).create()
        val service = controller.get()
        controller.startCommand(0, 1)
        assertTrue("the watcher asked", asked.await(5, TimeUnit.SECONDS))

        controller.destroy()

        waitUntil("the watcher to stop") { shadowOf(service).isStoppedBySelf }
    }

    @Test
    fun startsTheServiceOnlyWhenSomethingIsOutstanding() {
        Sync.backend = FakeBackend(isOutstanding = { false })
        SyncHoldService.holdIfOutstanding(app)
        assertNull(shadowOf(app).nextStartedService)

        Sync.backend = FakeBackend(isOutstanding = { true })
        SyncHoldService.holdIfOutstanding(app)
        assertEquals(
            SyncHoldService::class.java.name,
            shadowOf(app).nextStartedService.component?.className,
        )
    }

    @Test
    fun aBackendThatFailsOrIsMissingDoesNotCrashTheApp() {
        for (failure in listOf(RuntimeException("boom"), UnsatisfiedLinkError("no library"))) {
            Sync.backend = FakeBackend(failure = failure)
            SyncHoldService.holdIfOutstanding(app)
            assertNull("$failure", shadowOf(app).nextStartedService)
        }
    }
}
