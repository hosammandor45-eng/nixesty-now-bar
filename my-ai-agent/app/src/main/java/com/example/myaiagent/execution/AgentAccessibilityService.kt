package com.example.myaiagent.execution

import android.accessibilityservice.AccessibilityService
import android.os.Bundle
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import com.example.myaiagent.command.Command
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

class AgentAccessibilityService : AccessibilityService() {
    companion object {
        private var instance: AgentAccessibilityService? = null
        private val _connected = MutableStateFlow(false)
        val connected = _connected.asStateFlow()
        fun execute(command: Command): String = instance?.perform(command) ?: "الخدمة غير متصلة. فعّلها من الإعدادات."
    }

    override fun onServiceConnected() { instance = this; _connected.value = true }
    override fun onDestroy() { if (instance === this) { instance = null; _connected.value = false }; super.onDestroy() }
    override fun onAccessibilityEvent(event: AccessibilityEvent?) = Unit
    override fun onInterrupt() = Unit

    private fun perform(command: Command): String {
        val success = when (command) {
            Command.Back -> performGlobalAction(GLOBAL_ACTION_BACK)
            Command.Home -> performGlobalAction(GLOBAL_ACTION_HOME)
            is Command.Scroll -> withRoot { root -> scroll(root, command.forward) }
            is Command.Tap -> withRoot { root -> tap(root, command.label) }
            is Command.EnterText -> withRoot { root ->
                val focused = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                focused != null && focused.isEditable && !focused.isPassword && focused.performAction(
                    AccessibilityNodeInfo.ACTION_SET_TEXT,
                    Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, command.text) }
                )
            }
            is Command.OpenApp -> false
        }
        return if (success) "تم إرسال الإجراء بنجاح." else "تعذر تنفيذ الإجراء على الشاشة الحالية. تحقق من العنصر أو التطبيق."
    }

    private inline fun withRoot(action: (AccessibilityNodeInfo) -> Boolean): Boolean {
        val root = rootInActiveWindow ?: return false
        // Never operate on this app's own command screen.
        if (root.packageName?.toString() == packageName) return false
        return action(root)
    }

    private fun scroll(node: AccessibilityNodeInfo, forward: Boolean): Boolean {
        val action = if (forward) AccessibilityNodeInfo.ACTION_SCROLL_FORWARD else AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
        if (node.isScrollable && node.performAction(action)) return true
        for (i in 0 until node.childCount) if (node.getChild(i)?.let { scroll(it, forward) } == true) return true
        return false
    }

    private fun tap(root: AccessibilityNodeInfo, label: String): Boolean {
        val matches = root.findAccessibilityNodeInfosByText(label).filter {
            it.text?.toString() == label || it.contentDescription?.toString() == label
        }
        // Refuse ambiguous matches; do not guess which control the user meant.
        if (matches.size != 1) return false
        var node: AccessibilityNodeInfo? = matches.single()
        while (node != null) {
            if (node.isClickable) return node.performAction(AccessibilityNodeInfo.ACTION_CLICK)
            node = node.parent
        }
        return false
    }
}
