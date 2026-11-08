package com.example.myaiagent.execution

import android.accessibilityservice.AccessibilityService
import android.os.Build
import android.os.Bundle
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import com.example.myaiagent.command.Command
import com.example.myaiagent.command.NameMatcher
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

enum class ActionResult { SUCCESS, NOT_READY, FAILURE }

class AgentAccessibilityService : AccessibilityService() {
    companion object {
        private var instance: AgentAccessibilityService? = null
        private val _connected = MutableStateFlow(false)
        val connected = _connected.asStateFlow()
        fun execute(command: Command): ActionResult = instance?.perform(command) ?: ActionResult.FAILURE
        fun openSearch(packageName: String): ActionResult = instance?.openSearchControl(packageName) ?: ActionResult.FAILURE
        fun enterSearch(packageName: String, query: String): ActionResult = instance?.setSearchText(packageName, query) ?: ActionResult.FAILURE
        fun submitSearch(packageName: String): ActionResult = instance?.submitSearchText(packageName) ?: ActionResult.FAILURE
    }

    override fun onServiceConnected() { instance = this; _connected.value = true }
    override fun onDestroy() { if (instance === this) { instance = null; _connected.value = false }; super.onDestroy() }
    override fun onAccessibilityEvent(event: AccessibilityEvent?) = Unit
    override fun onInterrupt() = Unit

    private fun perform(command: Command): ActionResult {
        val success = when (command) {
            Command.Back -> performGlobalAction(GLOBAL_ACTION_BACK)
            Command.Home -> performGlobalAction(GLOBAL_ACTION_HOME)
            is Command.Scroll -> withRoot(null) { scroll(it, command.forward) } == ActionResult.SUCCESS
            is Command.Tap -> withRoot(null) { root -> clickUnique(root, setOf(command.label)) } == ActionResult.SUCCESS
            is Command.EnterText -> withRoot(null) { root ->
                root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.let { setText(it, command.text) } == true
            } == ActionResult.SUCCESS
            else -> false
        }
        return if (success) ActionResult.SUCCESS else ActionResult.FAILURE
    }

    private inline fun withRoot(expectedPackage: String?, action: (AccessibilityNodeInfo) -> Boolean): ActionResult {
        val root = rootInActiveWindow ?: return ActionResult.NOT_READY
        val current = root.packageName?.toString() ?: return ActionResult.NOT_READY
        if (current == packageName || (expectedPackage != null && current != expectedPackage)) return ActionResult.NOT_READY
        return if (action(root)) ActionResult.SUCCESS else ActionResult.FAILURE
    }

    private fun openSearchControl(expected: String): ActionResult {
        val root = rootInActiveWindow ?: return ActionResult.NOT_READY
        if (root.packageName?.toString() != expected || expected == packageName) return ActionResult.NOT_READY
        val fields = nodes(root).filter { it.isEditable && !it.isPassword && it.isVisibleToUser }
        if (fields.size == 1 && isSearchField(fields.single())) return ActionResult.SUCCESS
        if (fields.size == 1) return ActionResult.FAILURE
        if (fields.size > 1) return ActionResult.FAILURE
        val controls = matching(root, searchLabels)
        if (controls.isEmpty()) return ActionResult.NOT_READY
        return if (controls.size == 1 && click(controls.single())) ActionResult.SUCCESS else ActionResult.FAILURE
    }

    private fun setSearchText(expected: String, query: String): ActionResult {
        val root = rootInActiveWindow ?: return ActionResult.NOT_READY
        if (root.packageName?.toString() != expected || expected == packageName) return ActionResult.NOT_READY
        val fields = nodes(root).filter { it.isEditable && !it.isPassword && it.isVisibleToUser }
        if (fields.isEmpty()) return ActionResult.NOT_READY
        if (fields.size != 1 || !isSearchField(fields.single())) return ActionResult.FAILURE
        return if (setText(fields.single(), query)) ActionResult.SUCCESS else ActionResult.FAILURE
    }

    private fun submitSearchText(expected: String): ActionResult {
        return withRoot(expected) { root ->
            val fields = nodes(root).filter { it.isEditable && !it.isPassword && it.isVisibleToUser }
            if (fields.size != 1 || !isSearchField(fields.single())) return@withRoot false
            val field = fields.single()
            // Only invoke an IME action explicitly exposed by this field; never guess coordinates.
            if (Build.VERSION.SDK_INT >= 30 && field.actionList.any { it.id == AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id }) {
                field.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id)
            } else {
                val controls = matching(root, searchLabels)
                controls.size == 1 && click(controls.single())
            }
        }
    }

    private fun setText(node: AccessibilityNodeInfo, text: String): Boolean =
        node.isEditable && !node.isPassword && node.isVisibleToUser && node.performAction(
            AccessibilityNodeInfo.ACTION_SET_TEXT,
            Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, text) }
        )

    private fun nodes(root: AccessibilityNodeInfo): List<AccessibilityNodeInfo> {
        val result = mutableListOf<AccessibilityNodeInfo>()
        fun visit(node: AccessibilityNodeInfo) {
            result.add(node)
            for (i in 0 until node.childCount) node.getChild(i)?.let(::visit)
        }
        visit(root)
        return result
    }
    private fun isSearchField(node: AccessibilityNodeInfo): Boolean {
        val names = searchLabels.map(NameMatcher::normalize).toSet()
        return listOfNotNull(node.hintText, node.contentDescription).any {
            val label = NameMatcher.normalize(it.toString())
            label in names || label.startsWith("search ") || label.startsWith("بحث ") || label.startsWith("البحث ")
        }
    }
    private fun matching(root: AccessibilityNodeInfo, labels: Set<String>): List<AccessibilityNodeInfo> {
        val normalized = labels.map(NameMatcher::normalize).toSet()
        return nodes(root).filter { node ->
            !node.isPassword && !node.isEditable && node.isVisibleToUser && listOfNotNull(node.text, node.contentDescription).any { NameMatcher.normalize(it.toString()) in normalized }
        }
    }
    private fun clickUnique(root: AccessibilityNodeInfo, labels: Set<String>): Boolean {
        val matches = matching(root, labels)
        return matches.size == 1 && click(matches.single())
    }
    private fun click(initial: AccessibilityNodeInfo): Boolean {
        var node: AccessibilityNodeInfo? = initial
        while (node != null) {
            if (node.isClickable && node.isEnabled) return node.performAction(AccessibilityNodeInfo.ACTION_CLICK)
            node = node.parent
        }
        return false
    }
    private fun scroll(node: AccessibilityNodeInfo, forward: Boolean): Boolean {
        val action = if (forward) AccessibilityNodeInfo.ACTION_SCROLL_FORWARD else AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
        if (node.isScrollable && node.performAction(action)) return true
        for (i in 0 until node.childCount) if (node.getChild(i)?.let { scroll(it, forward) } == true) return true
        return false
    }
    private val searchLabels = setOf("بحث", "البحث", "ابحث", "Search")
}
