package com.example.myaiagent.execution

import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlin.coroutines.coroutineContext

enum class SearchState { EDITOR, SUGGESTIONS, RESULTS, OTHER }
enum class Attempt { ACCEPTED, UNAVAILABLE, FAILED }
enum class SubmissionResult { VERIFIED, FAILED, UNCONFIRMED }
interface SearchUi {
    fun focusAndEnter(): Boolean
    fun state(): SearchState
    fun ime(): Attempt
    fun suggestion(): Attempt
}

class SearchSubmission(private val ui: SearchUi, private val pause: suspend () -> Unit = { delay(250) }, private val polls: Int = 20) {
    private suspend fun observe(): SearchState {
        repeat(polls) {
            coroutineContext.ensureActive()
            if (ui.state() == SearchState.RESULTS) return SearchState.RESULTS
            pause()
        }
        coroutineContext.ensureActive()
        return ui.state()
    }
    suspend fun run(): SubmissionResult {
        coroutineContext.ensureActive()
        if (!ui.focusAndEnter() || ui.state() !in setOf(SearchState.EDITOR, SearchState.SUGGESTIONS)) return SubmissionResult.FAILED
        coroutineContext.ensureActive()
        val ime = ui.ime()
        // Always wait for delayed IME results before considering another action.
        when (observe()) {
            SearchState.RESULTS -> return SubmissionResult.VERIFIED
            SearchState.SUGGESTIONS -> Unit
            else -> return if (ime == Attempt.ACCEPTED) SubmissionResult.UNCONFIRMED else SubmissionResult.FAILED
        }
        coroutineContext.ensureActive()
        if (ui.state() != SearchState.SUGGESTIONS) return SubmissionResult.UNCONFIRMED
        coroutineContext.ensureActive()
        if (ui.suggestion() != Attempt.ACCEPTED) return SubmissionResult.UNCONFIRMED
        return if (observe() == SearchState.RESULTS) SubmissionResult.VERIFIED else SubmissionResult.UNCONFIRMED
    }
}
