package dev.apgo2.ui

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
            "A zone is one of your realms, played in one way of travelling. You start in zone 1. Every later zone" +
                " opens when you have found enough Zone Keys, " +
                "and a zone you reach by bike or by running also needs that tool. The same realm can be used for several zones.",
        )
    val travelMode =
        HelpTopic(
            "Travel by",
            "How you move in this zone: walking, running or biking. It decides how long a quest takes (a bike " +
                "covers more ground in the same time) " +
                "and which quest types make sense: parks and steps are for walking and running, trails also suit a bike. " +
                "A zone that is not walking needs the matching tool to be unlocked.",
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
            "courier" to HelpTopic("Courier", "Pick something up at one spot and deliver it to another, or go out and come back in time."),
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
    val goalTarget =
        HelpTopic(
            "Goal number",
            "Some goals count something. This is the number to reach. Leave it as it is to use the goal's usual number.",
        )
    val goalDescriptions: Map<String, HelpTopic> =
        mapOf(
            "macguffin_short" to HelpTopic("Letter Hunt", "Collect the four letters of APGO. They are hidden as rewards in your quests."),
            "macguffin_long" to HelpTopic("Letter Hunt XL", "Collect every letter of ARCHIPELAGO. A longer hunt."),
            "all_trips" to HelpTopic("Completionist", "Finish every quest in the game."),
            "boss" to HelpTopic("The Big One", "Finish the boss quest: one big quest in the last zone."),
            "treasure_hunt" to HelpTopic("Treasure Hunt", "Collect APGO, then finish the boss quest to claim the treasure."),
            "zone_conqueror" to HelpTopic("Zone Conqueror", "Finish a percentage of the quests in every zone. Set the percentage below."),
            "well_rounded" to HelpTopic("Well Rounded", "Finish at least one quest of every type this game uses."),
            "quest_dex" to HelpTopic("Quest-dex", "Finish many different kinds of quest, not just many quests. Gotta do them all."),
            "marathon" to HelpTopic("Marathon", "Travel a total distance on quests. Set the kilometers below."),
            "explorer" to HelpTopic("Explorer", "Reveal map cells by getting close to them. Pairs well with fog of war."),
            "streak" to HelpTopic("Daily Habit", "Finish at least one quest on this many days in a row."),
            "boss_rush" to HelpTopic("Boss Rush", "Finish this many Hard quests."),
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
    val awayZone =
        HelpTopic(
            "Time away",
            "Count the time you spend away from home only while you are inside one of your game's zones, or " +
                "anywhere. It needs GPS, so it only counts while you are playing.",
        )
    val awayDistance =
        HelpTopic(
            "Away distance",
            "How far from home counts as away. Automatic picks a distance from the size of your realm (about 40% " +
                "of the way to its far edge, between 300 m and 3 km). Switch it off to type your own.",
        )

    // ---- Archipelago
    val archipelago =
        HelpTopic(
            "Archipelago",
            "Join a multiworld: your quests hold items for other players and theirs hold items for you. " +
                "Enter the server address and your slot name, connect, then pick a realm for each zone the game needs.",
        )
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
