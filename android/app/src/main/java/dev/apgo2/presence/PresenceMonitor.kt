package dev.apgo2.presence

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.Looper

/** Watches the Wi-Fi network and the Bluetooth connections that decide presence. Needs location permission for Wi-Fi names and BLUETOOTH_CONNECT for devices. */
class PresenceMonitor(private val ctx: Context, private val onChange: () -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private val cm = ctx.getSystemService(ConnectivityManager::class.java)
    var currentWifi: WifiId? = null
        private set

    /** Addresses of connected Bluetooth devices; `null` while BLUETOOTH_CONNECT is not granted. */
    var connectedCarCandidates: Set<String>? = null
        private set
    /** True once a Wi-Fi callback has reported since [start] (even "not connected" is only reported by silence, see the app model's timeout). */
    var wifiReported = false
        private set

    /** True once the Bluetooth device read has answered (or was skipped for lack of permission), so `connectedCarCandidates` is trustworthy. */
    var bluetoothReady = false
        private set
    private var proxiesPending = 0
    /** Bumped on every start so that late answers from an earlier session are ignored. */
    private var generation = 0
    private val bt = mutableSetOf<String>()
    private var started = false

    private val netCallback: ConnectivityManager.NetworkCallback =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) object : ConnectivityManager.NetworkCallback(FLAG_INCLUDE_LOCATION_INFO) {
            override fun onCapabilitiesChanged(n: Network, caps: NetworkCapabilities) = update((caps.transportInfo as? WifiInfo)?.let { WifiId(it.ssid, it.bssid) })
            override fun onLost(n: Network) = update(null)
        } else object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(n: Network, caps: NetworkCapabilities) = update(legacyWifi())
            override fun onLost(n: Network) = update(null)
        }

    @Suppress("DEPRECATION")
    @SuppressLint("MissingPermission")
    private fun legacyWifi(): WifiId? = ctx.applicationContext.getSystemService(WifiManager::class.java)?.connectionInfo?.let { WifiId(it.ssid, it.bssid) }

    // Network callbacks are registered with the main handler, so this runs on the main thread.
    private fun update(w: WifiId?) {
        if (!started) return
        val first = !wifiReported
        wifiReported = true
        if (w != currentWifi || first) { currentWifi = w; onChange() }
    }

    private val btReceiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context, i: Intent) {
            val d = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) i.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java) else @Suppress("DEPRECATION") i.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
            val a = d?.address ?: return
            if (!started) return
            if (i.action == BluetoothDevice.ACTION_ACL_CONNECTED) bt.add(a) else bt.remove(a)
            connectedCarCandidates = bt.toSet()
            onChange()
        }
    }

    private fun btAllowed() = Build.VERSION.SDK_INT < Build.VERSION_CODES.S || ctx.checkSelfPermission(android.Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED

    @SuppressLint("MissingPermission")
    fun start() {
        if (started) return
        reset()
        cm.registerNetworkCallback(NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(), netCallback, main)
        started = true
        if (btAllowed()) {
            val filter = IntentFilter().apply { addAction(BluetoothDevice.ACTION_ACL_CONNECTED); addAction(BluetoothDevice.ACTION_ACL_DISCONNECTED) }
            // ACL broadcasts come from the system, so the receiver is exported; the flag exists from API 33 and is mandatory when targeting 34+.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) ctx.registerReceiver(btReceiver, filter, Context.RECEIVER_EXPORTED) else ctx.registerReceiver(btReceiver, filter)
            connectedCarCandidates = bt.toSet()
            val adapter = ctx.getSystemService(BluetoothManager::class.java)?.adapter
            val profiles = intArrayOf(BluetoothProfile.A2DP, BluetoothProfile.HEADSET)
            val gen = ++generation
            proxiesPending = profiles.size
            /** One profile has answered (or cannot): the read is complete when none is left. Stale sessions are ignored. */
            fun answered() { if (gen == generation && --proxiesPending <= 0) bluetoothReady = true }
            if (adapter == null) { proxiesPending = 0; bluetoothReady = true }
            else for (profile in profiles) {
                val asked = adapter.getProfileProxy(ctx, object : BluetoothProfile.ServiceListener {
                    override fun onServiceConnected(p: Int, proxy: BluetoothProfile) {
                        if (started && gen == generation) {
                            runCatching { proxy.connectedDevices.forEach { bt.add(it.address) } }
                            connectedCarCandidates = bt.toSet()
                            answered()
                        }
                        adapter.closeProfileProxy(p, proxy)
                        if (started && gen == generation) onChange()
                    }
                    override fun onServiceDisconnected(p: Int) {}
                }, profile)
                if (!asked) answered() // no listener will ever fire (adapter off)
            }
        } else {
            bluetoothReady = true // nothing to wait for: car devices stay unknown
        }
    }

    fun stop() {
        if (!started) return
        started = false
        runCatching { cm.unregisterNetworkCallback(netCallback) }
        runCatching { ctx.unregisterReceiver(btReceiver) }
        reset()
    }

    /** Forget everything: no events arrive while stopped, so old values would go stale. */
    private fun reset() {
        bt.clear()
        wifiReported = false
        bluetoothReady = false
        proxiesPending = 0
        currentWifi = null
        connectedCarCandidates = null
    }

    /** The network the phone is on right now, for "Add current network". */
    fun currentNetwork(): WifiId? = PresenceSignals.usableNetwork(currentWifi ?: legacyWifi())
}
