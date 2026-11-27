package uk.co.aylett.quiddity.sync

import org.junit.Assert.assertEquals
import org.junit.Test

class HoldWatcherTest {
    /** A clock that only moves when the watcher sleeps. */
    private class Time {
        var now = 0L
        val asks = mutableListOf<Long>()
    }

    private fun watcher(time: Time, limit: Long, poll: Long, outstanding: () -> Boolean) =
        HoldWatcher(
            outstanding = {
                time.asks += time.now
                outstanding()
            },
            limitMs = limit,
            pollMs = poll,
            clock = { time.now },
            sleep = { time.now += it },
        )

    @Test
    fun stopsAtOnceWhenNothingIsOutstanding() {
        val time = Time()
        assertEquals(Reason.NOTHING_OUTSTANDING, watcher(time, 1_000, 100) { false }.run())
        assertEquals(listOf(0L), time.asks)
        assertEquals(0L, time.now)
    }

    @Test
    fun keepsAskingEachPollWhileSomethingIsOutstandingAndStopsWhenItIsDone() {
        val time = Time()
        val answers = ArrayDeque(listOf(true, true, false))
        assertEquals(
            Reason.NOTHING_OUTSTANDING,
            watcher(time, 10_000, 100) { answers.removeFirst() }.run(),
        )
        assertEquals(listOf(0L, 100L, 200L), time.asks)
    }

    @Test
    fun givesUpAtTheLimitWhateverIsStillOutstanding() {
        val time = Time()
        assertEquals(Reason.TIMED_OUT, watcher(time, 1_000, 300) { true }.run())
        // Asked at 0, 300, 600 and 900; at 1200 the time was up.
        assertEquals(listOf(0L, 300L, 600L, 900L), time.asks)
        assertEquals(1_200L, time.now)
    }

    @Test
    fun anInterruptEndsIt() {
        val interrupted = HoldWatcher(
            outstanding = { true },
            limitMs = 1_000,
            pollMs = 100,
            clock = { 0L },
            sleep = { throw InterruptedException() },
        )
        assertEquals(Reason.INTERRUPTED, interrupted.run())
    }
}
