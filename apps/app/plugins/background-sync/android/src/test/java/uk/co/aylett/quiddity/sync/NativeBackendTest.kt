package uk.co.aylett.quiddity.sync

import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * The library is only in an Android process, so on the JVM its calls fail, and
 * failing loudly is what this checks: a plugin that took the absence for an
 * answer would sync nothing and say so to nobody. The first call is the one
 * that tries to load it.
 */
class NativeBackendTest {
    @Test
    fun callsTheLibraryAndFailsWhereThereIsNone() {
        val backend = NativeBackend()

        assertThrows(UnsatisfiedLinkError::class.java) {
            backend.syncOnce("/data/user/0/app", "app", 1, false)
        }
        // The class that failed to load stays failed: an error, not an answer.
        assertThrows(Error::class.java) { backend.outstanding() }
    }
}
