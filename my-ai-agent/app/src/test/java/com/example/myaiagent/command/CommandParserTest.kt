package com.example.myaiagent.command

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CommandParserTest {
    @Test fun parsesNavigationAndScroll() {
        assertEquals(Command.Back, CommandParser.parse(" رجوع "))
        assertEquals(Command.Back, CommandParser.parse("عودة"))
        assertEquals(Command.Home, CommandParser.parse("الرئيسية"))
        assertEquals(Command.Scroll(true), CommandParser.parse("تمرير للأسفل"))
        assertEquals(Command.Scroll(false), CommandParser.parse("تمرير للأعلى"))
    }
    @Test fun preservesArguments() {
        assertEquals(Command.OpenApp("الخرائط"), CommandParser.parse("افتح الخرائط"))
        assertEquals(Command.Tap("إرسال"), CommandParser.parse("اضغط إرسال"))
        assertEquals(Command.EnterText("مرحبا  بالعالم"), CommandParser.parse("اكتب مرحبا  بالعالم"))
    }
    @Test fun rejectsIncompleteAndUnrecognizedCommands() {
        listOf("", "افتح", "افتح   ", "اضغط ", "اكتب", "رجوع الآن", "احذف كل شيء").forEach {
            assertNull(CommandParser.parse(it))
        }
    }
}
