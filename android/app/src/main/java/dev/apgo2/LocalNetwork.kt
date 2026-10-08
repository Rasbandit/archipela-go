package dev.apgo2

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext

// Android 17 (apps targeting API 37) blocks traffic to the local network unless ACCESS_LOCAL_NETWORK is granted: a TCP
// connect to a LAN Archipelago server just times out. Loopback (`adb reverse`, an on-device server) and the internet are not gated.

private val LAN_SUFFIXES = listOf(".local", ".lan", ".home", ".home.arpa", ".internal")
private const val IPV4_PARTS = 4
private const val OCTET_MAX = 255

/**
 * True when the Archipelago server typed by the player ([url]: `host:port`, optionally with `ws://`/`wss://` and a path)
 * is on the local network: a private, link-local or unique-local address, or a LAN-only host name. Pure; never resolves DNS.
 */
internal fun isLocalNetworkServer(url: String): Boolean {
    val host = hostOf(url) ?: return false
    val octets = ipv4Octets(host)
    return when {
        host.contains(':') -> isLocalIpv6(host)

        octets != null -> isPrivateIpv4(octets)

        // dotted digits that are not an address, or loopback
        host.all { it.isDigit() || it == '.' } || host == "localhost" -> false

        else -> !host.contains('.') || LAN_SUFFIXES.any { host.endsWith(it) }
    }
}

// The host part, lower-cased, without brackets for IPv6; null when there is none.
private fun hostOf(url: String): String? {
    val rest =
        url
            .trim()
            .lowercase()
            .substringAfter("://")
            .substringBefore('/')
    val host = if (rest.startsWith('[')) rest.substringAfter('[').substringBefore(']') else rest.substringBefore(':')
    return host.ifEmpty { null }
}

private fun ipv4Octets(host: String): List<Int>? =
    host
        .split('.')
        .takeIf { it.size == IPV4_PARTS }
        ?.map { part -> part.toIntOrNull()?.takeIf { it in 0..OCTET_MAX } ?: return null }

@Suppress("MagicNumber") // the RFC 1918 and RFC 3927 ranges are clearer as literals
private fun isPrivateIpv4(o: List<Int>) =
    o[0] == 10 || (o[0] == 172 && o[1] in 16..31) || (o[0] == 192 && o[1] == 168) || (o[0] == 169 && o[1] == 254)

// fe80::/10 (link-local) and fc00::/7 (unique local); loopback ::1 is not local network.
private val LOCAL_IPV6 = Regex("^(fe[89ab]|f[cd])")

private fun isLocalIpv6(host: String) = LOCAL_IPV6.containsMatchIn(host)

private const val LAN_DENIED = "blocked: allow Nearby devices to reach a server on your network"

/**
 * Returns a connect action for the server form: it asks for ACCESS_LOCAL_NETWORK first when [url] is on the LAN and the
 * permission is missing (Android 17+), then calls [connect]; when it is denied, [onDeny] gets a status line instead.
 */
@Composable
internal fun rememberLanAwareConnect(
    connect: (url: String) -> Unit,
    onDeny: (status: String) -> Unit,
): (url: String) -> Unit {
    val ctx = LocalContext.current
    // Saveable: the permission dialog may recreate the activity before it answers.
    var pending by rememberSaveable { mutableStateOf("") }
    val ask =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            Diag.info("permission", "local_network", "granted" to granted)
            if (granted) connect(pending) else onDeny(LAN_DENIED)
        }
    return { url ->
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.CINNAMON_BUN && isLocalNetworkServer(url) &&
            !ctx.hasPermission(Manifest.permission.ACCESS_LOCAL_NETWORK)
        ) {
            pending = url
            ask.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
        } else {
            connect(url)
        }
    }
}
