package com.example.myaiagent.execution

import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlin.coroutines.coroutineContext

enum class SearchState { EDITOR, RESULTS, OTHER }
enum class Attempt { ACCEPTED, UNAVAILABLE, FAILED }
enum class SubmissionResult { VERIFIED, FAILED, UNCONFIRMED }

interface SearchUi {
    fun focusAndEnter(): Boolean
    fun state(): SearchState
    fun ime(): Attempt
    fun button(): Attempt
}

/** An accepted accessibility action is not evidence that the app performed a search. */
class SearchSubmission(private val ui: SearchUi, private val pause: suspend () -> Unit = { delay(250) }, private val polls: Int = 20) {
    private suspend fun observe(): SearchState {
        repeat(polls) {
            coroutineContext.ensureActive()
            val state = ui.state()
            if (state == SearchState.RESULTS) return state
            pause()
        }
        return ui.state()
    }
    suspend fun run(): SubmissionResult {
        coroutineContext.ensureActive()
        if (!ui.focusAndEnter() || ui.state() != SearchState.EDITOR) return SubmissionResult.FAILED
        coroutineContext.ensureActive()
        val ime = ui.ime()
        if (ime == Attempt.ACCEPTED) {
            when (observe()) {
                SearchState.RESULTS -> return SubmissionResult.VERIFIED
                SearchState.OTHER -> return SubmissionResult.UNCONFIRMED
                SearchState.EDITOR -> Unit
            }
        }
        coroutineContext.ensureActive()
        // Only fall back if the original focused editor remains unchanged; never click on an unknown screen.
        if (ui.state() != SearchState.EDITOR) return SubmissionResult.UNCONFIRMED
        if (ui.button() != Attempt.ACCEPTED) return if (ime == Attempt.ACCEPTED) SubmissionResult.UNCONFIRMED else SubmissionResult.FAILED
        return if (observe() == SearchState.RESULTS) SubmissionResult.VERIFIED else SubmissionResult.UNCONFIRMED
    }
}
