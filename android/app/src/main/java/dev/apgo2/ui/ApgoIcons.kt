package dev.apgo2.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import com.composables.icons.lucide.Anchor
import com.composables.icons.lucide.Armchair
import com.composables.icons.lucide.ArrowLeftRight
import com.composables.icons.lucide.Ban
import com.composables.icons.lucide.Bike
import com.composables.icons.lucide.Binoculars
import com.composables.icons.lucide.Bird
import com.composables.icons.lucide.BookOpen
import com.composables.icons.lucide.Briefcase
import com.composables.icons.lucide.Building2
import com.composables.icons.lucide.Bus
import com.composables.icons.lucide.Camera
import com.composables.icons.lucide.Car
import com.composables.icons.lucide.Castle
import com.composables.icons.lucide.Check
import com.composables.icons.lucide.ChevronsUp
import com.composables.icons.lucide.Church
import com.composables.icons.lucide.Circle
import com.composables.icons.lucide.CircleCheck
import com.composables.icons.lucide.CircleQuestionMark
import com.composables.icons.lucide.Clock
import com.composables.icons.lucide.Coffee
import com.composables.icons.lucide.Compass
import com.composables.icons.lucide.Croissant
import com.composables.icons.lucide.Crown
import com.composables.icons.lucide.Dog
import com.composables.icons.lucide.Droplet
import com.composables.icons.lucide.Droplets
import com.composables.icons.lucide.Dumbbell
import com.composables.icons.lucide.EllipsisVertical
import com.composables.icons.lucide.Eraser
import com.composables.icons.lucide.FerrisWheel
import com.composables.icons.lucide.Flag
import com.composables.icons.lucide.Flame
import com.composables.icons.lucide.Flower2
import com.composables.icons.lucide.Footprints
import com.composables.icons.lucide.Fuel
import com.composables.icons.lucide.Gamepad2
import com.composables.icons.lucide.GlassWater
import com.composables.icons.lucide.GraduationCap
import com.composables.icons.lucide.Grape
import com.composables.icons.lucide.Heart
import com.composables.icons.lucide.History
import com.composables.icons.lucide.Hourglass
import com.composables.icons.lucide.House
import com.composables.icons.lucide.IceCreamCone
import com.composables.icons.lucide.Landmark
import com.composables.icons.lucide.List
import com.composables.icons.lucide.ListChecks
import com.composables.icons.lucide.Lock
import com.composables.icons.lucide.Lucide
import com.composables.icons.lucide.Mail
import com.composables.icons.lucide.MapPin
import com.composables.icons.lucide.MapPinned
import com.composables.icons.lucide.Mountain
import com.composables.icons.lucide.Music
import com.composables.icons.lucide.Package
import com.composables.icons.lucide.Palette
import com.composables.icons.lucide.Pause
import com.composables.icons.lucide.PawPrint
import com.composables.icons.lucide.Pentagon
import com.composables.icons.lucide.PlugZap
import com.composables.icons.lucide.Plus
import com.composables.icons.lucide.RadioTower
import com.composables.icons.lucide.Redo2
import com.composables.icons.lucide.RefreshCw
import com.composables.icons.lucide.Repeat
import com.composables.icons.lucide.Rocket
import com.composables.icons.lucide.Route
import com.composables.icons.lucide.Sailboat
import com.composables.icons.lucide.ScrollText
import com.composables.icons.lucide.Ship
import com.composables.icons.lucide.ShoppingBasket
import com.composables.icons.lucide.Signpost
import com.composables.icons.lucide.Sparkles
import com.composables.icons.lucide.Star
import com.composables.icons.lucide.Store
import com.composables.icons.lucide.Sun
import com.composables.icons.lucide.Tent
import com.composables.icons.lucide.TrainFront
import com.composables.icons.lucide.Trash2
import com.composables.icons.lucide.TreeDeciduous
import com.composables.icons.lucide.TreePalm
import com.composables.icons.lucide.Trees
import com.composables.icons.lucide.TriangleAlert
import com.composables.icons.lucide.Trophy
import com.composables.icons.lucide.Umbrella
import com.composables.icons.lucide.Undo2
import com.composables.icons.lucide.User
import com.composables.icons.lucide.UtensilsCrossed
import com.composables.icons.lucide.Waves
import com.composables.icons.lucide.X

/**
 * Every icon the app shows, named by what it means. Screens use these, never the icon library directly,
 * so the set can be swapped in one place. Lucide (https://lucide.dev, ISC licence, see THIRD_PARTY_NOTICES.md).
 */
