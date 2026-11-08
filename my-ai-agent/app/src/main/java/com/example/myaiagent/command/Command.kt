package com.example.myaiagent.command

import java.text.Normalizer
import java.util.Locale

sealed interface Command {
    data class OpenApp(val name: String) : Command
    data object Back : Command
    data object Home : Command
    data class Scroll(val forward: Boolean) : Command
    data class Tap(val label: String) : Command
    data class EnterText(val text: String) : Command
    data class Search(val query: String) : Command
}

/** Normalization is only used for matching, never for text entered into another app. */
object NameMatcher {
    fun normalize(value: String): String = Normalizer.normalize(value, Normalizer.Form.NFKD)
        .replace(Regex("\\p{M}+"), "")
        .replace('أ', 'ا').replace('إ', 'ا').replace('آ', 'ا').replace('ٱ', 'ا')
        .replace('ى', 'ي').replace('ة', 'ه').replace("ـ", "")
        .lowercase(Locale.ROOT).trim().replace(Regex("\\s+"), " ")

    private val aliases = listOf(
        setOf("settings", "الإعدادات", "الاعدادات", "ضبط"),
        setOf("youtube", "you tube", "يوتيوب"),
        setOf("chrome", "كروم", "جوجل كروم"),
        setOf("whatsapp", "whats app", "واتساب", "واتس اب"),
        setOf("camera", "الكاميرا", "كاميرا"),
        setOf("gallery", "photos", "المعرض", "الصور", "معرض الصور"),
        setOf("maps", "google maps", "الخرائط", "خرائط", "خرائط جوجل"),
        setOf("files", "file manager", "الملفات", "ملفات", "مدير الملفات")
    ).map { group -> group.map(::normalize).toSet() }

    fun equivalentNames(name: String): Set<String> {
        val normalized = normalize(name)
        return aliases.firstOrNull { normalized in it } ?: setOf(normalized)
    }
    fun knownPackages(name: String): Set<String> = when {
        normalize("youtube") in equivalentNames(name) -> setOf("com.google.android.youtube")
        normalize("chrome") in equivalentNames(name) -> setOf("com.android.chrome")
        normalize("whatsapp") in equivalentNames(name) -> setOf("com.whatsapp", "com.whatsapp.w4b")
        normalize("camera") in equivalentNames(name) -> setOf("com.google.android.GoogleCamera")
        normalize("gallery") in equivalentNames(name) -> setOf("com.google.android.apps.photos")
        normalize("maps") in equivalentNames(name) -> setOf("com.google.android.apps.maps")
        normalize("files") in equivalentNames(name) -> setOf("com.google.android.apps.nbu.files")
        else -> emptySet()
    }
    fun isSettings(name: String): Boolean = normalize("settings") in equivalentNames(name)
}

/** A deliberately small grammar; separators inside a search query remain part of the query. */
object CommandParser {
    private val openAndSearch = Regex("^افتح\\s+(.+?)\\s+وابحث\\s+عن\\s+(.+)$", RegexOption.DOT_MATCHES_ALL)
    fun parse(input: String): List<Command>? {
        val value = input.trim()
        openAndSearch.matchEntire(value)?.let {
            val app = it.groupValues[1].trim()
            val query = it.groupValues[2].trim()
            if (app.isNotEmpty() && query.isNotEmpty()) return listOf(Command.OpenApp(app), Command.Search(query))
        }
        // Reject incomplete composite commands rather than treating them as an app name.
        if (Regex("\\s+وابحث(?:\\s|$)").containsMatchIn(value)) return null
        val command = when (value) {
            "رجوع", "عودة" -> Command.Back
            "الرئيسية" -> Command.Home
            "تمرير للأسفل" -> Command.Scroll(true)
            "تمرير للأعلى" -> Command.Scroll(false)
            else -> parseArgument(value, "افتح ") { Command.OpenApp(it) }
                ?: parseArgument(value, "اضغط ") { Command.Tap(it) }
                ?: parseArgument(value, "اكتب ") { Command.EnterText(it) }
        }
        return command?.let(::listOf)
    }
    private fun parseArgument(value: String, prefix: String, factory: (String) -> Command): Command? {
        if (!value.startsWith(prefix)) return null
        val argument = value.removePrefix(prefix).trim()
        return if (argument.isEmpty()) null else factory(argument)
    }
}
