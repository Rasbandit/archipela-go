package dev.apgo2

import android.Manifest
import android.content.Context
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.net.InetAddress

// Android 17 (apps targeting API 37) blocks traffic to the local network unless ACCESS_LOCAL_NETWORK is granted: a TCP
// connect to a LAN Archipelago server just hangs. Loopback (`adb reverse`, an on-device server), the internet and VPN
// routes (Tailscale) are not gated. The classifier below only decides whether to ASK; the app always connects.

/** Shown next to the connection status when local network access is missing and a connection is not getting through. */
internal const val LAN_HINT = "If this server is on your home network, allow Nearby devices for this app in Settings."

private val LAN_SUFFIXES = listOf(".local", ".lan", ".home", ".home.arpa", ".internal")
private const val IPV4_PARTS = 4
private const val OCTET_MAX = 255

/**
 * True when the Archipelago server typed by the player ([url]: `host:port`, optionally with `ws://`/`wss://` and a path)
 * looks like it is on the local network: a local address (see [isLocalAddress]) or a LAN-only host name. Never resolves DNS.
 */
internal fun isLocalNetworkServer(url: String): Boolean {
    val host = serverHost(url) ?: return false
    return when {
        host.contains(':') || host.all { it.isDigit() || it == '.' } -> isLocalAddress(host)
        host == "localhost" -> false
        else -> !host.contains('.') || LAN_SUFFIXES.any { host.endsWith(it) }
    }
}

/** True when [url] or any of the addresses its host [resolved] to is on the local network: ask before connecting. */
internal fun needsLocalNetworkPrompt(
    url: String,
    resolved: List<String>,
) = isLocalNetworkServer(url) || resolved.any(::isLocalAddress)

/** The host part of [url], lower-cased, without IPv6 brackets or a trailing dot; null when there is none. */
internal fun serverHost(url: String): String? {
    val rest =
        url
            .trim()
            .lowercase()
            .substringAfter("://")
            .substringBefore('/')
    val host = if (rest.startsWith('[')) rest.substringAfter('[').substringBefore(']') else rest.substringBefore(':')
    return host.removeSuffix(".").ifEmpty { null }
}

/**
 * True for an address literal ([ip], as `InetAddress.hostAddress` prints it) on the local network: private, carrier-grade
 * NAT or link-local IPv4, or link-local/unique-local IPv6. Loopback and anything that is not an address are false.
 */
internal fun isLocalAddress(ip: String): Boolean {
    if (ip.contains(':')) return LOCAL_IPV6.containsMatchIn(ip.lowercase())
    val octets = ipv4Octets(ip)
    return octets != null && isPrivateIpv4(octets)
}

private fun ipv4Octets(host: String): List<Int>? =
    host
        .split('.')
        .takeIf { it.size == IPV4_PARTS }
        ?.map { part -> part.toIntOrNull()?.takeIf { it in 0..OCTET_MAX } ?: return null }

// RFC 1918, RFC 6598 (carrier-grade NAT) and RFC 3927 (link-local).
@Suppress("MagicNumber") // the ranges are clearer as literals
private fun isPrivateIpv4(o: List<Int>) =
    o[0] == 10 ||
        (o[0] == 172 && o[1] in 16..31) ||
        (o[0] == 192 && o[1] == 168) ||
        (o[0] == 100 && o[1] in 64..127) ||
        (o[0] == 169 && o[1] == 254)

// fe80::/10 (link-local) and fc00::/7 (unique local); loopback ::1 is not local network.
private val LOCAL_IPV6 = Regex("^(fe[89ab]|f[cd])")

/** True when this Android blocks the local network for us: Android 17+ and ACCESS_LOCAL_NETWORK not granted. */
internal fun Context.lacksLocalNetwork() =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.CINNAMON_BUN && !hasPermission(Manifest.permission.ACCESS_LOCAL_NETWORK)

// The addresses [host] resolves to, as literals; empty when the lookup fails (the connect will report the error).
private suspend fun resolve(host: String): List<String> =
    withContext(Dispatchers.IO) {
        runCatching { InetAddress.getAllByName(host).mapNotNull { it.hostAddress } }.getOrDefault(emptyList())
    }

/**
 * Returns a connect action for the server form. When local network access is missing and the server (by name or by its
 * resolved addresses) is on the LAN, it asks for ACCESS_LOCAL_NETWORK first. It always calls [connect] in the end; after a
 * denial [onHint] gets [LAN_HINT], since the server may still be reachable (a VPN route is not gated).
 */
@Composable
internal fun rememberLanAwareConnect(
    connect: (url: String) -> Unit,
    onHint: (hint: String) -> Unit,
): (url: String) -> Unit {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    // Saveable: the permission dialog may recreate the activity before it answers.
    var pending by rememberSaveable { mutableStateOf("") }
    val ask =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            Diag.info("permission", "local_network", "granted" to granted)
            connect(pending)
            if (!granted) onHint(LAN_HINT)
        }
    return { url ->
        scope.launch {
            val host = serverHost(url)
            val prompt = ctx.lacksLocalNetwork() && host != null && needsLocalNetworkPrompt(url, resolve(host))
            if (prompt) {
                pending = url
                ask.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
            } else {
                connect(url)
            }
        }
    }
}
