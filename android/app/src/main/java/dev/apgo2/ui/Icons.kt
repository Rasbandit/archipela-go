package dev.apgo2.ui

import androidx.compose.ui.graphics.vector.ImageVector
import com.composables.icons.lucide.*

/**
 * Every icon the app shows, named by what it means. Screens use these, never the icon library directly,
 * so the set can be swapped in one place. Lucide (https://lucide.dev, ISC licence, see THIRD_PARTY_NOTICES.md).
 */
object ApgoIcons {
    // Navigation
    val Realms = Lucide.MapPinned
    val NewGame = Lucide.Sparkles
    val Play = Lucide.Flag

    // Marks on places
    val Favorite = Lucide.Star
    val Banned = Lucide.Ban

    // Travel modes
    val Walk = Lucide.Footprints
    val Run = Lucide.Zap
    val Bike = Lucide.Bike
    val Car = Lucide.Car

    // Actions
    val Add = Lucide.Plus
    val Delete = Lucide.Trash2
    val Rescan = Lucide.RefreshCw
    val Undo = Lucide.Undo2
    val Close = Lucide.X
    val All = Lucide.List
    val ClearAll = Lucide.Eraser

    // Status
    val Warning = Lucide.TriangleAlert
    val Unlocked = Lucide.Check
    val Locked = Lucide.Lock

    // Quest families: the fallback icon for any find or quest of that family
    private val families: Map<String, ImageVector> = mapOf(
        "reach" to Lucide.MapPin, "dwell" to Lucide.Armchair, "landmark" to Lucide.Landmark, "trail" to Lucide.Route,
        "park" to Lucide.Trees, "water" to Lucide.Droplets, "courier" to Lucide.Package, "explore" to Lucide.Compass,
        "steps" to Lucide.Footprints, "away" to Lucide.Rocket, "boss" to Lucide.Crown,
    )

    // Quest kinds that deserve a more specific icon than their family's
    private val kinds: Map<String, ImageVector> = mapOf(
        "gallery_walls" to Lucide.Palette, "mural_mural" to Lucide.Palette, "remember_when" to Lucide.History, "time_traveler" to Lucide.Hourglass,
        "museum_mile" to Lucide.Landmark, "eagle_eye" to Lucide.Binoculars, "summit_fever" to Lucide.Mountain, "chasing_waterfalls" to Lucide.Waves,
        "tower_defense" to Lucide.RadioTower, "flag_day" to Lucide.Flag, "hydrant_hunter" to Lucide.Droplet, "rack_em_up" to Lucide.Bike,
        "mailbox_maven" to Lucide.Mail, "tree_hugger" to Lucide.TreeDeciduous, "game_day" to Lucide.Trophy, "outdoor_gym" to Lucide.Dumbbell,
        "quiet_please" to Lucide.BookOpen, "steeple_chase" to Lucide.Church, "make_a_wish" to Lucide.Sparkles, "brain_freeze" to Lucide.IceCreamCone,
        "rise_and_shine" to Lucide.Croissant, "caffeine_quest" to Lucide.Coffee, "market_day" to Lucide.ShoppingBasket, "wild_side" to Lucide.PawPrint,
        "merry_go_round" to Lucide.FerrisWheel, "thrill_seeker" to Lucide.FerrisWheel, "sand_between_toes" to Lucide.Umbrella, "spring_fling" to Lucide.Flower2,
        "dam_good_walk" to Lucide.Waves, "grape_expectations" to Lucide.Grape, "bus_stop_bingo" to Lucide.Bus, "all_aboard" to Lucide.TrainFront,
        "know_your_block" to Lucide.Building2, "pit_stop" to Lucide.Fuel, "hydration_station" to Lucide.GlassWater, "plug_in" to Lucide.PlugZap,
        "camp_out" to Lucide.Tent, "hut_to_hut" to Lucide.House, "spelunker" to Lucide.Mountain, "hot_stuff" to Lucide.Flame,
        "campus_crawl" to Lucide.GraduationCap, "dock_of_the_bay" to Lucide.Anchor,
        "bench_warmer" to Lucide.Armchair, "picnic_break" to Lucide.UtensilsCrossed, "under_the_roof" to Lucide.House, "scenic_pause" to Lucide.Camera,
        "soak_it_in" to Lucide.Sun, "take_five" to Lucide.Clock,
        "touch_grass" to Lucide.Trees, "dog_days" to Lucide.Dog, "garden_party" to Lucide.Flower2, "wild_things" to Lucide.Bird,
        "trail_boss" to Lucide.Signpost, "full_circle" to Lucide.Repeat, "pedal_pusher" to Lucide.Bike, "ridge_runner" to Lucide.Mountain, "stairmaster" to Lucide.ChevronsUp,
        "follow_the_flow" to Lucide.Waves, "towpath_tour" to Lucide.Ship,
        "street_smarts" to Lucide.MapPin, "compass_rose" to Lucide.Compass, "special_delivery" to Lucide.Package, "there_and_back" to Lucide.ArrowLeftRight,
    )

    /** The icon for a quest kind, falling back to its family's icon. */
    fun forKind(kindId: String, family: String): ImageVector = kinds[kindId] ?: families[family] ?: Lucide.MapPin

    /** The family icons, for pins that mark a whole family. */
    val familyIcons: Map<String, ImageVector> get() = families

    fun mode(mode: String): ImageVector = when (mode) {
        "run" -> Run
        "bike" -> Bike
        "drive" -> Car
        else -> Walk
    }
}
