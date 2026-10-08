package dev.apgo2

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PromptGateTest {
    @Test fun followUpsWaitForLocation() = assertFalse(askFollowUps(location = false, setupOpen = false))

    @Test fun followUpsAreHeldWhileSetupIsOpen() = assertFalse(askFollowUps(location = true, setupOpen = true))

    @Test fun followUpsRunOnceSetupIsClosed() = assertTrue(askFollowUps(location = true, setupOpen = false))

    @Test fun followUpsNeverWithoutLocation() = assertFalse(askFollowUps(location = false, setupOpen = true))

    @Test fun backgroundIsExplainedOnceSetupIsClosed() = assertTrue(explain())

    @Test fun backgroundIsHeldWhileSetupIsOpen() = assertFalse(explain(setupOpen = true))

    @Test fun backgroundNeedsLocation() = assertFalse(explain(location = false))

    @Test fun backgroundNotAskedWhenGranted() = assertFalse(explain(background = true))

    @Test fun backgroundNotAskedAfterNotNow() = assertFalse(explain(declined = true))

    @Test fun backgroundNeedsAndroid10() = assertFalse(explain(sdk = 28))

    @Test fun backgroundAskedFromAndroid10() = assertTrue(explain(sdk = 29))

    private fun explain(
        location: Boolean = true,
        background: Boolean = false,
        declined: Boolean = false,
        sdk: Int = 34,
        setupOpen: Boolean = false,
    ) = explainBackground(location, background, declined, sdk, setupOpen)
}
