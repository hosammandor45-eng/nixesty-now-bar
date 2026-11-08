package com.example.myaiagent.execution

import com.example.myaiagent.command.Command
import com.example.myaiagent.command.CommandParser
import com.example.myaiagent.command.NameMatcher
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class SearchSubmissionTest {
    private class Fake : SearchUi {
        var focused = true
        var state = SearchState.EDITOR
        var imeResult = Attempt.ACCEPTED
        var buttonResult = Attempt.ACCEPTED
        var onIme: () -> Unit = {}
        var onButton: () -> Unit = {}
        var buttons = 0
        override fun focusAndEnter() = focused
        override fun state() = state
        override fun ime(): Attempt { onIme(); return imeResult }
        override fun button(): Attempt { buttons++; onButton(); return buttonResult }
    }
    private suspend fun run(fake: Fake) = SearchSubmission(fake, pause = {}, polls = 2).run()
    @Test fun focusFailureStopsBeforeSubmission() = runBlocking {
        val f = Fake().apply { focused = false }; assertEquals(SubmissionResult.FAILED, run(f)); assertEquals(0, f.buttons)
    }
    @Test fun imeSuccessRequiresResults() = runBlocking {
        val f = Fake(); f.onIme = { f.state = SearchState.RESULTS }
        assertEquals(SubmissionResult.VERIFIED, run(f)); assertEquals(0, f.buttons)
    }
    @Test fun unavailableOrFailedImeUsesUniqueButton() = runBlocking {
        for (result in listOf(Attempt.UNAVAILABLE, Attempt.FAILED)) {
            val f = Fake().apply { imeResult = result }; f.onButton = { f.state = SearchState.RESULTS }
            assertEquals(SubmissionResult.VERIFIED, run(f)); assertEquals(1, f.buttons)
        }
    }
    @Test fun acceptedImeWithoutTransitionFallsBackOnce() = runBlocking {
        val f = Fake(); f.onButton = { f.state = SearchState.RESULTS }
        assertEquals(SubmissionResult.VERIFIED, run(f)); assertEquals(1, f.buttons)
    }
    @Test fun timeoutIsUnconfirmedAndNeverRepeatsAction() = runBlocking {
        val f = Fake(); assertEquals(SubmissionResult.UNCONFIRMED, run(f)); assertEquals(1, f.buttons)
    }
    @Test fun cancellationStopsBeforeFallback() = runBlocking {
        val f = Fake()
        try { SearchSubmission(f, pause = { throw CancellationException() }, polls = 2).run(); fail("expected cancellation") }
        catch (_: CancellationException) { assertEquals(0, f.buttons) }
    }
    @Test fun arabicOpeningRegressions() {
        assertTrue(NameMatcher.isSettings("الإعدادات"))
        assertTrue("com.google.android.youtube" in NameMatcher.knownPackages("يوتيوب"))
        assertEquals(listOf(Command.OpenApp("الإعدادات")), CommandParser.parse("افتح الإعدادات"))
        assertEquals(listOf(Command.OpenApp("يوتيوب")), CommandParser.parse("افتح يوتيوب"))
    }
}
