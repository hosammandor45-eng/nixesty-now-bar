package com.example.myaiagent.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.example.myaiagent.command.Command
import com.example.myaiagent.command.CommandParser
import com.example.myaiagent.execution.ActionResult
import com.example.myaiagent.execution.AgentAccessibilityService
import com.example.myaiagent.execution.AppChoice
import com.example.myaiagent.execution.AppLauncher
import com.example.myaiagent.execution.SearchSubmission
import com.example.myaiagent.execution.SubmissionResult
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

class AgentViewModel(application: Application) : AndroidViewModel(application) {
    private val _log = MutableStateFlow<List<String>>(emptyList())
    val log = _log.asStateFlow()
    private val _pending = MutableStateFlow<List<Command>?>(null)
    val pending = _pending.asStateFlow()
    private val _choices = MutableStateFlow<List<AppChoice>>(emptyList())
    val choices = _choices.asStateFlow()
    private val _running = MutableStateFlow(false)
    val running = _running.asStateFlow()
    private var selection: CompletableDeferred<AppChoice?>? = null
    private var job: Job? = null

    fun submit(input: String) {
        if (job != null || _running.value || _pending.value != null) return
        val commands = CommandParser.parse(input)
        if (commands == null) { record("أمر غير معروف أو ناقص. راجع الأمثلة."); return }
        if (commands.any { it is Command.Tap || it is Command.EnterText || it is Command.Search }) _pending.value = commands
        else execute(commands)
    }
    fun confirm() { val commands = _pending.value ?: return; _pending.value = null; execute(commands) }
    fun dismiss() { _pending.value = null; record("أُلغي الإجراء.") }
    fun choose(choice: AppChoice?) { selection?.complete(choice) }
    fun stop() {
        job?.cancel(); selection = null; _choices.value = emptyList()
        _pending.value = null
        if (job == null) _running.value = false
        record("تم الإيقاف. لن يُنفذ أي إجراء قيد الانتظار.")
    }
    private fun execute(commands: List<Command>) {
        if (commands.any { it !is Command.OpenApp } && !AgentAccessibilityService.connected.value) {
            record("الخدمة غير متصلة. فعّلها أولاً."); return
        }
        _running.value = true
        job = viewModelScope.launch(start = CoroutineStart.LAZY) {
            try {
                var targetPackage: String? = null
                for ((index, command) in commands.withIndex()) {
                    record("الخطوة ${index + 1} من ${commands.size} قيد التنفيذ.")
                    val success = when (command) {
                        is Command.OpenApp -> {
                            val matches = AppLauncher.resolve(getApplication(), command.name)
                            val choice = when (matches.size) {
                                0 -> null
                                1 -> matches.single()
                                else -> {
                                    val deferred = CompletableDeferred<AppChoice?>()
                                    selection = deferred; _choices.value = matches
                                    record("اختر التطبيق المطلوب خلال دقيقة.")
                                    val selected = withTimeoutOrNull(60_000) { deferred.await() }
                                    selection = null; _choices.value = emptyList(); selected
                                }
                            }
                            if (choice == null) false else {
                                targetPackage = choice.packageName
                                AppLauncher.open(getApplication(), choice)
                            }
                        }
                        is Command.Search -> {
                            val target = targetPackage
                            if (target == null) false else {
                                record("انتظار شاشة البحث في التطبيق المفتوح…")
                                awaitReady { AgentAccessibilityService.openSearch(target) } &&
                                    when (SearchSubmission(AgentAccessibilityService.searchUi(target, command.query)).run()) {
                                        SubmissionResult.VERIFIED -> true
                                        SubmissionResult.FAILED -> { record("تعذر إرسال البحث؛ تحقق من حقل البحث وزره."); false }
                                        SubmissionResult.UNCONFIRMED -> { record("تعذر تأكيد ظهور النتائج. ربما أُرسل البحث؛ تحقق قبل إعادة المحاولة."); false }
                                    }
                            }
                        }
                        else -> {
                            record("سيُنفذ الإجراء بعد 5 ثوانٍ. انتقل إلى التطبيق المطلوب أو اضغط إيقاف.")
                            delay(5_000)
                            AgentAccessibilityService.execute(command) == ActionResult.SUCCESS
                        }
                    }
                    if (!success) {
                        record("توقفت السلسلة: تعذر التحقق من الشاشة أو تنفيذ الخطوة. لم تُنفذ الخطوات المتبقية.")
                        return@launch
                    }
                    record("اكتملت الخطوة ${index + 1} من ${commands.size}.")
                }
            } finally {
                selection = null; _choices.value = emptyList(); _running.value = false; job = null
            }
        }
        job?.start()
    }
    private suspend fun awaitReady(action: () -> ActionResult): Boolean = withTimeoutOrNull(10_000) {
        while (true) {
            when (action()) {
                ActionResult.SUCCESS -> return@withTimeoutOrNull true
                ActionResult.FAILURE -> return@withTimeoutOrNull false
                ActionResult.NOT_READY -> delay(250)
            }
        }
        @Suppress("UNREACHABLE_CODE") false
    } ?: false
    private fun record(message: String) { _log.value = (listOf(message) + _log.value).take(50) }
}
