package com.example.myaiagent.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.example.myaiagent.command.Command
import com.example.myaiagent.command.CommandParser
import com.example.myaiagent.execution.AgentAccessibilityService
import com.example.myaiagent.execution.AppLauncher
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class AgentViewModel(application: Application) : AndroidViewModel(application) {
    private val _log = MutableStateFlow<List<String>>(emptyList())
    val log = _log.asStateFlow()
    private val _pending = MutableStateFlow<Command?>(null)
    val pending = _pending.asStateFlow()
    private val _running = MutableStateFlow(false)
    val running = _running.asStateFlow()
    private var job: Job? = null

    fun submit(input: String) {
        if (_running.value || _pending.value != null) return
        val command = CommandParser.parse(input)
        if (command == null) { record("أمر غير معروف أو ناقص. راجع الأمثلة."); return }
        if (command is Command.Tap || command is Command.EnterText) _pending.value = command
        else execute(command)
    }
    fun confirm() { val command = _pending.value ?: return; _pending.value = null; execute(command) }
    fun dismiss() { _pending.value = null; record("أُلغي الإجراء.") }
    fun stop() {
        job?.cancel(); job = null; _pending.value = null; _running.value = false
        record("تم الإيقاف. لن يُنفذ أي إجراء قيد الانتظار.")
    }
    private fun execute(command: Command) {
        if (command is Command.OpenApp) { record(AppLauncher.open(getApplication(), command)); return }
        if (!AgentAccessibilityService.connected.value) { record("الخدمة غير متصلة. فعّلها أولاً."); return }
        _running.value = true
        record("سيُنفذ الإجراء بعد 5 ثوانٍ. انتقل إلى التطبيق المطلوب أو اضغط إيقاف.")
        job = viewModelScope.launch {
            delay(5_000)
            record(AgentAccessibilityService.execute(command))
            _running.value = false
            job = null
        }
    }
    private fun record(message: String) { _log.value = (listOf(message) + _log.value).take(50) }
}
