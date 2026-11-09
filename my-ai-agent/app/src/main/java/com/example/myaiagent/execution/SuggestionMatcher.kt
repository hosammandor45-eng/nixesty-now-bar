package com.example.myaiagent.execution

import java.text.Normalizer
import java.util.Locale

/** Minimal, immutable tree; text from password nodes is never copied. */
data class SuggestionNode(
    val text: String? = null,
    val description: String? = null,
    val visible: Boolean = true,
    val enabled: Boolean = true,
    val clickable: Boolean = false,
    val editable: Boolean = false,
    val password: Boolean = false,
    val children: List<SuggestionNode> = emptyList()
)

object SuggestionMatcher {
    fun normalize(value: String): String = Normalizer.normalize(value, Normalizer.Form.NFKD)
        .replace(Regex("\\p{M}+"), "").replace('أ', 'ا').replace('إ', 'ا').replace('آ', 'ا')
        .replace('ٱ', 'ا').replace('ى', 'ي').replace("ـ", "")
        .lowercase(Locale.ROOT).trim().replace(Regex("\\s+"), " ")

    /** Return a child-index path to a unique row, never the root or a broad container. */
    fun find(root: SuggestionNode, query: String): List<Int>? {
        val needle = normalize(query)
        if (needle.isEmpty()) return null
        val matches = mutableListOf<Pair<List<Int>, List<SuggestionNode>>>()
        fun visit(node: SuggestionNode, path: List<Int>, chain: List<SuggestionNode>) {
            if (!node.visible || !node.enabled || node.password || node.editable) return
            val labels = listOfNotNull(node.text, node.description).map(::normalize).filter { it.isNotEmpty() }.distinct()
            if (needle in labels) matches.add(path to (chain + node))
            node.children.forEachIndexed { i, child -> visit(child, path + i, chain + node) }
        }
        // Editors are skipped, but their sibling suggestions are included.
        visit(root, emptyList(), emptyList())
        if (matches.size != 1) return null
        val (path, chain) = matches.single()
        // At most two levels above the matching label; the root is never a row.
        for (depth in path.size downTo maxOf(1, path.size - 2)) {
            val candidate = chain[depth]
            if (candidate.clickable && candidate.enabled && candidate.visible) {
                // A row must not contain another editable or clickable descendant.
                fun safe(n: SuggestionNode, isRoot: Boolean): Boolean =
                    !n.editable && !n.password && (isRoot || !n.clickable) &&
                        listOfNotNull(n.text, n.description).map(::normalize).all { it.isEmpty() || it == needle } &&
                        n.children.all { safe(it, false) }
                return if (safe(candidate, true)) path.take(depth) else null
            }
        }
        return null
    }
}