internal object ApgoIcons {
    // Navigation
    val Realms = Lucide.MapPinned
    val NewGame = Lucide.Sparkles
    val Play = Lucide.Flag
    val Activity = Lucide.ScrollText
    val Pause = Lucide.Pause

    // Marks on places
    val Favorite = Lucide.Star
    val Banned = Lucide.Ban

    // Travel modes
    val Walk = Lucide.Footprints
    val Run: ImageVector get() = runner
    val Bike = Lucide.Bike
    val Car = Lucide.Car

    // Actions
    val Add = Lucide.Plus
    val Delete = Lucide.Trash2
    val Rescan = Lucide.RefreshCw
    val Undo = Lucide.Undo2
    val Redo = Lucide.Redo2
    val Finds = Lucide.ListChecks
    val Close = Lucide.X
    val Help = Lucide.CircleQuestionMark
    val Remove = Lucide.Trash2
    val Check = Lucide.Check
    val Done = Lucide.Check
    val Saved = Lucide.CircleCheck
    val Me = Lucide.User
    val Home = Lucide.House
    val Circle = Lucide.Circle
    val Polygon = Lucide.Pentagon
    val More = Lucide.EllipsisVertical
    val All = Lucide.List
    val ClearAll = Lucide.Eraser

    // Status
    val Warning = Lucide.TriangleAlert
    val Unlocked = Lucide.Check
    val Locked = Lucide.Lock

    // A runner, drawn to match Lucide (round 2 px strokes on a 24 px grid), because Lucide has no running figure.
    private val runner: ImageVector by lazy {
        val builder = ImageVector.Builder("Runner", 24.dp, 24.dp, 24f, 24f)
        builder.path(
            fill = null,
            stroke = SolidColor(ApgoPalette.iconStroke),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        ) {
            // head
            moveTo(14.4f, 4.5f)
            arcToRelative(1.6f, 1.6f, 0f, true, true, 3.2f, 0f)
            arcToRelative(1.6f, 1.6f, 0f, true, true, -3.2f, 0f)
            // back, from the neck to the hip
            moveTo(15f, 8f)
            lineTo(12.4f, 13.2f)
            // front arm and back arm
            moveTo(14.8f, 8.6f)
            lineTo(18.2f, 10.4f)
            lineTo(20f, 8.8f)
            moveTo(14.4f, 8.8f)
            lineTo(11f, 10f)
            lineTo(9f, 8.4f)
            // front leg (knee forward, foot down) and back leg (kicked behind)
            moveTo(12.4f, 13.2f)
            lineTo(15.6f, 15.6f)
            lineTo(15f, 20f)
            moveTo(12.4f, 13.2f)
            lineTo(9f, 16.4f)
            lineTo(5.4f, 16.8f)
        }
        builder.build()
    }

    // Icons a realm can be given. The key is what is saved with the realm, so keys are never renamed.
    private val realmIcons: Map<String, ImageVector> =
        linkedMapOf(
            "pin" to Lucide.MapPin,
            "home" to Lucide.House,
            "trees" to Lucide.Trees,
            "mountain" to Lucide.Mountain,
            "building" to Lucide.Building2,
            "coffee" to Lucide.Coffee,
            "waves" to Lucide.Waves,
            "tent" to Lucide.Tent,
            "landmark" to Lucide.Landmark,
            "bike" to Lucide.Bike,
            "school" to Lucide.GraduationCap,
            "dumbbell" to Lucide.Dumbbell,
            "sun" to Lucide.Sun,
            "heart" to Lucide.Heart,
            "star" to Lucide.Star,
            "flag" to Lucide.Flag,
            "compass" to Lucide.Compass,
            "anchor" to Lucide.Anchor,
            "train" to Lucide.TrainFront,
            "flower" to Lucide.Flower2,
            "castle" to Lucide.Castle,
            "palm" to Lucide.TreePalm,
            "store" to Lucide.Store,
            "briefcase" to Lucide.Briefcase,
            "book" to Lucide.BookOpen,
            "dog" to Lucide.Dog,
            "paw" to Lucide.PawPrint,
            "sailboat" to Lucide.Sailboat,
            "rocket" to Lucide.Rocket,
            "gamepad" to Lucide.Gamepad2,
            "music" to Lucide.Music,
            "camera" to Lucide.Camera,
        )

    /** The icons to choose from, as (key, icon). */
    val realmChoices: List<Pair<String, ImageVector>> get() = realmIcons.entries.map { it.key to it.value }

