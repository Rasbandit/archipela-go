package dev.apgo2

// When the permission prompts may appear. Pure, so it is unit-tested. Location is asked at launch (setup step 1 and the Wi-Fi step
// use it); the rest waits until the setup wizard is closed so nothing stacks over it.

private const val ANDROID_10 = 29 // Build.VERSION_CODES.Q: "Allow all the time" is a separate grant from here on

/** The step counter and notification prompts follow the location grant, but only once the setup wizard is closed. */
internal fun askFollowUps(
    location: Boolean,
    setupOpen: Boolean,
) = location && !setupOpen

/** Where the step counter and notification prompts are; saved across an activity recreate so a rotation does not ask again. */
internal enum class FollowUps { NotAsked, Asking, Done }

/**
 * Explain "Allow all the time": location is allowed, background is not, the player has not said "Not now", setup is closed and the
 * step counter and notification prompts have been answered ([followUpsDone]), so the dialog does not appear on top of them.
 */
internal fun explainBackground(
    location: Boolean,
    background: Boolean,
    declined: Boolean,
    sdk: Int,
    setupOpen: Boolean,
    followUpsDone: Boolean,
) = location && !background && !declined && sdk >= ANDROID_10 && !setupOpen && followUpsDone

/** The home Wi-Fi offer waits (it is held, not dropped) while any other dialog is up, so dialogs never stack. */
internal fun showHomeOffer(otherDialogs: List<Boolean>) = otherDialogs.none { it }
