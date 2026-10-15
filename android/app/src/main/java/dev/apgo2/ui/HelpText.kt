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
                        "or gather things scattered around your area and bring enough of them to a camp.",
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

    const val ZONE_CHAINS = "Zone-specific progressive quests"
    val zoneChains =
        HelpTopic(
            ZONE_CHAINS,
            "Progressive quests like steps, time away and exploring belong to a zone, and only count while you are inside your " +
                "zones. On: walking in a zone fills that zone's quests only. Off: walking in any zone you have opened fills them " +
                "in every zone you have opened. Set once for this game.",
        )

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

    // ---- developer (debug builds only)
    const val DEVELOPER = "Developer"
    const val PREVIEW_RECAP = "Preview walk recap"

    // ---- walk recap
    const val RECAP_TITLE = "Welcome back"
    const val RECAP_DONE = "Done"
    const val RECAP_CLOSE = "Close"
    const val WALK_ROW_HINT = "Open this walk's recap"
    const val EXPANDED = "Expanded"
    const val COLLAPSED = "Collapsed"
    const val RECAP_PROGRESS = "This walk"
    const val RECAP_LEGEND_BEFORE = "before"
    const val RECAP_LEGEND_COUNTED = "counted"
    const val RECAP_QUESTS = "Quests completed"
    const val RECAP_GOAL = "Goal"
    const val RECAP_DONE_AT = "Done at"
    const val RECAP_REWARD = "Reward"
    const val RECAP_NOTICE_TITLE = "Welcome back: see your walk"
    const val RECAP_CHANNEL = "Walk recap"
    const val RECAP_CHANNEL_DESCRIPTION = "How your walk went, when you get back to a camp"
    const val RECAP_ALL_MILESTONES = "All milestones reached"
    const val CHECKS_SENT = "Checks sent"
    const val ITEMS_RECEIVED = "Items received"
    const val ZONES_UNLOCKED = "Zones unlocked"

    /** "+2 banked" on a forager's recap bar. */
    fun recapBanked(n: Long) = "+$n banked"

    /** "2 still in your bag" under a forager's recap bar. */
    fun recapInBag(n: UInt) = "$n still in your bag"

    /** "4,812 walked · 4,090 counted" (counts already formatted). */
    fun recapStepsLine(
        walked: String,
        counted: String,
    ) = "$walked walked \u00b7 $counted counted"

    /** "6,230 total (was 2,140)" on a chain's recap bar (counts already formatted). */
    fun recapTotal(
        total: String,
        was: String,
    ) = "$total total (was $was)"

    /** "next: 7,500" on a chain's recap bar (the mark already formatted). */
    fun recapNext(mark: String) = "next: $mark"

    fun recapGainMinutes(n: Long) = "+$n min"

    fun recapGainSquares(n: Long) = "+$n new squares"

    /** "+4,090 steps" (the count already formatted). */
    fun recapGainSteps(n: String) = "+$n steps"

    fun recapZone(n: UInt) = "Zone $n"

    fun recapNoticeText(r: RecapOut): String {
        val time = "${AwayFormat.duration(r.endMs - r.startMs)} out"
        val n = r.questsDone.size
        return if (n == 0) time else "$time, $n ${if (n == 1) "quest" else "quests"} done"
    }
}

/** Explanatory copy of the setup wizard and its nag, kept here with the rest of the app's help text. */
internal object SetupText {
    const val WIFI_WHY =
        "While you are on any of these networks nothing counts and GPS turns off. That saves battery and " +
            "stops cheating. Tick every network the camp uses."
    const val CAR_WHY =
        "While your car is connected nothing counts, so rides do not turn into pickups. Skip this if you " +
            "never drive while playing."
    const val WIFI_NONE_FOUND = "No networks found. Connect to the camp's Wi-Fi, or type its name below."
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
    const val WIFI_NEEDS_CAMP = "Set your camp first, then add its Wi-Fi."
    const val WIFI_FOR_CAMP = "Networks for this camp:"
    const val WIFI_TITLE = "Camp Wi-Fi · step 2 of 3"
    const val WIFI_SKIP = "Skip, I'll do this at the camp"
    const val HOME_NEEDS_WIFI = "Add a camp's Wi-Fi so the game pauses there"
}

/** The camps card, the camps screen and the camp picker. */
internal object CampText {
    const val NAME = "Camps"
    const val HELP =
        "A camp is a place you play from. Arriving at any camp pauses the game, banks what you carry and shows your walk."
    const val CARD = "$HELP Tap to see your camps."
    const val UNSET = "No camp yet. Tap to add the place you play from."
    const val ADD = "Add camp"
    const val FIRST_STEP = "Your first camp · step 1 of 3"
    const val ADD_TITLE = "New camp"
    const val EDIT_TITLE = "Edit camp"
    const val NAME_LABEL = "Camp name"
    const val NAME_PLACEHOLDER = "Left blank, it gets a name"
    const val TAP_TO_PLACE = "Tap the map to put your camp there."
    const val DRAG_TO_MOVE = "Drag the pin or tap the map to move it. Distances in your games are measured from your first camp."
    const val SAVED = "Camp saved"
    const val SAVE = "Save camp"
    const val WIFI_SAVED = "Wi-Fi saved"
    const val NO_WIFI = "No Wi-Fi"
    const val WIFI_AND_CAR = "Wi-Fi and car"
    const val FINISH_SETUP = "Finish setup"
    const val DELETE = "Delete"
    const val CANCEL = "Cancel"
    const val NEXT = "Next"
    const val WAITING_FOR_LOCATION = "Waiting for your location…"
    const val USE_MY_LOCATION = "Use my location"
    const val EDIT_HINT = "Edit camp"
    const val OPEN_HINT = "Open your camps"
    const val DELETE_BODY = "The Wi-Fi networks saved for it go with it. Games already started keep their quests."
    const val BACK = "Back"

    fun campLabel(name: String?) = name?.ifBlank { null } ?: "a camp"

    fun atCamp(name: String?) = "At ${campLabel(name)}: paused"

    fun title(count: Int) = "$NAME · $count"

    fun deleteTitle(name: String) = "Delete $name?"

    fun deleteDescription(name: String) = "Delete $name"

    fun wifiFor(name: String) = "Wi-Fi for $name"

    fun distance(meters: Double?) = meters?.let { "${Units.distance(it)} away" }
}

/** The "You're at {camp}: add this Wi-Fi?" dialog (see `HomeWifiDialog`). */
internal object HomeOfferText {
    const val ADD = "Add"
    const val MUTE = "Not this one"
    const val LATER = "Later"

    fun title(camp: String?) = "You're at ${CampText.campLabel(camp)}: add this Wi-Fi?"

    fun body(ssid: String) =
        "You are at a camp and connected to \"$ssid\". Add it as this camp's Wi-Fi so the game pauses and GPS turns off while you " +
            "are on it. That saves battery, and nothing counts at a camp."
}
