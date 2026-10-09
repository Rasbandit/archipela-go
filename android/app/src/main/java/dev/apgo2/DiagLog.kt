package dev.apgo2

import java.io.File

/**
 * Rotating JSON-lines diagnostics log kept on the phone (`diag-0001.jsonl`, ...), so a whole outing can be pulled off
 * the device and analysed later. One line per entry: `{"t":epoch_ms,"lvl":"I|W|E","tag":..,"msg":..,<fields>}`.
 * Never throws: a log that cannot be written must not break the app.
 */
internal class DiagLog(
    private val dir: File,
    private val maxFileBytes: Long = 2_000_000,
    private val keep: Int = 10,
    private val clock: () -> Long = System::currentTimeMillis,
    private val prefix: String = "diag",
) {
    private val pattern = Regex("${Regex.escape(prefix)}-(\\d{4})\\.jsonl")

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
    fun files(): List<File> = dir.listFiles { f -> f.isFile && pattern.matches(f.name) }?.sortedBy { seq(it) } ?: emptyList()

    private fun seq(f: File) = pattern.matchEntire(f.name)!!.groupValues[1].toInt()

    private fun name(n: Int) = "$prefix-%04d.jsonl".format(n)

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
}

/** The app-wide log. Safe to call before [init] (entries are dropped). */
internal object Diag {
    @Volatile private var log: DiagLog? = null

    @Volatile private var rawLog: DiagLog? = null

    /**
     * Internal storage (`files/diag`), pulled with run-as (`scripts/pull_diag.sh`): app-specific external storage is readable by any
     * app with READ_EXTERNAL_STORAGE on Android 8 and 9 (adversarial review M4). A log left there by an older build is deleted.
     */
    fun init(ctx: android.content.Context) {
        runCatching { ctx.getExternalFilesDir(null)?.let { java.io.File(it, "diag").deleteRecursively() } }
        log = DiagLog(java.io.File(ctx.filesDir, "diag"))
    }

    /**
     * Debug builds only: raw fixes, steps and headings for the replay bench, in `diag/raw/` (10 x 2 MB of their own, so they never push
     * the normal log out). Release builds never call this, so they never store raw positions.
     */
    fun initRaw(ctx: android.content.Context) {
        val debuggable = ctx.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0
        if (!debuggable) return
        rawLog = DiagLog(java.io.File(ctx.filesDir, "diag/raw"), prefix = "raw")
    }

    /** A raw-track line (no-op unless [initRaw] enabled it). */
    fun raw(
        tag: String,
        fields: Map<String, Any?>,
    ) {
        rawLog?.write("I", tag, "", fields)
    }

    fun info(
        tag: String,
        msg: String,
        vararg fields: Pair<String, Any?>,
    ) {
        log?.info(tag, msg, mapOf(*fields))
    }

    fun warn(
        tag: String,
        msg: String,
        vararg fields: Pair<String, Any?>,
    ) {
        log?.warn(tag, msg, mapOf(*fields))
    }

    fun error(
        tag: String,
        msg: String,
        t: Throwable? = null,
        vararg fields: Pair<String, Any?>,
    ) {
        log?.error(tag, msg, t, mapOf(*fields))
    }

    /** An app-model operation ([what]) threw: log it with its stack. */
    fun failure(
        what: String,
        t: Throwable,
    ) = error("model", "$what failed", t)
}
