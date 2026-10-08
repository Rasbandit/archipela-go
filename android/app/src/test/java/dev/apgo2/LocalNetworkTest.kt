package dev.apgo2

import org.junit.Assert.assertFalse
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
}
