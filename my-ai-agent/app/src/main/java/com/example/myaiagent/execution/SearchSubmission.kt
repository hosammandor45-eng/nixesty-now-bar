package com.example.myaiagent.execution

import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlin.coroutines.coroutineContext

enum class SearchState { EDITOR, SUGGESTIONS, RESULTS, OTHER }
enum class Attempt { ACCEPTED, UNAVAILABLE, FAILED }
enum class SubmissionResult { VERIFIED, FAILED, UNCONFIRMED }
enum class Stage { SEARCH_UI, FIELD, FOCUS, TEXT_ENTRY, TEXT_VERIFICATION, SUBMISSION, RESULTS }
enum class StageStatus { READY, NOT_READY, FAILED, UNCONFIRMED }
data class StageResult(val status: StageStatus, val reason: String)
data class SubmissionOutcome(val result: SubmissionResult, val stage: Stage, val reason: String)
interface SearchUi {
    fun openSearch(): StageResult
    fun discoverField(): StageResult
    fun requestFocus(): StageResult
    fun verifyFocus(): StageResult
    fun enterText(): StageResult
    fun verifyText(): StageResult
    fun state(): SearchState
    fun ime(): Attempt
    fun suggestion(): Attempt
}

class SearchSubmission(
    private val ui: SearchUi,
    private val pause: suspend () -> Unit = { delay(250) },
    private val polls: Int = 40,
    private val progress: (Stage, StageResult) -> Unit = { _, _ -> }
) {
    private suspend fun waitFor(stage: Stage, check: () -> StageResult): StageResult {
        var lastReason = "لم تصبح المرحلة جاهزة"
        repeat(polls) {
            coroutineContext.ensureActive()
            val result = check()
            if (result.status != StageStatus.NOT_READY) { progress(stage, result); return result }
            lastReason = result.reason
            pause()
        }
        coroutineContext.ensureActive()
        return StageResult(StageStatus.NOT_READY, "انتهت مهلة الانتظار: $lastReason").also { progress(stage, it) }
    }
    private fun outcome(stage: Stage, result: StageResult): SubmissionOutcome {
        val status = if (result.status == StageStatus.UNCONFIRMED) SubmissionResult.UNCONFIRMED else SubmissionResult.FAILED
        return SubmissionOutcome(status, stage, result.reason)
    }
    private suspend fun observe(): SearchState {
        repeat(polls) {
            coroutineContext.ensureActive()
            if (ui.state() == SearchState.RESULTS) return SearchState.RESULTS
            pause()
        }
        coroutineContext.ensureActive()
        return ui.state()
    }
    suspend fun run(): SubmissionOutcome {
        val opening = waitFor(Stage.SEARCH_UI, ui::openSearch)
        if (opening.status != StageStatus.READY) return outcome(Stage.SEARCH_UI, opening)
        val field = waitFor(Stage.FIELD, ui::discoverField)
        if (field.status != StageStatus.READY) return outcome(Stage.FIELD, field)
        coroutineContext.ensureActive()
        val focusRequest = ui.requestFocus().also { progress(Stage.FOCUS, it) }
        if (focusRequest.status != StageStatus.READY) return outcome(Stage.FOCUS, focusRequest)
        val focused = waitFor(Stage.FOCUS, ui::verifyFocus)
        if (focused.status != StageStatus.READY) return outcome(Stage.FOCUS, focused)
        coroutineContext.ensureActive()
        val entered = ui.enterText().also { progress(Stage.TEXT_ENTRY, it) }
        if (entered.status != StageStatus.READY) return outcome(Stage.TEXT_ENTRY, entered)
        val verified = waitFor(Stage.TEXT_VERIFICATION, ui::verifyText)
        if (verified.status != StageStatus.READY) {
            // The single text action may still complete asynchronously after a timeout.
            return SubmissionOutcome(SubmissionResult.UNCONFIRMED, Stage.TEXT_VERIFICATION, verified.reason)
        }
        coroutineContext.ensureActive()
        val ime = ui.ime()
        progress(Stage.SUBMISSION, StageResult(if (ime == Attempt.ACCEPTED) StageStatus.UNCONFIRMED else StageStatus.NOT_READY,
            when (ime) { Attempt.ACCEPTED -> "قُبل إجراء لوحة المفاتيح؛ ننتظر النتيجة"; Attempt.UNAVAILABLE -> "إجراء لوحة المفاتيح غير متاح"; Attempt.FAILED -> "تعذر تأكيد إجراء لوحة المفاتيح" }))
        val state = observe()
        if (state == SearchState.RESULTS) return SubmissionOutcome(SubmissionResult.VERIFIED, Stage.RESULTS, "تم التحقق من شاشة النتائج")
        // Never dispatch a second submission if an IME action was accepted or failed ambiguously.
        if (ime != Attempt.UNAVAILABLE) return SubmissionOutcome(SubmissionResult.UNCONFIRMED, Stage.RESULTS, "لم يمكن تأكيد النتائج؛ تحقق قبل إعادة المحاولة")
        if (state != SearchState.SUGGESTIONS) return SubmissionOutcome(SubmissionResult.FAILED, Stage.SUBMISSION, "لم يظهر اقتراح فريد مطابق")
        coroutineContext.ensureActive()
        if (ui.state() != SearchState.SUGGESTIONS) return SubmissionOutcome(SubmissionResult.UNCONFIRMED, Stage.SUBMISSION, "تغيرت الشاشة قبل الاختيار")
        coroutineContext.ensureActive()
        if (ui.suggestion() != Attempt.ACCEPTED) return SubmissionOutcome(SubmissionResult.UNCONFIRMED, Stage.SUBMISSION, "تعذر تأكيد اختيار الاقتراح")
        return if (observe() == SearchState.RESULTS) SubmissionOutcome(SubmissionResult.VERIFIED, Stage.RESULTS, "تم التحقق من شاشة النتائج")
        else SubmissionOutcome(SubmissionResult.UNCONFIRMED, Stage.RESULTS, "لم يمكن تأكيد النتائج؛ تحقق قبل إعادة المحاولة")
    }
}
