package dev.apgo2

import android.app.Application
import android.os.Build
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/** Owns the model for the life of the process, so tracking survives the activity being recreated or destroyed. */
internal class ApgoApp : Application() {
    private val modelLazy =
        lazy {
            AppModel(applicationContext, CoroutineScope(SupervisorJob() + Dispatchers.Main)).apply {
                refreshAll()
                library.resumePlaying()
            }
        }
    val model: AppModel by modelLazy

    /** The model if this process already loaded it (the screen or the tracking service did), without loading it. */
    val loadedModel: AppModel? get() = if (modelLazy.isInitialized()) modelLazy.value else null

    override fun onCreate() {
        super.onCreate()
        Diag.init(this)
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { t, e ->
            Diag.error("crash", "uncaught exception on ${t.name}", e)
            previous?.uncaughtException(t, e)
        }
        Diag.info(
            "app",
            "start",
            "device" to "${Build.MANUFACTURER} ${Build.MODEL}",
            "sdk" to Build.VERSION.SDK_INT,
            "abi" to Build.SUPPORTED_ABIS.firstOrNull(),
        )
    }
}
