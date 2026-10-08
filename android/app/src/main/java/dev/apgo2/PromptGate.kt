package dev.apgo2

// When the permission prompts may appear. Pure, so it is unit-tested. Location is asked at launch (setup step 1 and the Wi-Fi step
// use it); the rest waits until the setup wizard is closed so nothing stacks over it.

private const val ANDROID_10 = 29 // Build.VERSION_CODES.Q: "Allow all the time" is a separate grant from here on

/** The step counter and notification prompts follow the location grant, but only once the setup wizard is closed. */
internal fun askFollowUps(
    location: Boolean,
    setupOpen: Boolean,
) = location && !setupOpen

/** Explain "Allow all the time": location is allowed, background is not, the player has not said "Not now" and setup is closed. */
internal fun explainBackground(
    location: Boolean,
    background: Boolean,
    declined: Boolean,
    sdk: Int,
    setupOpen: Boolean,
) = location && !background && !declined && sdk >= ANDROID_10 && !setupOpen
