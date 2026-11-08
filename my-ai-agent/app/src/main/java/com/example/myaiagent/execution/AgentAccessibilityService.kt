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
        fun searchUi(packageName: String, query: String): SearchUi = object : SearchUi {
            override fun focusAndEnter() = instance?.focusAndEnter(packageName, query) == true
            override fun state() = instance?.searchState(packageName, query) ?: SearchState.OTHER
            override fun ime() = instance?.submitIme(packageName, query) ?: Attempt.FAILED
            override fun button() = instance?.submitButton(packageName, query) ?: Attempt.FAILED
        }
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

    private fun activeRoot(expected: String): AccessibilityNodeInfo? = rootInActiveWindow?.takeIf {
        it.packageName?.toString() == expected && expected != packageName
    }
    private fun focusedSearch(root: AccessibilityNodeInfo, query: String): AccessibilityNodeInfo? =
        root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.takeIf {
            it.isEditable && !it.isPassword && it.isVisibleToUser && isSearchField(it) && it.text?.toString() == query
        }
    private fun focusAndEnter(expected: String, query: String): Boolean {
        val root = activeRoot(expected) ?: return false
        val fields = nodes(root).filter { it.isEditable && !it.isPassword && it.isVisibleToUser && isSearchField(it) }
        if (fields.size != 1) return false
        val field = fields.single()
        if (!field.isFocused && !field.performAction(AccessibilityNodeInfo.ACTION_FOCUS)) return false
        val fresh = activeRoot(expected) ?: return false
        val focused = fresh.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return false
        if (!focused.isEditable || focused.isPassword || !isSearchField(focused)) return false
        if (!setText(focused, query)) return false
        return activeRoot(expected)?.let { focusedSearch(it, query) } != null
    }
    private fun searchState(expected: String, query: String): SearchState {
        val root = activeRoot(expected) ?: return SearchState.OTHER
        if (focusedSearch(root, query) != null) return SearchState.EDITOR
        val focus = root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
        if (focus != null && focus.isEditable && !focus.isPassword && isSearchField(focus)) return SearchState.OTHER
        // Require query text on the target screen after leaving the focused editor.
        return if (nodes(root).any { !it.isEditable && !it.isPassword && it.isVisibleToUser &&
                (it.text?.toString() == query || it.contentDescription?.toString() == query) }) SearchState.RESULTS else SearchState.OTHER
    }
    private fun submitIme(expected: String, query: String): Attempt {
        val root = activeRoot(expected) ?: return Attempt.FAILED
        val field = focusedSearch(root, query) ?: return Attempt.FAILED
        if (Build.VERSION.SDK_INT < 30 || field.actionList.none { it.id == AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id }) return Attempt.UNAVAILABLE
        return if (field.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id)) Attempt.ACCEPTED else Attempt.FAILED
    }
    private fun submitButton(expected: String, query: String): Attempt {
        val root = activeRoot(expected) ?: return Attempt.FAILED
        if (focusedSearch(root, query) == null) return Attempt.FAILED
        val controls = matching(root, searchLabels + setOf("Submit", "إرسال"))
        return if (controls.size == 1 && click(controls.single())) Attempt.ACCEPTED else Attempt.UNAVAILABLE
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
