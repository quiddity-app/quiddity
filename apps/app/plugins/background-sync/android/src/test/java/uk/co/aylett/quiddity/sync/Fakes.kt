package uk.co.aylett.quiddity.sync

import org.junit.rules.TestWatcher
import org.junit.runner.Description

/** What a test says the library does, and what it was asked. */
class FakeBackend(
    var outcome: Int = Outcome.SETTLED,
    var isOutstanding: () -> Boolean = { false },
    var failure: Throwable? = null,
) : SyncBackend {
    data class Call(val dataDir: String, val identifier: String, val timeoutSecs: Int, val blobs: Boolean)

    val calls = mutableListOf<Call>()

    override fun syncOnce(dataDir: String, identifier: String, timeoutSecs: Int, blobs: Boolean): Int {
        failure?.let { throw it }
        calls += Call(dataDir, identifier, timeoutSecs, blobs)
        return outcome
    }

    override fun outstanding(): Boolean {
        failure?.let { throw it }
        return isOutstanding()
    }
}

/** Puts [Sync] back as it was, so that a test that replaced it leaves nothing behind. */
class SyncRule : TestWatcher() {
    private val backend = Sync.backend
    private val metered = Sync.metered

    override fun finished(description: Description?) {
        Sync.backend = backend
        Sync.metered = metered
    }
}

/** Waits for something another thread does, for up to five seconds. */
fun waitUntil(what: String, condition: () -> Boolean) {
    val end = System.currentTimeMillis() + 5_000
    while (!condition()) {
        if (System.currentTimeMillis() > end) throw AssertionError("timed out waiting for $what")
        Thread.sleep(20)
    }
}
