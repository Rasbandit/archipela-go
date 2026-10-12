package dev.apgo2

import androidx.activity.ComponentActivity
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.github.takahirom.roborazzi.AccessibilityCheckAfterTestStrategy
import com.github.takahirom.roborazzi.ExperimentalRoborazziApi
import com.github.takahirom.roborazzi.RobolectricDeviceQualifiers
import com.github.takahirom.roborazzi.RoborazziATFAccessibilityCheckOptions
import com.github.takahirom.roborazzi.RoborazziATFAccessibilityChecker
import com.github.takahirom.roborazzi.RoborazziRule
import com.google.android.apps.common.testing.accessibility.framework.AccessibilityCheckPreset
import dev.apgo2.ui.ApgoTheme
import dev.apgo2.ui.MapOverlayCard
import org.junit.Rule
import org.junit.Test
import org.junit.rules.ExpectedException
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.apgo_ffi.AboutOut
import uniffi.apgo_ffi.AboutRowOut
import uniffi.apgo_ffi.PlaceOut
import uniffi.apgo_ffi.ProgressOut
import uniffi.apgo_ffi.QuestOut

private const val SDK = 35 // the newest Android version Robolectric runs

// Google's Accessibility Test Framework on the rendered quest card (touch targets, contrast measured from the pixels, labels),
// in light and dark. Any warning fails the test.
@OptIn(ExperimentalRoborazziApi::class)
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(qualifiers = RobolectricDeviceQualifiers.Pixel7, sdk = [SDK])
class QuestCardA11yTest {
    // Lets the control test expect the failure the after-test check throws.
    @Suppress("DEPRECATION")
    @get:Rule(order = Int.MIN_VALUE)
    val thrown: ExpectedException = ExpectedException.none()

    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @get:Rule
    val roborazzi =
        RoborazziRule(
            composeRule = compose,
            captureRoot = compose.onRoot(),
            options =
                RoborazziRule.Options(
                    roborazziAccessibilityOptions =
                        RoborazziATFAccessibilityCheckOptions(
                            RoborazziATFAccessibilityChecker(preset = AccessibilityCheckPreset.LATEST),
                            failureLevel = RoborazziATFAccessibilityChecker.CheckLevel.Warning,
                        ),
                    accessibilityCheckStrategy = AccessibilityCheckAfterTestStrategy(),
                ),
        )

    private val about =
        AboutOut(
            description = "A bronze man offering his umbrella.",
            rows = listOf(AboutRowOut("Kind", "Statue"), AboutRowOut("Artist", "J. Seward Johnson")),
            wikipediaUrl = "https://en.wikipedia.org/wiki/Allow_Me",
        )

    private fun show(q: QuestOut) {
        compose.setContent {
            ApgoTheme { MapOverlayCard { QuestCard(q, loadAbout = { about }) {} } }
        }
    }

    private fun placeQuest(state: String) =
        quest(state).copy(
            headline = "Get here",
            cardDetail = "Within 130 ft",
            placeRow = PlaceOut("Allow Me", "Artwork", quoted = true),
            hasAbout = true,
        )

    private fun dwellQuest(state: String) =
        quest(state).copy(
            headline = "Sit 3 min",
            cardDetail = "Within 130 ft · leaving pauses the timer",
            placeRow = PlaceOut(null, "Bench", quoted = false),
            progressBlock = ProgressOut("Time here", "1:12 of 3:00", 0.4f, null, emptyList(), 0u),
            reward = if (state == "done") "Key to zone 2" else null,
        )

    private fun courierQuest(state: String) =
        quest(state).copy(
            headline = "Carry it across",
            cardDetail = "0.4 mi between the two points",
            progressBlock = ProgressOut("", "", 0.5f, "6:40 left to deliver", listOf("Pick up", "Deliver"), 1u),
        )

    private fun aboutOpen(state: String) {
        show(placeQuest(state))
        compose.onNodeWithText("About this place").performClick()
        compose.waitForIdle()
    }

    // Control: proves the checker runs and fails the build; a tap target with no label must be caught.
    @Test fun anUnlabelledButtonFails() {
        thrown.expectMessage("SpeakableTextPresentCheck")
        compose.setContent { Box(Modifier.size(48.dp).clickable {}) }
    }

    // Control: the bug that started this check, the old reward green on the dark card (2.2:1), must be caught.
    @Config(qualifiers = "+night")
    @Test
    fun theOldRewardGreenOnTheDarkCardFails() {
        thrown.expectMessage("TextContrastCheck")
        compose.setContent { ApgoTheme { MapOverlayCard { Text("Reward: Key to zone 2", color = Color(0xFF2E7D32)) } } }
    }

    @Test fun placeCardWithAboutOpenLight() = aboutOpen("open")

    @Config(qualifiers = "+night")
    @Test
    fun placeCardWithAboutOpenDark() = aboutOpen("open")

    @Test fun dwellInProgressLight() = show(dwellQuest("progress"))

    @Config(qualifiers = "+night")
    @Test
    fun dwellInProgressDark() = show(dwellQuest("progress"))

    @Test fun doneWithRewardLight() = show(dwellQuest("done"))

    @Config(qualifiers = "+night")
    @Test
    fun doneWithRewardDark() = show(dwellQuest("done"))

    @Test fun courierStepsLight() = show(courierQuest("progress"))

    @Config(qualifiers = "+night")
    @Test
    fun courierStepsDark() = show(courierQuest("progress"))

    private fun quest(state: String) =
        QuestOut(
            locationId = 1,
            zone = 1u,
            name = "Gallery Without Walls",
            place = "Allow Me",
            family = "landmark",
            kindId = "gallery_walls",
            difficulty = "easy",
            tier = 1u,
            effortMin = 9.0,
            mode = "walk",
            state = state,
            progress = 0f,
            shape = "point",
            anchor = null,
            anchorB = null,
            radiusM = 40.0,
            path = emptyList(),
            detail = "",
            fallback = false,
            boss = false,
            blurb = "",
            reward = null,
            chainId = null,
            collect = null,
            headline = "",
            cardDetail = "",
            placeRow = null,
            progressBlock = null,
            hasAbout = false,
        )
}
