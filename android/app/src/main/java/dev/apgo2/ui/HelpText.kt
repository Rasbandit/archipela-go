package dev.apgo2.ui

import dev.apgo2.AwayFormat
import uniffi.apgo_ffi.RecapOut

/** One explanation: a short title and the text. All of the app's help copy is in this file, so wording can be fixed in one place. */
internal data class HelpTopic(
    val title: String,
    val body: String,
)

internal object Help {
    // ---- zones
    val zones =
        HelpTopic(
            "Zones",
            "A zone is one of your realms. You start in zone 1. Every later zone opens when you have found enough Zone Keys. " +
                "The same realm can be used for several zones.",
        )
    val runQuests =
        HelpTopic(
            "Run quests",
            "Turns some quests into runs (the share you pick). A run covers more ground in the same time, and it carries " +
                "the Running Shoes badge: it stays locked until you have found the Running Shoes.",
        )
    val questTypes =
        HelpTopic(
            "Quest types",
            "The kinds of quest this zone can give you. The number on each is how many places of that type were found in the realm. " +
                "A type with none found is switched off, because there is nothing to send you to. Walking to a point is always allowed.",
        )

    // ---- quest families
    val families: Map<String, HelpTopic> =
        mapOf(
            "reach" to HelpTopic("Reach", "Get to a spot: a street corner, a landmark. The simplest quest, and it always works."),
            "dwell" to HelpTopic("Dwell", "Get to a place and stay a while: a bench, a picnic table, a viewpoint."),
            "landmark" to HelpTopic("Landmark", "Visit a specific kind of place: a mural, a fountain, a courthouse, a fire hydrant."),
            "trail" to
                HelpTopic("Trail", "Walk (or ride) part of a named trail or path. Long trails ask for the share that fits the effort."),
            "park" to HelpTopic("Park", "Spend time in a park, or walk around its edge."),
            "water" to HelpTopic("Water", "Follow a river, creek or canal for a stretch."),
            "courier" to
                HelpTopic(
                    "Courier",
                    "Pick something up at one spot and deliver it to another, go out and come back, " +
                        "or gather things scattered around your area and bring enough of them home.",
                ),
            "explore" to HelpTopic("Explore", "Visit new map cells you have never been to."),
            "steps" to HelpTopic("Steps", "Take a number of steps. Counted by your phone's step sensor."),
            "away" to HelpTopic("Away", "Go well away from home and stay there for a while."),
        )

    // ---- goals
    val goals =
        HelpTopic(
            "Win conditions",
            "What you have to do to win the game. Choose one, or several and decide how they combine. " +
                "Archipelago multiworlds support this too: you report that you have won when your rule is met.",
        )
    val goalRule =
        HelpTopic(
            "When do I win?",
            "With several goals you choose how they count. Any one: the first goal you finish wins. All: you must finish every goal. " +
                "At least N: finishing any N of them wins.",
        )
    val goalDescriptions: Map<String, HelpTopic> =
        mapOf(
            "treasure_hunt" to HelpTopic("Treasure Hunt", "Collect APGO, then finish the boss quest to claim the treasure."),
            "all_trips" to HelpTopic("Completionist", "Finish every quest in the game."),
        )

    // ---- game tuning
    val questCount =
        HelpTopic(
            "Number of quests",
            "How many quests the game has in all, spread over your zones. More quests means a longer game and " +
                "more rewards to find.",
        )
    val difficulty =
        HelpTopic(
            "Difficulty mix",
            "How the quests split between Easy (short), Medium and Hard (long). Relaxed is mostly easy, Challenging has many hard ones.",
        )
    val minutesPerTier =
        HelpTopic(
            "Minutes per tier",
            "Quests have a size from 1 to 10. This is how many active minutes one step of that size is worth, so " +
                "a size 5 quest at 10 minutes takes about 50 minutes.",
        )
    val fog = HelpTopic("Fog of war", "Quests stay hidden until you get close to them. You discover the game by exploring.")
    val traps =
        HelpTopic(
            "Traps",
            "Some rewards are traps: Freeze (stand still to thaw), Leash, Detour, Toll and more. Each ends in a " +
                "way you can always complete. Switch off for a calm game.",
        )
    val bonus =
        HelpTopic(
            "Bonus items",
            "Extra helpful rewards: Effort Reductions make quests shorter, Scouting reveals quests from farther " +
                "away, Collection makes it easier to count as arrived.",
        )
    val terrain =
        HelpTopic(
            "Terrain",
            "Prefer paved: avoids unpaved paths when the map says which they are. Paved only: never sends you on them. " +
                "Only some of the map is tagged with surfaces, so this is a best effort.",
        )
    val stairs = HelpTopic("Avoid stairs", "Leaves out quests that are staircases.")
    val parking = HelpTopic("Allow parking lots", "Turn off to keep quests out of parking lots, where cars back out of spaces.")

