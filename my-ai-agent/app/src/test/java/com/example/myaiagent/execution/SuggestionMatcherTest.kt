package com.example.myaiagent.execution

import org.junit.Assert.*
import org.junit.Test

class SuggestionMatcherTest {
    private val query = "جيلي سيتي راي"
    private fun label(s: String) = SuggestionNode(text = s)
    private fun row(s: String) = SuggestionNode(clickable = true, children = listOf(label(s)))
    private fun tree(vararg rows: SuggestionNode) = SuggestionNode(children = rows.toList())
    @Test fun exactAndArabicNormalization() {
        assertEquals(listOf(0), SuggestionMatcher.find(tree(row("جِيلي  سيتي راي")), query))
        assertEquals(listOf(0), SuggestionMatcher.find(tree(SuggestionNode(clickable = true, children = listOf(SuggestionNode(description = query)))), query))
    }
    @Test fun missingPartialAndDuplicatesAreRejected() {
        assertNull(SuggestionMatcher.find(tree(row("جيلي سيتي")), query))
        assertNull(SuggestionMatcher.find(tree(row("جيلي سيتي راي فيديو")), query))
        assertNull(SuggestionMatcher.find(tree(row(query), row(query)), query))
        assertNull(SuggestionMatcher.find(tree(row("غير ذلك")), query))
    }
    @Test fun boundedClickableParentAndUnsafeRows() {
        assertNull(SuggestionMatcher.find(tree(label(query)), query))
        assertNull(SuggestionMatcher.find(tree(SuggestionNode(clickable = true, children = listOf(label(query), label("unrelated")))), query))
        assertNull(SuggestionMatcher.find(SuggestionNode(clickable = true, children = listOf(label(query))), query))
        assertNull(SuggestionMatcher.find(tree(SuggestionNode(clickable = true, children = listOf(label(query), SuggestionNode(clickable = true)))), query))
        assertNull(SuggestionMatcher.find(tree(SuggestionNode(clickable = true, enabled = false, children = listOf(label(query)))), query))
        assertNull(SuggestionMatcher.find(tree(SuggestionNode(clickable = true, children = listOf(SuggestionNode(text = query, password = true)))), query))
    }
}
