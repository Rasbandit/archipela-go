package dev.apgo2

import java.io.File

/**
 * Rotating JSON-lines diagnostics log kept on the phone (`diag-0001.jsonl`, ...), so a whole outing can be pulled off
 * the device and analysed later. One line per entry: `{"t":epoch_ms,"lvl":"I|W|E","tag":..,"msg":..,<fields>}`.
 * Never throws: a log that cannot be written must not break the app.
 */
class DiagLog(
    private val dir: File,
    private val maxFileBytes: Long = 2_000_000,
    private val keep: Int = 10,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    @Synchronized
    fun write(
        level: String,
        tag: String,
        msg: String,
        fields: Map<String, Any?> = emptyMap(),
    ) {
        runCatching {
            dir.mkdirs()
            val line =
                buildString {
                    append(
                        "{\"t\":",
                    ).append(
                        clock(),
                    ).append(",\"lvl\":")
                        .append(quote(level))
                        .append(",\"tag\":")
                        .append(quote(tag))
                        .append(",\"msg\":")
                        .append(quote(msg))
                    fields.forEach { (k, v) -> append(',').append(quote(k)).append(':').append(value(v)) }
                    append("}\n")
                }
            val bytes = line.toByteArray()
            var target = files().lastOrNull() ?: File(dir, name(1))
            if (target.length() > 0 && target.length() + bytes.size > maxFileBytes) {
                target = File(dir, name(seq(target) + 1))
                files().let { all -> all.take((all.size + 1 - keep).coerceAtLeast(0)).forEach { it.delete() } }
            }
            target.appendBytes(bytes)
        }
    }

    fun info(
        tag: String,
        msg: String,
        fields: Map<String, Any?> = emptyMap(),
    ) = write("I", tag, msg, fields)

    fun warn(
        tag: String,
        msg: String,
        fields: Map<String, Any?> = emptyMap(),
    ) = write("W", tag, msg, fields)

    fun error(
        tag: String,
        msg: String,
        t: Throwable? = null,
        fields: Map<String, Any?> = emptyMap(),
    ) = write("E", tag, msg, if (t == null) fields else fields + ("stack" to t.stackTraceToString()))

    /** Log files, oldest first. */
    fun files(): List<File> = dir.listFiles { f -> f.isFile && PATTERN.matches(f.name) }?.sortedBy { seq(it) } ?: emptyList()

    private fun seq(f: File) = PATTERN.matchEntire(f.name)!!.groupValues[1].toInt()

    private fun name(n: Int) = "diag-%04d.jsonl".format(n)

    private fun value(v: Any?): String =
        when (v) {
            null -> "null"
            is Boolean, is Int, is Long -> v.toString()
            is Double -> if (v.isFinite()) v.toString() else "null"
            is Float -> if (v.isFinite()) v.toString() else "null"
            else -> quote(v.toString())
        }

    private fun quote(s: String): String =
        buildString {
            append('"')
            for (c in s) {
                when {
                    c == '"' -> append("\\\"")
                    c == '\\' -> append("\\\\")
                    c == '\n' -> append("\\n")
                    c == '\r' -> append("\\r")
                    c == '\t' -> append("\\t")
                    c < ' ' -> append("\\u%04x".format(c.code))
                    else -> append(c)
                }
            }
            append('"')
        }

    companion object {
        private val PATTERN = Regex("diag-(\\d{4})\\.jsonl")
    }
}

/** The app-wide log. Safe to call before [init] (entries are dropped). */
object Diag {
    @Volatile private var log: DiagLog? = null

    /** App-specific external storage, so `adb pull` can read it without run-as (falls back to internal files). */
    fun init(ctx: android.content.Context) {
        log = DiagLog(java.io.File(ctx.getExternalFilesDir(null) ?: ctx.filesDir, "diag"))
    }

    fun i(
        tag: String,
        msg: String,
        vararg fields: Pair<String, Any?>,
    ) {
        log?.info(tag, msg, mapOf(*fields))
    }

    fun w(
        tag: String,
        msg: String,
        vararg fields: Pair<String, Any?>,
    ) {
        log?.warn(tag, msg, mapOf(*fields))
    }

    fun e(
        tag: String,
        msg: String,
        t: Throwable? = null,
        vararg fields: Pair<String, Any?>,
    ) {
        log?.error(tag, msg, t, mapOf(*fields))
    }
}
