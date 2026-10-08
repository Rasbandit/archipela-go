package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalNetworkTest {
    private fun local(url: String) = assertTrue(url, isLocalNetworkServer(url))

    private fun notLocal(url: String) = assertFalse(url, isLocalNetworkServer(url))

    @Test fun privateIpv4RangesAreLocal() {
        local("192.168.1.20:38281")
        local("10.0.20.214:38281")
        local("10.0.2.2:38281") // the emulator's host machine
        local("172.16.0.1:38281")
        local("172.31.255.255:38281")
        local("169.254.10.10:38281") // link-local
    }

    @Test fun schemesPathsAndMissingPortsAreIgnored() {
        local("ws://192.168.1.20:38281")
        local("wss://192.168.1.20:38281/")
        local("WS://10.1.2.3")
        local("192.168.0.5")
        local("  192.168.0.5:38281  ")
    }

    @Test fun publicAddressesAndHostsAreNotLocal() {
        notLocal("archipelago.gg:38281")
        notLocal("wss://archipelago.gg:38281")
        notLocal("8.8.8.8:38281")
        notLocal("172.15.0.1:38281") // just below 172.16/12
        notLocal("172.32.0.1:38281") // just above it
        notLocal("11.0.0.1:38281")
        notLocal("192.169.0.1:38281")
    }

    @Test fun loopbackIsNotLocalNetwork() {
        // `adb reverse` dev loop and on-device servers use loopback, which the permission does not gate.
        notLocal("localhost:38281")
        notLocal("127.0.0.1:38281")
        notLocal("ws://localhost:38281")
        notLocal("[::1]:38281")
    }

    @Test fun lanHostNamesAreLocal() {
        local("mypc.local:38281")
        local("ws://server.lan:38281")
        local("box.home.arpa:38281")
        local("nas.internal:38281")
        local("fastraid:38281") // a single-label name resolves on the LAN
    }

    @Test fun ipv6LinkLocalAndUniqueLocalAreLocal() {
        local("[fe80::1]:38281")
        local("[fd12:3456::1]:38281")
        local("ws://[fc00::5]")
        notLocal("[2001:4860:4860::8888]:38281")
    }

    @Test fun blankOrMalformedInputIsNotLocal() {
        notLocal("")
        notLocal("   ")
        notLocal("ws://")
        notLocal(":38281")
        notLocal("999.1.1.1:38281")
        notLocal("192.168.1:38281") // a name with dots, not an address
    }

    @Test fun carrierGradeNatIsLocal() {
        local("100.64.0.1:38281")
        local("100.127.255.255:38281")
        notLocal("100.63.255.255:38281")
        notLocal("100.128.0.1:38281")
    }

    @Test fun aTrailingDotIsIgnored() {
        local("nas.lan.:38281")
        local("fastraid.:38281")
        notLocal("archipelago.gg.:38281")
        assertEquals("archipelago.gg", serverHost("wss://archipelago.gg.:38281/"))
    }

    @Test fun serverHostIsTheBareHost() {
        assertEquals("192.168.1.2", serverHost("ws://192.168.1.2:38281"))
        assertEquals("fe80::1", serverHost("[fe80::1]:38281"))
        assertEquals("archipelago.gg", serverHost("ARCHIPELAGO.GG"))
        assertNull(serverHost("ws://"))
        assertNull(serverHost("  "))
    }

    @Test fun resolvedAddressesAreClassifiedAsRawLiterals() {
        assertTrue(isLocalAddress("10.0.20.214"))
        assertTrue(isLocalAddress("fe80::1%wlan0")) // InetAddress.hostAddress carries the scope
        assertTrue(isLocalAddress("fd7a:115c:a1e0::1"))
        assertFalse(isLocalAddress("2001:4860:4860::8888"))
        assertFalse(isLocalAddress("::1"))
        assertFalse(isLocalAddress("127.0.0.1"))
        assertFalse(isLocalAddress("8.8.8.8"))
        assertFalse(isLocalAddress("not an address"))
        assertFalse(isLocalAddress(""))
    }

    @Test fun promptWhenTheNameOrAnyResolvedAddressIsLocal() {
        // A public name pointed at the LAN (split DNS, Pi-hole, sslip.io) is only caught by its addresses.
        assertTrue(needsLocalNetworkPrompt("ap.example.com:38281", listOf("10.0.0.5")))
        assertTrue(needsLocalNetworkPrompt("ap.example.com:38281", listOf("8.8.8.8", "fd00::5")))
        assertTrue(needsLocalNetworkPrompt("192.168.1.2:38281", emptyList()))
        assertFalse(needsLocalNetworkPrompt("archipelago.gg:38281", listOf("1.2.3.4")))
        assertFalse(needsLocalNetworkPrompt("localhost:38281", listOf("127.0.0.1", "::1")))
        assertFalse(needsLocalNetworkPrompt("archipelago.gg:38281", emptyList())) // lookup failed
    }
}