    // Quest families: the fallback icon for any find or quest of that family
    private val families: Map<String, ImageVector> =
        mapOf(
            "reach" to Lucide.MapPin,
            "dwell" to Lucide.Armchair,
            "landmark" to Lucide.Landmark,
            "trail" to Lucide.Route,
            "park" to Lucide.Trees,
            "water" to Lucide.Droplets,
            "courier" to Lucide.Package,
            "explore" to Lucide.Compass,
            "steps" to Lucide.Footprints,
            "away" to Lucide.Rocket,
            "boss" to Lucide.Crown,
        )

    /** The family icons, for pins that mark a whole family. */
    val familyIcons: Map<String, ImageVector> get() = families

    // Quest kinds that deserve a more specific icon than their family's
    private val kinds: Map<String, ImageVector> =
        mapOf(
            "gallery_walls" to Lucide.Palette,
            "mural_mural" to Lucide.Palette,
            "remember_when" to Lucide.History,
            "time_traveler" to Lucide.Hourglass,
            "museum_mile" to Lucide.Landmark,
            "eagle_eye" to Lucide.Binoculars,
            "summit_fever" to Lucide.Mountain,
            "chasing_waterfalls" to Lucide.Waves,
            "tower_defense" to Lucide.RadioTower,
            "flag_day" to Lucide.Flag,
            "hydrant_hunter" to Lucide.Droplet,
            "rack_em_up" to Lucide.Bike,
            "mailbox_maven" to Lucide.Mail,
            "tree_hugger" to Lucide.TreeDeciduous,
            "game_day" to Lucide.Trophy,
            "outdoor_gym" to Lucide.Dumbbell,
            "quiet_please" to Lucide.BookOpen,
            "steeple_chase" to Lucide.Church,
            "make_a_wish" to Lucide.Sparkles,
            "brain_freeze" to Lucide.IceCreamCone,
            "rise_and_shine" to Lucide.Croissant,
            "caffeine_quest" to Lucide.Coffee,
            "market_day" to Lucide.ShoppingBasket,
            "wild_side" to Lucide.PawPrint,
            "merry_go_round" to Lucide.FerrisWheel,
            "thrill_seeker" to Lucide.FerrisWheel,
            "sand_between_toes" to Lucide.Umbrella,
            "spring_fling" to Lucide.Flower2,
            "dam_good_walk" to Lucide.Waves,
            "grape_expectations" to Lucide.Grape,
            "bus_stop_bingo" to Lucide.Bus,
            "all_aboard" to Lucide.TrainFront,
            "know_your_block" to Lucide.Building2,
            "pit_stop" to Lucide.Fuel,
            "hydration_station" to Lucide.GlassWater,
            "plug_in" to Lucide.PlugZap,
            "camp_out" to Lucide.Tent,
            "hut_to_hut" to Lucide.House,
            "spelunker" to Lucide.Mountain,
            "hot_stuff" to Lucide.Flame,
            "campus_crawl" to Lucide.GraduationCap,
            "dock_of_the_bay" to Lucide.Anchor,
            "bench_warmer" to Lucide.Armchair,
            "picnic_break" to Lucide.UtensilsCrossed,
            "under_the_roof" to Lucide.House,
            "scenic_pause" to Lucide.Camera,
            "soak_it_in" to Lucide.Sun,
            "take_five" to Lucide.Clock,
            "touch_grass" to Lucide.Trees,
            "dog_days" to Lucide.Dog,
            "garden_party" to Lucide.Flower2,
            "wild_things" to Lucide.Bird,
            "trail_boss" to Lucide.Signpost,
            "full_circle" to Lucide.Repeat,
            "pedal_pusher" to Lucide.Bike,
            "ridge_runner" to Lucide.Mountain,
            "stairmaster" to Lucide.ChevronsUp,
            "follow_the_flow" to Lucide.Waves,
            "towpath_tour" to Lucide.Ship,
            "street_smarts" to Lucide.MapPin,
            "compass_rose" to Lucide.Compass,
            "special_delivery" to Lucide.Package,
            "there_and_back" to Lucide.ArrowLeftRight,
        )

    /** The icon for a quest kind, falling back to its family's icon. */
    fun forKind(
        kindId: String,
        family: String,
    ): ImageVector = kinds[kindId] ?: families[family] ?: Lucide.MapPin

    fun mode(mode: String): ImageVector =
        when (mode) {
            "run" -> Run
            "bike" -> Bike
            "drive" -> Car
            else -> Walk
        }

    /** A realm's icon, a map pin until one is picked. */
    fun realm(key: String?): ImageVector = realmIcons[key] ?: Lucide.MapPin
}
