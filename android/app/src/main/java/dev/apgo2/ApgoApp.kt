package dev.apgo2

import android.app.Application
import android.os.Build
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/** Owns the model for the life of the process, so tracking survives the activity being recreated or destroyed. */
class ApgoApp : Application() {
    override fun onCreate() {
        super.onCreate()
        Diag.init(this)
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { t, e ->
            Diag.e("crash", "uncaught exception on ${t.name}", e)
            previous?.uncaughtException(t, e)
        }
        Diag.i("app", "start", "device" to "${Build.MANUFACTURER} ${Build.MODEL}", "sdk" to Build.VERSION.SDK_INT, "abi" to Build.SUPPORTED_ABIS.firstOrNull())
    }

    val model: AppModel by lazy {
        AppModel(applicationContext, CoroutineScope(SupervisorJob() + Dispatchers.Main)).also { it.refreshAll() }
    }
}
