package com.example.myaiagent.command

import org.junit.Assert.*
import org.junit.Test

class CommandParserTest {
    @Test fun parsesNavigationAndScroll() {
        assertEquals(listOf(Command.Back), CommandParser.parse(" رجوع "))
        assertEquals(listOf(Command.Back), CommandParser.parse("عودة"))
        assertEquals(listOf(Command.Home), CommandParser.parse("الرئيسية"))
        assertEquals(listOf(Command.Scroll(true)), CommandParser.parse("تمرير للأسفل"))
        assertEquals(listOf(Command.Scroll(false)), CommandParser.parse("تمرير للأعلى"))
    }
    @Test fun preservesArguments() {
        assertEquals(listOf(Command.OpenApp("الخرائط")), CommandParser.parse("افتح الخرائط"))
        assertEquals(listOf(Command.Tap("إرسال")), CommandParser.parse("اضغط إرسال"))
        assertEquals(listOf(Command.EnterText("مرحبا  بالعالم")), CommandParser.parse("اكتب مرحبا  بالعالم"))
    }
    @Test fun parsesOpenAndSearchAsOrderedActions() {
        assertEquals(listOf(Command.OpenApp("يوتيوب"), Command.Search("جيلي سيتي راي")), CommandParser.parse("افتح يوتيوب وابحث عن جيلي سيتي راي"))
        assertEquals(listOf(Command.OpenApp("YouTube"), Command.Search("a وابحث عن b")), CommandParser.parse("افتح YouTube وابحث عن a وابحث عن b"))
    }
    @Test fun rejectsIncompleteAndUnrecognizedCommands() {
        listOf("", "افتح", "افتح   ", "اضغط ", "اكتب", "رجوع الآن", "احذف كل شيء", "افتح يوتيوب وابحث عن ", "افتح يوتيوب وابحث").forEach {
            assertNull(it, CommandParser.parse(it))
        }
    }
    @Test fun normalizesArabicAndEnglishNames() {
        assertEquals(NameMatcher.normalize("الإِعْدَادَات"), NameMatcher.normalize("الاعدادات"))
        assertEquals("chrome", NameMatcher.normalize("  ChRoMe  "))
        assertEquals(NameMatcher.normalize("مدير الملفات"), NameMatcher.normalize("  مُدِير   المَلَفَّات "))
    }
    @Test fun resolvesBilingualAliasGroups() {
        listOf("الإعدادات" to "settings", "يوتيوب" to "YouTube", "كروم" to "Chrome", "واتساب" to "WhatsApp", "الكاميرا" to "Camera", "المعرض" to "Gallery", "الخرائط" to "Maps", "الملفات" to "Files").forEach { (arabic, english) ->
            assertTrue("$arabic / $english", NameMatcher.normalize(english) in NameMatcher.equivalentNames(arabic))
        }
        assertTrue(NameMatcher.isSettings("الإِعْدَادَات"))
        assertFalse(NameMatcher.isSettings("Camera"))
        assertTrue("com.google.android.youtube" in NameMatcher.knownPackages("يُوتِيُوب"))
        assertTrue("com.whatsapp" in NameMatcher.knownPackages("واتساب"))
        assertTrue(NameMatcher.knownPackages("Unknown").isEmpty())
        assertEquals(setOf("my app"), NameMatcher.equivalentNames(" My   App "))
    }
}
