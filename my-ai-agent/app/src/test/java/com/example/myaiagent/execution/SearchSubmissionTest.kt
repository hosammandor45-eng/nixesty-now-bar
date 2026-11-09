package com.example.myaiagent.execution

import com.example.myaiagent.command.Command
import com.example.myaiagent.command.CommandParser
import com.example.myaiagent.command.NameMatcher
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class SearchSubmissionTest {
    private val ready = StageResult(StageStatus.READY, "ready")
    private val waiting = StageResult(StageStatus.NOT_READY, "waiting")
    private class Fake : SearchUi {
        var opened = StageResult(StageStatus.READY, "ready")
        var field = StageResult(StageStatus.READY, "ready")
        var focus = StageResult(StageStatus.READY, "ready")
        var focusConfirmed = StageResult(StageStatus.READY, "ready")
        var text = StageResult(StageStatus.READY, "ready")
        var verified = StageResult(StageStatus.READY, "ready")
        var currentState = SearchState.EDITOR
        var imeResult = Attempt.ACCEPTED
        var suggestionResult = Attempt.ACCEPTED
        var onIme: () -> Unit = {}
        var onSuggestion: () -> Unit = {}
        var textCalls = 0
        var imeCalls = 0
        var suggestionCalls = 0
        override fun openSearch() = opened
        override fun discoverField() = field
        override fun requestFocus() = focus
        override fun verifyFocus() = focusConfirmed
        override fun enterText(): StageResult { textCalls++; return text }
        override fun verifyText() = verified
        override fun state() = currentState
        override fun ime(): Attempt { imeCalls++; onIme(); return imeResult }
        override fun suggestion(): Attempt { suggestionCalls++; onSuggestion(); return suggestionResult }
    }
    private suspend fun run(fake: Fake) = SearchSubmission(fake, pause = {}, polls = 2).run()
    @Test fun delayedSearchFieldIsRetriedWithoutRepeatedEntry() = runBlocking {
        val f = Fake().apply { field = waiting }; var ticks = 0
        f.onIme = { f.currentState = SearchState.RESULTS }
        val result = SearchSubmission(f, pause = { if (++ticks == 2) f.field = ready }, polls = 4).run()
        assertEquals(SubmissionResult.VERIFIED, result.result); assertEquals(1, f.textCalls)
    }
    @Test fun missingFieldTimesOutWithStageAndNoText() = runBlocking {
        val f = Fake().apply { field = waiting }; val result = run(f)
        assertEquals(SubmissionResult.FAILED, result.result); assertEquals(Stage.FIELD, result.stage); assertEquals(0, f.textCalls)
    }
    @Test fun focusChangesAndDelayedTextAreVerifiedWithoutReentering() = runBlocking {
        val f = Fake().apply { focusConfirmed = waiting; verified = waiting }; var ticks = 0
        f.onIme = { f.currentState = SearchState.RESULTS }
        val result = SearchSubmission(f, pause = {
            ticks++
            if (ticks == 1) f.focusConfirmed = ready
            if (ticks == 2) { f.focusConfirmed = waiting; f.verified = ready }
        }, polls = 4).run()
        assertEquals(SubmissionResult.VERIFIED, result.result); assertEquals(1, f.textCalls)
    }
    @Test fun focusFailureHasSpecificStage() = runBlocking {
        val f = Fake().apply { focus = StageResult(StageStatus.FAILED, "focus rejected") }
        val result = run(f); assertEquals(Stage.FOCUS, result.stage); assertEquals(0, f.textCalls)
    }
    @Test fun textVerificationTimeoutIsUnconfirmed() = runBlocking {
        val f = Fake().apply { verified = waiting }; val result = run(f)
        assertEquals(SubmissionResult.UNCONFIRMED, result.result); assertEquals(Stage.TEXT_VERIFICATION, result.stage)
        assertEquals(1, f.textCalls); assertEquals(0, f.imeCalls)
    }
    @Test fun acceptedImeWithResultsIsVerified() = runBlocking {
        val f = Fake(); f.onIme = { f.currentState = SearchState.RESULTS }
        assertEquals(SubmissionResult.VERIFIED, run(f).result); assertEquals(1, f.imeCalls); assertEquals(0, f.suggestionCalls)
    }
    @Test fun rejectedTextActionIsNotRetried() = runBlocking {
        val f = Fake().apply { text = StageResult(StageStatus.UNCONFIRMED, "action not confirmed") }
        val result = run(f)
        assertEquals(SubmissionResult.UNCONFIRMED, result.result); assertEquals(Stage.TEXT_ENTRY, result.stage)
        assertEquals(1, f.textCalls); assertEquals(0, f.imeCalls)
    }
    @Test fun acceptedImeNeverTriggersSecondSubmission() = runBlocking {
        val f = Fake(); f.onIme = { f.currentState = SearchState.SUGGESTIONS }
        assertEquals(SubmissionResult.UNCONFIRMED, run(f).result); assertEquals(0, f.suggestionCalls)
    }
    @Test fun unavailableImeCanUseOneSuggestion() = runBlocking {
        val f = Fake().apply { imeResult = Attempt.UNAVAILABLE }; f.onIme = { f.currentState = SearchState.SUGGESTIONS }
        f.onSuggestion = { f.currentState = SearchState.RESULTS }
        assertEquals(SubmissionResult.VERIFIED, run(f).result); assertEquals(1, f.suggestionCalls)
    }
    @Test fun failedImeDoesNotTriggerSecondSubmission() = runBlocking {
        val f = Fake().apply { imeResult = Attempt.FAILED }; f.onIme = { f.currentState = SearchState.SUGGESTIONS }
        assertEquals(SubmissionResult.UNCONFIRMED, run(f).result); assertEquals(0, f.suggestionCalls)
    }
    @Test fun cancellationDuringDiscoveryOrVerificationStopsActions() = runBlocking {
        for (discovery in listOf(true, false)) {
            val f = Fake(); if (discovery) f.field = waiting else f.verified = waiting
            try { SearchSubmission(f, pause = { throw CancellationException() }, polls = 2).run(); fail("expected cancellation") }
            catch (_: CancellationException) { assertEquals(0, f.imeCalls); assertEquals(0, f.suggestionCalls); assertEquals(if (discovery) 0 else 1, f.textCalls) }
        }
    }
    @Test fun arabicOpeningRegressions() {
        assertTrue(NameMatcher.isSettings("الإعدادات")); assertTrue("com.google.android.youtube" in NameMatcher.knownPackages("يوتيوب"))
        assertEquals(listOf(Command.OpenApp("الإعدادات")), CommandParser.parse("افتح الإعدادات"))
        assertEquals(listOf(Command.OpenApp("يوتيوب")), CommandParser.parse("افتح يوتيوب"))
    }
}
