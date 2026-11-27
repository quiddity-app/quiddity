package uk.co.aylett.quiddity.sync

/** The numbers `background::Outcome::code` returns, which a Rust test pins. */
object Outcome {
    const val SETTLED = 0
    const val OFFLINE = 1
    const val BEHIND = 2
    const val NO_GRAPH = 3
    const val FAILED = 4
}
