package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class DiagLogTest {
    @get:Rule val tmp = TemporaryFolder()

    private fun log(
        maxFileBytes: Long = 1_000_000,
        keep: Int = 5,
        clock: () -> Long = {
            1_000L
        },
    ) = DiagLog(tmp.newFolder(), maxFileBytes, keep, clock)

    @Test fun writesOneJsonLinePerEntryWithFields() {
        val l = log()
        l.write("I", "sensors", "start", mapOf("interval_ms" to 5000, "ok" to true, "name" to "gps"))
        val line =
            l
                .files()
                .single()
                .readLines()
                .single()
        assertEquals("""{"t":1000,"lvl":"I","tag":"sensors","msg":"start","interval_ms":5000,"ok":true,"name":"gps"}""", line)
    }

    @Test fun escapesQuotesNewlinesAndControlCharacters() {
        val l = log()
        l.write("E", "x", "he said \"hi\"\nnext\ttab\\ \u0001")
        val line =
            l
                .files()
                .single()
                .readLines()
                .single() // a newline in a message must not break the line format
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
        DiagLog(dir, 1_000_000, 5, clock = { 1L }).write("I", "a", "one")
        DiagLog(dir, 1_000_000, 5, clock = { 2L }).write("I", "a", "two")
        assertEquals(2, DiagLog(dir, 1_000_000, 5, clock = { 3L }).files().sumOf { it.readLines().size })
        // an unwritable location must not crash the app
        DiagLog(File(dir, "file-not-dir").apply { writeText("x") }, 1000, 3, clock = { 4L }).write("I", "a", "ignored")
    }

    private fun DiagLog.onlyLine() = files().single().readLines().single()

    @Test fun infoAndWarnWriteTheirLevel() {
        val l = log()
        l.info("a", "one")
        l.warn("b", "two", mapOf("n" to 1))
        val lines = l.files().single().readLines()
        assertEquals("""{"t":1000,"lvl":"I","tag":"a","msg":"one"}""", lines[0])
        assertEquals("""{"t":1000,"lvl":"W","tag":"b","msg":"two","n":1}""", lines[1])
    }

    @Test fun errorWithoutAThrowableHasNoStack() {
        val l = log()
        l.error("net", "gone", fields = mapOf("code" to 7))
        assertEquals("""{"t":1000,"lvl":"E","tag":"net","msg":"gone","code":7}""", l.onlyLine())
    }

    @Test fun fieldValuesAreWrittenAsJson() {
        val l = log()
        val fields =
            mapOf(
                "none" to null,
                "long" to 12_345_678_901L,
                "double" to 1.5,
                "float" to 2.5f,
                "nan" to Double.NaN,
                "inf" to Double.POSITIVE_INFINITY,
                "fnan" to Float.NaN,
                "list" to listOf(1, 2),
                "cr" to "a\rb",
            )
        l.write("I", "t", "m", fields)
        assertEquals(
            """{"t":1000,"lvl":"I","tag":"t","msg":"m","none":null,"long":12345678901,"double":1.5,"float":2.5,""" +
                """"nan":null,"inf":null,"fnan":null,"list":"[1, 2]","cr":"a\rb"}""",
            l.onlyLine(),
        )
    }

    @Test fun theAppLogDropsEntriesBeforeInit() {
        // Diag is never initialised in unit tests: every call must be a silent no-op, not a crash.
        Diag.info("t", "m", "k" to 1)
        Diag.warn("t", "m")
        Diag.error("t", "m", IllegalStateException("x"))
        Diag.failure("save", IllegalStateException("x"))
    }

    @Test fun aSecondLogWithItsOwnPrefixRotatesOnItsOwnFiles() {
        val dir = tmp.newFolder()
        val main = DiagLog(dir, 1_000_000, 5, { 1L })
        val raw = DiagLog(java.io.File(dir, "raw"), maxFileBytes = 120, keep = 2, clock = { 1L }, prefix = "raw")
        main.write("I", "a", "b")
        repeat(10) { raw.write("I", "rawfix", "", mapOf("tf" to it)) }
        assertEquals(listOf("diag-0001.jsonl"), main.files().map { it.name })
        assertEquals(2, raw.files().size)
        assertTrue(raw.files().all { it.name.matches(Regex("raw-\\d{4}\\.jsonl")) })
    }
}
