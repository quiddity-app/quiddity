package uk.co.aylett.quiddity.sync

import org.junit.Assert.assertEquals
import org.junit.Test

class SyncPolicyTest {
    @Test
    fun aRunThatBrokeIsTriedAgainButNotForEver() {
        for (attempt in 0 until SyncPolicy.MAX_ATTEMPTS) {
            assertEquals("attempt $attempt", Verdict.RETRY, SyncPolicy.verdict(Outcome.FAILED, attempt))
        }
        assertEquals(Verdict.DONE, SyncPolicy.verdict(Outcome.FAILED, SyncPolicy.MAX_ATTEMPTS))
    }

    @Test
    fun reachingNobodyOrBeingCutOffIsNotWorthAnotherTryNow() {
        val others = listOf(Outcome.SETTLED, Outcome.OFFLINE, Outcome.BEHIND, Outcome.NO_GRAPH)
        for (outcome in others) {
            assertEquals("outcome $outcome", Verdict.DONE, SyncPolicy.verdict(outcome, 0))
        }
    }
}
