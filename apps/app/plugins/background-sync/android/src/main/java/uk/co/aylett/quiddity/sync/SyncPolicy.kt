package uk.co.aylett.quiddity.sync

/** What a finished run asks of WorkManager. */
enum class Verdict { DONE, RETRY }

/** How long, how often and when to try again: decisions with no Android in them. */
object SyncPolicy {
    /** How often the periodic job runs, roughly: the system batches it. */
    const val PERIOD_HOURS = 6L

    /** How long one run may sync for. The system gives a job about ten minutes. */
    const val TIMEOUT_SECS = 120

    /** How many times a run that itself broke is tried. */
    const val MAX_ATTEMPTS = 3

    /** The longest the foreground service holds the process. */
    const val HOLD_LIMIT_MS = 60_000L

    /** How often it asks whether there is still something outstanding. */
    const val HOLD_POLL_MS = 1_000L

    /**
     * Reaching nobody is not a failure (the other devices are mostly asleep, and
     * the next run is a few hours away), so only a run that broke is tried
     * again, and not for ever. [attempt] counts from zero.
     */
    fun verdict(outcome: Int, attempt: Int): Verdict =
        if (outcome == Outcome.FAILED && attempt < MAX_ATTEMPTS) Verdict.RETRY else Verdict.DONE
}

/** Why a [HoldWatcher] stopped. */
enum class Reason { NOTHING_OUTSTANDING, TIMED_OUT, INTERRUPTED }

/**
 * Waits while something is outstanding, for no longer than [limitMs]. The clock
 * and the sleep are parameters, so that a test needn't wait.
 */
class HoldWatcher(
    private val outstanding: () -> Boolean,
    private val limitMs: Long = SyncPolicy.HOLD_LIMIT_MS,
    private val pollMs: Long = SyncPolicy.HOLD_POLL_MS,
    private val clock: () -> Long = System::currentTimeMillis,
    private val sleep: (Long) -> Unit = Thread::sleep,
) {
    fun run(): Reason {
        val end = clock() + limitMs
        while (clock() < end) {
            if (!outstanding()) return Reason.NOTHING_OUTSTANDING
            try {
                sleep(pollMs)
            } catch (e: InterruptedException) {
                return Reason.INTERRUPTED
            }
        }
        return Reason.TIMED_OUT
    }
}
