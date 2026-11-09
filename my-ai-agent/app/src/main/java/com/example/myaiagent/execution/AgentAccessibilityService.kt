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
        fun searchUi(packageName: String, query: String, isCurrent: () -> Boolean): SearchUi = object : SearchUi {
            override fun focusAndEnter() = isCurrent() && instance?.focusAndEnter(packageName, query) == true
            override fun state() = if (isCurrent()) instance?.searchState(packageName, query) ?: SearchState.OTHER else SearchState.OTHER
            override fun ime() = if (isCurrent()) instance?.submitIme(packageName, query) ?: Attempt.FAILED else Attempt.FAILED
            override fun suggestion() = if (isCurrent()) instance?.submitSuggestion(packageName, query, isCurrent) ?: Attempt.FAILED else Attempt.FAILED
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
    private fun snapshot(node: AccessibilityNodeInfo): SuggestionNode = SuggestionNode(
        text = if (!node.isPassword && !node.isEditable) node.text?.toString() else null,
        description = if (!node.isPassword && !node.isEditable) node.contentDescription?.toString() else null,
        visible = node.isVisibleToUser, enabled = node.isEnabled, clickable = node.isClickable,
        editable = node.isEditable, password = node.isPassword,
        children = (0 until node.childCount).map { node.getChild(it)?.let(::snapshot) ?: SuggestionNode(visible = false, enabled = false) }
    )
    private fun queryEditor(root: AccessibilityNodeInfo, query: String): Boolean = nodes(root).any {
        it.isVisibleToUser && it.isEditable && !it.isPassword && isSearchField(it) && it.text?.toString() == query
    }
    private fun searchState(expected: String, query: String): SearchState {
        val root = activeRoot(expected) ?: return SearchState.OTHER
        if (queryEditor(root, query)) {
            if (SuggestionMatcher.find(snapshot(root), query) != null) return SearchState.SUGGESTIONS
            return SearchState.EDITOR
        }
        // Require a result list and query label after the editor has disappeared; keyboard dismissal alone is insufficient.
        val all = nodes(root)
        val label = all.any { !it.isEditable && !it.isPassword && it.isVisibleToUser &&
            (it.text?.toString() == query || it.contentDescription?.toString() == query) }
        val resultMarker = all.any {
            val id = it.viewIdResourceName?.substringAfterLast('/')?.lowercase().orEmpty()
            val description = it.contentDescription?.toString()?.let(SuggestionMatcher::normalize).orEmpty()
            it.isVisibleToUser && (id == "search_results" || id == "search_results_list" ||
                description == "search results" || description == SuggestionMatcher.normalize("نتائج البحث"))
        }
        return if (label && resultMarker && all.any { it.isVisibleToUser && it.isScrollable } &&
            SuggestionMatcher.find(snapshot(root), query) == null) SearchState.RESULTS else SearchState.OTHER
    }
    private fun submitIme(expected: String, query: String): Attempt {
        val root = activeRoot(expected) ?: return Attempt.FAILED
        val field = focusedSearch(root, query) ?: return Attempt.FAILED
        if (Build.VERSION.SDK_INT < 30 || field.actionList.none { it.id == AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id }) return Attempt.UNAVAILABLE
        return if (field.performAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id)) Attempt.ACCEPTED else Attempt.FAILED
    }
    private fun submitSuggestion(expected: String, query: String, isCurrent: () -> Boolean): Attempt {
        if (!isCurrent()) return Attempt.FAILED
        val root = activeRoot(expected) ?: return Attempt.FAILED
        if (!queryEditor(root, query)) return Attempt.FAILED
        val path = SuggestionMatcher.find(snapshot(root), query) ?: return Attempt.UNAVAILABLE
        // Discard the first tree; reacquire and compare the target just before clicking.
        val fresh = activeRoot(expected) ?: return Attempt.FAILED
        if (!isCurrent() || !queryEditor(fresh, query) || SuggestionMatcher.find(snapshot(fresh), query) != path) return Attempt.FAILED
        var target = fresh
        for (index in path) target = target.getChild(index) ?: return Attempt.FAILED
        if (!target.refresh() || !target.isVisibleToUser || !target.isEnabled || !target.isClickable ||
            target.packageName?.toString() != expected || !isCurrent()) return Attempt.FAILED
        if (SuggestionMatcher.find(SuggestionNode(children = listOf(snapshot(target))), query) != listOf(0)) return Attempt.FAILED
        val finalRoot = activeRoot(expected) ?: return Attempt.FAILED
        if (!isCurrent() || !queryEditor(finalRoot, query) || SuggestionMatcher.find(snapshot(finalRoot), query) != path) return Attempt.FAILED
        return if (target.performAction(AccessibilityNodeInfo.ACTION_CLICK)) Attempt.ACCEPTED else Attempt.FAILED
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
