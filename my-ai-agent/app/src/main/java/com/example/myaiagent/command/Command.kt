package com.example.myaiagent.command

sealed interface Command {
    data class OpenApp(val name: String) : Command
    data object Back : Command
    data object Home : Command
    data class Scroll(val forward: Boolean) : Command
    data class Tap(val label: String) : Command
    data class EnterText(val text: String) : Command
}

/** Exact grammar: no fuzzy matching, interpretation, or network access. */
object CommandParser {
    fun parse(input: String): Command? {
        val value = input.trim()
        return when (value) {
            "رجوع", "عودة" -> Command.Back
            "الرئيسية" -> Command.Home
            "تمرير للأسفل" -> Command.Scroll(true)
            "تمرير للأعلى" -> Command.Scroll(false)
            else -> parseArgument(value, "افتح ") { Command.OpenApp(it) }
                ?: parseArgument(value, "اضغط ") { Command.Tap(it) }
                ?: parseArgument(value, "اكتب ") { Command.EnterText(it) }
        }
    }

    private fun parseArgument(value: String, prefix: String, factory: (String) -> Command): Command? {
        if (!value.startsWith(prefix)) return null
        val argument = value.removePrefix(prefix).trim()
        return if (argument.isEmpty()) null else factory(argument)
    }
}