    // ---- keeping tracking alive (battery optimisation), by phone maker
    val batterySamsung =
        HelpTopic(
            "Keep tracking alive on Samsung",
            "Samsung puts apps to sleep to save battery, which stops quest tracking with the screen off. Tap the button, find " +
                "Archipela-Go 2 and choose Don't optimise. Also open Settings, Battery, Background usage limits and make sure the app " +
                "is not in Sleeping apps or Deep sleeping apps.",
        )
    val batteryXiaomi =
        HelpTopic(
            "Keep tracking alive on Xiaomi, Redmi and POCO",
            "MIUI and HyperOS stop apps in the background. Tap the button and turn optimisation off for Archipela-Go 2. Then open the " +
                "app's info page, choose Battery saver and pick No restrictions, and turn on Autostart.",
        )
    val batteryHuawei =
        HelpTopic(
            "Keep tracking alive on Huawei and Honor",
            "These phones close apps in the background. Tap the button and allow Archipela-Go 2 to ignore optimisation. Then open " +
                "Settings, Battery, App launch, find the app and switch it to Manage manually with every option on.",
        )
    val batteryOppo =
        HelpTopic(
            "Keep tracking alive on OnePlus, OPPO and realme",
            "These phones freeze apps in the background. Tap the button and turn optimisation off for Archipela-Go 2. Then open the " +
                "app's info page, choose Battery and allow background activity.",
        )
    val batteryOther =
        HelpTopic(
            "Keep tracking alive",
            "Android may pause apps in the background to save battery, which stops quest tracking with the screen off. Tap the button, " +
                "find Archipela-Go 2 and turn battery optimisation off for it.",
        )

    // ---- the map pin
    val hollowDot =
        HelpTopic(
            "Why is my dot hollow or grey?",
            "A hollow dot means GPS dropped out and the game is guessing where you are from your steps and the streets. A grey dot means " +
                "the last position is more than half a minute old. Neither counts for quests: only a solid dot from GPS does.",
        )
    val networkOnly =
        HelpTopic(
            "Quests do not count with location from Wi-Fi only",
            "Your position is coming from Wi-Fi and mobile networks, not from satellites. It shows on the map but never counts " +
                "for quests: turn on GPS (precise location) in the phone's location settings, and step outside if you are indoors.",
        )

    // ---- Archipelago
    val archipelago =
        HelpTopic(
            "Archipelago",
            "Join a multiworld: your quests hold items for other players and theirs hold items for you. " +
                "Enter the server address and your slot name, connect, then pick a realm for each zone the game needs.",
        )

    // ---- settings
    val trapAlerts =
        HelpTopic(
            "Trap alerts",
            "When a trap hits you, the phone shows a loud alert and buzzes hard, so you notice even with the phone in a pocket. " +
                "A Silence Trap mutes them while it lasts. You can tune the Traps channel in the phone's notification settings.",
        )

    val units =
        HelpTopic(
            "Distance units",
            "How distances are shown. Auto uses the units of your phone's region.",
        )

    // ---- walk recap
    const val RECAP_TITLE = "Welcome home"
    const val RECAP_DONE = "Done"
    const val RECAP_QUESTS = "Quests completed"
    const val RECAP_GOAL = "Goal"
    const val RECAP_DONE_AT = "Done at"
    const val RECAP_REWARD = "Reward"
    const val RECAP_NOTICE_TITLE = "Welcome home: see your walk"

    fun recapNoticeText(r: RecapOut): String {
        val time = "${AwayFormat.duration(r.endMs - r.startMs)} out"
        val n = r.questsDone.size
        return if (n == 0) time else "$time, $n ${if (n == 1) "quest" else "quests"} done"
    }
}

/** Explanatory copy of the setup wizard and its Home card nag, kept here with the rest of the app's help text. */
internal object SetupText {
    const val HOME_BASE_NAME = "Home Base"
    const val HOME_BASE_CARD =
        "Distances in your games are measured from here. On your home Wi-Fi the game pauses and GPS turns " +
            "off; while your car is connected nothing counts. Tap to change."
    const val HOME_BASE_UNSET = "Not set yet. Tap to choose where distances are measured from, and which Wi-Fi and car pause the game."
    const val WIFI_WHY =
        "While you are on any of these networks nothing counts and GPS turns off. That saves battery and " +
            "stops cheating. Tick every network your home uses."
    const val CAR_WHY =
        "While your car is connected nothing counts, so rides do not turn into pickups. Skip this if you " +
            "never drive while playing."
    const val WIFI_NONE_FOUND = "No networks found. Connect to your home Wi-Fi, or type its name below."
    const val WIFI_NEEDS_LOCATION = "Allow location to list nearby networks."
    const val CAR_NEEDS_BLUETOOTH = "Bluetooth permission lets the app see your paired devices."
    const val CAR_BLUETOOTH_BLOCKED =
        "Android will not ask again. In the app's settings open Permissions, allow Nearby devices, then come back here."
    const val CAR_NONE_PAIRED = "No paired Bluetooth devices found. Pair your car in the phone's Bluetooth settings first."
    const val BATTERY_TITLE = "Keep tracking alive · optional"
    const val BATTERY_WHY =
        "Quests are tracked with the screen off. Your phone's battery saver can stop that; this lets the game keep running while you play."
    const val BATTERY_DONE = "Battery saver is off for this app. Tracking will keep running."
    const val BATTERY_BUTTON = "Open battery settings"
    const val HOME_NEEDS_WIFI = "Add your home Wi-Fi so the game pauses at home"
}

/** The "You're home: add this Wi-Fi?" dialog (see `HomeWifiDialog`). */
internal object HomeOfferText {
    const val TITLE = "You're home: add this Wi-Fi?"
    const val ADD = "Add"
    const val MUTE = "Not this one"
    const val LATER = "Later"

    fun body(ssid: String) =
        "You are at your home pin and connected to \"$ssid\". Add it as home Wi-Fi so the game pauses and GPS turns off while you " +
            "are on it. That saves battery, and nothing counts at home."
}
