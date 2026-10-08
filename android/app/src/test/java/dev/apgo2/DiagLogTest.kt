package dev.apgo2

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class DiagLogTest {
    @get:Rule val tmp = TemporaryFolder()

    private fun log(maxFileBytes: Long = 1_000_000, keep: Int = 5, clock: () -> Long = { 1_000L }) = DiagLog(tmp.newFolder(), maxFileBytes, keep, clock)

    @Test fun writesOneJsonLinePerEntryWithFields() {
        val l = log()
        l.write("I", "sensors", "start", mapOf("interval_ms" to 5000, "ok" to true, "name" to "gps"))
        val line = l.files().single().readLines().single()
        assertEquals("""{"t":1000,"lvl":"I","tag":"sensors","msg":"start","interval_ms":5000,"ok":true,"name":"gps"}""", line)
    }

    @Test fun escapesQuotesNewlinesAndControlCharacters() {
        val l = log()
        l.write("E", "x", "he said \"hi\"\nnext\ttab\\ \u0001")
        val line = l.files().single().readLines().single() // a newline in a message must not break the line format
        assertTrue(line, line.contains("""he said \"hi\"\nnext\ttab\\ \u0001"""))
    }

    @Test fun errorEntriesCarryTheStackTrace() {
        val l = log()
        l.error("net", "connect failed", IllegalStateException("boom"))
        val text = l.files().single().readText()
        assertTrue(text.contains("IllegalStateException: boom"))
        assertTrue(text.contains("\"lvl\":\"E\""))
    }

    @Test fun rotatesWhenAFileIsFullAndKeepsOnlyTheNewest() {
        var t = 0L
        val l = log(maxFileBytes = 200, keep = 3) { ++t }
        repeat(60) { l.write("I", "t", "message number $it padded padded padded") }
        val files = l.files()
        assertEquals(3, files.size)
        val all = files.joinToString("") { it.readText() }
        assertTrue("newest entry survives", all.contains("message number 59"))
        assertTrue("oldest entry is gone", !all.contains("message number 0 "))
        assertTrue(files.all { it.length() < 400 })
    }

    @Test fun reopeningContinuesTheNewestFileAndNeverThrows() {
        val dir = tmp.newFolder()
        DiagLog(dir, 1_000_000, 5) { 1L }.write("I", "a", "one")
        DiagLog(dir, 1_000_000, 5) { 2L }.write("I", "a", "two")
        assertEquals(2, DiagLog(dir, 1_000_000, 5) { 3L }.files().sumOf { it.readLines().size })
        // an unwritable location must not crash the app
        DiagLog(File(dir, "file-not-dir").also { it.writeText("x") }, 1000, 3) { 4L }.write("I", "a", "ignored")
    }
}
