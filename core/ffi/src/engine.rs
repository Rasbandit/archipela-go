//! UniFFI facade over the game engine: realms, scanning, game setup, play. Blocking calls; call from a background thread.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use apgo_core::assign::SurfacePref;
use apgo_core::assign::Target;
use apgo_core::catalog::{Catalog, Mode};
use apgo_core::game::{Backend, Event, Game, NewGame, QuestState};
use apgo_core::geo::{distance_m, Point};
use apgo_core::marks::Mark;
use apgo_core::realm::{Realm, RealmStore, Shape};
use apgo_core::scan::{scan_realm, Atlas};
use apgo_core::slot::SlotData;
use apgo_core::solo::{generate, SoloOptions};
use apgo_core::verify::Fix;
use apgo_core::yaml::build_yaml;

use crate::{CoreError, GeoPoint};

fn err<E: ToString>(e: E) -> CoreError {
    CoreError::Failed { detail: e.to_string() }
}

fn gp(p: Point) -> GeoPoint {
    GeoPoint { lat: p.lat, lon: p.lon }
}

fn pt(p: &GeoPoint) -> Point {
    Point::new(p.lat, p.lon)
}

#[derive(Debug, uniffi::Record)]
pub struct CircleOut {
    pub center: GeoPoint,
    pub radius_m: f64,
}

#[derive(Debug, uniffi::Record)]
pub struct RealmOut {
    pub id: String,
    pub name: String,
    /// The icon picked for the realm, if any.
    pub icon: Option<String>,
    /// The circle if the realm has one (active or kept in reserve).
    pub circle: Option<CircleOut>,
    /// The polygon corners (empty if none), active or kept in reserve.
    pub polygon: Vec<GeoPoint>,
    /// Which of the two is the realm's real outline.
    pub polygon_active: bool,
    pub scanned_at_ms: Option<u64>,
    pub places: u32,
    /// Set when a scan stopped early (slow public map servers); Rescan continues from the cache.
    pub warning: Option<String>,
}

/// A find reduced to what a small map preview needs.
#[derive(Debug, uniffi::Record)]
pub struct DotOut {
    pub at: GeoPoint,
    pub kind_id: String,
    pub family: String,
}

/// A quest kind a find can serve, with what it means and how it is completed.
#[derive(Debug, uniffi::Record)]
pub struct KindOut {
    pub id: String,
    pub name: String,
    pub family: String,
    pub blurb: String,
    /// What the player has to do ("Get within 40 m.").
    pub how: String,
}

/// One find: a scanned spot a realm can use for quests, with the player's mark on it.
#[derive(Debug, uniffi::Record)]
pub struct FindOut {
    pub id: String,
    /// The find's own name, or the name of its first quest kind when it has none ("Bench Warmer").
    pub name: String,
    pub named: bool,
    /// The quest kinds this find can serve.
    pub kinds: Vec<KindOut>,
    /// The `key=value` map tags that made it match, for the curious ("leisure=pitch").
    pub tags: Vec<String>,
    /// The first quest kind's id and family, for choosing an icon.
    pub kind_id: String,
    pub family: String,
    pub at: GeoPoint,
    pub distance_m: f64,
    /// "none" | "favorite" | "banned"
    pub mark: String,
}

#[derive(Debug, uniffi::Record)]
pub struct OfferOut {
    pub kind_id: String,
    pub name: String,
    pub family: String,
    pub blurb: String,
    pub count: u32,
}

#[derive(Debug, uniffi::Record)]
pub struct SoloOptionsIn {
    pub goal: String,
    pub goal_target: u32,
    pub number_of_trips: u32,
    pub zone_modes: Vec<String>,
    pub easy_share: u32,
    pub medium_share: u32,
    pub hard_share: u32,
    pub minutes_per_tier: u32,
    pub min_distance_m: u32,
    pub quest_types: Vec<String>,
    pub enabled_traps: Vec<String>,
    pub trap_rate: u32,
    pub enable_effort_reductions: bool,
    pub enable_scouting: bool,
    pub enable_collection: bool,
    pub reduction_percent: u32,
    pub fog_of_war: bool,
    pub return_home: bool,
}

fn to_core(o: SoloOptionsIn) -> Result<SoloOptions, CoreError> {
    let zone_modes = o.zone_modes.iter().map(|m| Mode::parse(m).ok_or_else(|| err(format!("unknown mode {m}")))).collect::<Result<Vec<_>, _>>()?;
    Ok(SoloOptions {
        goal: o.goal,
        goal_target: o.goal_target,
        number_of_trips: o.number_of_trips,
        zone_modes,
        easy_share: o.easy_share,
        medium_share: o.medium_share,
        hard_share: o.hard_share,
        minutes_per_tier: o.minutes_per_tier,
        min_distance_m: o.min_distance_m,
        quest_types: o.quest_types,
        enabled_traps: o.enabled_traps,
        trap_rate: o.trap_rate,
        enable_effort_reductions: o.enable_effort_reductions,
        enable_scouting: o.enable_scouting,
        enable_collection: o.enable_collection,
        reduction_percent: o.reduction_percent,
        fog_of_war: o.fog_of_war,
        return_home: o.return_home,
    })
}

#[derive(Debug, uniffi::Record)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, uniffi::Record)]
pub struct QuestOut {
    pub location_id: i64,
    pub zone: u32,
    pub name: String,
    pub place: String,
    pub family: String,
    pub kind_id: String,
    pub difficulty: String,
    pub tier: u8,
    pub effort_min: f64,
    pub mode: String,
    /// locked | hidden | open | progress | done
    pub state: String,
    pub progress: f32,
    /// point | dwell | area | line | courier | roundtrip | cells | steps | away
    pub shape: String,
    pub anchor: Option<GeoPoint>,
    pub anchor_b: Option<GeoPoint>,
    pub radius_m: f64,
    pub path: Vec<GeoPoint>,
    pub detail: String,
    pub fallback: bool,
    pub boss: bool,
    pub blurb: String,
    pub reward: Option<String>,
}

#[derive(Debug, uniffi::Record)]
pub struct ZoneOut {
    pub id: u32,
    pub mode: String,
    pub unlocked: bool,
    pub keys_needed: u32,
    pub tool: Option<String>,
    pub realm_name: String,
}

#[derive(Debug, uniffi::Record)]
pub struct HudOut {
    pub goal_label: String,
    pub goal_progress: f32,
    pub goal_achieved: bool,
    pub done: u32,
    pub total: u32,
    pub keys: u32,
    pub tools: Vec<String>,
    pub letters: String,
    pub traps: Vec<String>,
    pub thaw: Option<GeoPoint>,
    pub waypoint: Option<GeoPoint>,
    pub blocked: Option<String>,
    pub distance_km: f64,
    pub streak_days: u32,
    pub fog: bool,
    pub backend: String,
    pub game_name: String,
}

#[derive(Debug, uniffi::Enum)]
pub enum EventOut {
    QuestDone { location_id: i64, name: String },
    SendCheck { location_id: i64 },
    Reward { location_id: i64, item: String },
    ZoneUnlocked { zone: u32 },
    Trap { item: String, message: String },
    ShuffleRequested,
    Discovered { location_id: i64 },
    GoalAchieved { label: String },
    Info { text: String },
}

fn ev_out(e: Event) -> EventOut {
    match e {
        Event::QuestDone { location_id, name } => EventOut::QuestDone { location_id, name },
        Event::SendCheck { location_id } => EventOut::SendCheck { location_id },
        Event::Reward { location_id, item } => EventOut::Reward { location_id, item },
        Event::ZoneUnlocked { zone } => EventOut::ZoneUnlocked { zone },
        Event::Trap { item, message } => EventOut::Trap { item, message },
        Event::ShuffleRequested => EventOut::ShuffleRequested,
        Event::Discovered { location_id } => EventOut::Discovered { location_id },
        Event::GoalAchieved { label } => EventOut::GoalAchieved { label },
        Event::Info { text } => EventOut::Info { text },
    }
}

fn describe(t: &Target) -> (&'static str, Option<Point>, Option<Point>, f64, Vec<Point>, String) {
    match t {
        Target::Point { p, r } => ("point", Some(*p), None, *r, vec![], format!("Get within {r:.0} m")),
        Target::Dwell { p, r, minutes } => ("dwell", Some(*p), None, *r, vec![], format!("Stay {minutes:.0} min within {r:.0} m")),
        Target::DwellArea { poly, center, r, minutes } => ("area", Some(*center), None, *r, poly.clone(), format!("Spend {minutes:.0} min inside the area")),
        Target::Line { pts, corridor_m, coverage } => {
            let km = apgo_core::geo::polyline_len_m(pts) / 1000.0;
            ("line", pts.first().copied(), None, *corridor_m, pts.clone(), format!("Cover {:.0}% of this {km:.1} km path", coverage * 100.0))
        }
        Target::Courier { a, b, r, time_limit_min } => {
            ("courier", Some(*a), Some(*b), *r, vec![], format!("Pick up at A, deliver to B within {time_limit_min:.0} min"))
        }
        Target::RoundTrip { far, r, time_limit_min } => {
            ("roundtrip", Some(*far), None, *r, vec![], format!("Reach the far point and be back home within {time_limit_min:.0} min"))
        }
        Target::Cells { n, cell_m } => ("cells", None, None, *cell_m, vec![], format!("Visit {n} new map cells")),
        Target::Steps { n } => ("steps", None, None, 0.0, vec![], format!("Take {n} steps")),
        Target::Away { min_distance_m, minutes } => {
            ("away", None, None, *min_distance_m, vec![], format!("Spend {minutes:.0} min at least {:.1} km from home", min_distance_m / 1000.0))
        }
    }
}

#[derive(uniffi::Object)]
pub struct Engine {
    dir: PathBuf,
    catalog: Catalog,
    game: Mutex<Option<Game>>,
}

impl Engine {
    fn store(&self) -> RealmStore {
        RealmStore::new(&self.dir)
    }

    fn cache(&self) -> PathBuf {
        self.dir.join("http-cache")
    }

    fn with_game<T>(&self, f: impl FnOnce(&mut Game) -> T) -> Option<T> {
        self.game.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(f)
    }

    /// The realm's scanned atlas, restricted to the realm's current zone (see `Atlas::restrict_to`).
    fn zoned_atlas(&self, realm: &Realm) -> Option<Atlas> {
        let mut a = self.store().load_atlas(&realm.id)?;
        a.restrict_to(&realm.shape.to_zone());
        Some(a)
    }

    /// Finds of a (zoned) atlas: place index -> the quest kinds it can serve for this realm's mode.
    fn kinds_by_place<'a>(&'a self, atlas: &Atlas) -> std::collections::BTreeMap<usize, Vec<&'a apgo_core::catalog::Kind>> {
        let mut out: std::collections::BTreeMap<usize, Vec<&apgo_core::catalog::Kind>> = std::collections::BTreeMap::new();
        for (kind_id, idxs) in &atlas.matches {
            let Some(kind) = self.catalog.kind(kind_id).filter(|k| Mode::PLAY.iter().any(|&m| k.allows(m))) else { continue };
            for &i in idxs {
                out.entry(i).or_default().push(kind);
            }
        }
        out
    }

    fn realm_atlases(&self, ids: &[String]) -> Result<Vec<(Realm, Atlas)>, CoreError> {
        let store = self.store();
        ids.iter()
            .map(|id| {
                let r = store.get(id).ok_or_else(|| err(format!("realm {id} not found")))?;
                let mut a = self.zoned_atlas(&r).ok_or_else(|| err(format!("realm \"{}\" has not been scanned yet", r.name)))?;
                a.apply_marks(&store.marks(id)); // banned places are left out, favorites are preferred
                Ok((r, a))
            })
            .collect()
    }

    fn home_for(&self, realms: &[(Realm, Atlas)]) -> Point {
        self.store().home().or_else(|| realms.first().map(|(r, _)| r.shape.center())).unwrap_or(Point::new(0.0, 0.0))
    }

    fn offers_of(&self, atlas: &Atlas) -> Vec<OfferOut> {
        let mut v: Vec<OfferOut> = atlas
            .offers(&self.catalog, &Mode::PLAY)
            .into_iter()
            .filter_map(|(id, count)| {
                self.catalog.kind(&id).map(|k| OfferOut { kind_id: id, name: k.name.clone(), family: k.family.clone(), blurb: k.blurb.clone(), count })
            })
            .collect();
        v.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
        v
    }
}

#[uniffi::export]
impl Engine {
    #[uniffi::constructor]
    pub fn new(dir: String) -> Arc<Self> {
        let dir = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self { dir, catalog: Catalog::builtin(), game: Mutex::new(None) })
    }

    pub fn catalog_size(&self) -> u32 {
        self.catalog.kinds.len() as u32
    }

    // ---------- realms ----------
    pub fn realms(&self) -> Vec<RealmOut> {
        let store = self.store();
        store
            .list()
            .into_iter()
            .map(|r| {
                let circle = r.circle().map(|(center, radius_m)| CircleOut { center: gp(center), radius_m });
                let polygon: Vec<GeoPoint> = r.polygon().unwrap_or_default().iter().map(|p| gp(*p)).collect();
                let polygon_active = r.polygon_active();
                let atlas = store.load_atlas(&r.id);
                // Count finds (zoned, usable by this realm's mode), the same number the Details list shows.
                let places = self.zoned_atlas(&r).map_or(0, |a| self.kinds_by_place(&a).len() as u32);
                let warning = atlas.and_then(|a| a.warnings.first().cloned());
                RealmOut { id: r.id, name: r.name, icon: r.icon.clone(), circle, polygon, polygon_active, scanned_at_ms: r.scanned_at_ms, places, warning }
            })
            .collect()
    }

    /// Saves a realm with both outlines it has; `polygon_active` picks the real one, the other is kept in reserve.
    pub fn save_realm(
        &self,
        id: String,
        name: String,
        icon: Option<String>,
        circle: Option<CircleOut>,
        polygon: Vec<GeoPoint>,
        polygon_active: bool,
    ) -> Result<(), CoreError> {
        let circle = circle.map(|c| Shape::Circle { center: pt(&c.center), radius_m: c.radius_m });
        let polygon = (!polygon.is_empty()).then(|| Shape::Polygon { vertices: polygon.iter().map(pt).collect() });
        let (shape, spare) = match (polygon_active, circle, polygon) {
            (true, c, Some(p)) => (p, c),
            (false, Some(c), p) => (c, p),
            _ => return Err(err("the active outline is missing")),
        };
        let prev = self.store().get(&id);
        self.store().save(&Realm { id, name, icon, shape, spare, scanned_at_ms: prev.and_then(|p| p.scanned_at_ms) }).map_err(err)
    }

    pub fn delete_realm(&self, id: String) -> Result<(), CoreError> {
        self.store().delete(&id).map_err(err)
    }

    /// Scan the realm over the network (cached) and save its atlas. Returns what it offers.
    pub fn scan_realm(&self, id: String, now_ms: u64) -> Result<Vec<OfferOut>, CoreError> {
        let store = self.store();
        let mut realm = store.get(&id).ok_or_else(|| err("realm not found"))?;
        let atlas = scan_realm(&realm, &self.catalog, Some(&self.cache()), now_ms).map_err(err)?;
        store.save_atlas(&atlas).map_err(err)?;
        realm.scanned_at_ms = Some(now_ms);
        store.save(&realm).map_err(err)?;
        let mut zoned = atlas;
        zoned.restrict_to(&realm.shape.to_zone());
        zoned.apply_marks(&store.marks(&id));
        Ok(self.offers_of(&zoned))
    }

    pub fn realm_offers(&self, id: String) -> Vec<OfferOut> {
        let store = self.store();
        match store.get(&id).and_then(|r| self.zoned_atlas(&r)) {
            Some(mut a) => {
                a.apply_marks(&store.marks(&id));
                self.offers_of(&a)
            }
            None => vec![],
        }
    }

    /// An evenly spread sample of at most `max` of a realm's usable finds (banned ones left out), for drawing a preview of the realm.
    pub fn realm_dots(&self, id: String, max: u32) -> Vec<DotOut> {
        let store = self.store();
        let Some(mut atlas) = store.get(&id).and_then(|r| self.zoned_atlas(&r)) else { return vec![] };
        atlas.apply_marks(&store.marks(&id));
        let places = self.kinds_by_place(&atlas);
        let stride = (places.len() / max.max(1) as usize).max(1);
        places
            .into_iter()
            .step_by(stride)
            .map(|(i, kinds)| DotOut { at: gp(atlas.features[i].point), kind_id: kinds[0].id.clone(), family: kinds[0].family.clone() })
            .collect()
    }

    /// Every find in a realm (a scanned spot that can serve a quest), with the player's mark on it, nearest first.
    pub fn realm_finds(&self, id: String) -> Vec<FindOut> {
        let store = self.store();
        let Some((realm, atlas)) = store.get(&id).and_then(|r| self.zoned_atlas(&r).map(|a| (r, a))) else { return vec![] };
        let marks = store.marks(&id);
        let home = store.home().unwrap_or_else(|| realm.shape.center());
        let kinds_of = self.kinds_by_place(&atlas);
        let mut out: Vec<FindOut> = kinds_of
            .into_iter()
            .map(|(i, kinds)| {
                let f = &atlas.features[i];
                FindOut {
                    id: f.id.clone(),
                    name: f.name.clone().unwrap_or_else(|| kinds[0].name.clone()),
                    named: f.name.is_some(),
                    kind_id: kinds[0].id.clone(),
                    family: kinds[0].family.clone(),
                    tags: {
                        let mut t: Vec<String> = kinds.iter().flat_map(|k| k.evidence(&f.tags)).collect();
                        t.sort();
                        t.dedup();
                        t
                    },
                    at: gp(f.point),
                    distance_m: distance_m(home, f.point),
                    mark: match marks.get(&f.id) {
                        Mark::None => "none",
                        Mark::Favorite => "favorite",
                        Mark::Banned => "banned",
                    }
                    .into(),
                    kinds: kinds
                        .into_iter()
                        .map(|k| KindOut { id: k.id.clone(), name: k.name.clone(), family: k.family.clone(), blurb: k.blurb.clone(), how: k.verify.how() })
                        .collect(),
                }
            })
            .collect();
        out.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m));
        out
    }

    /// `mark` is "none", "favorite" or "banned". Takes effect the next time quests are made or re-rolled.
    pub fn set_find_mark(&self, realm_id: String, find_id: String, mark: String) -> Result<(), CoreError> {
        let mark = match mark.as_str() {
            "none" => Mark::None,
            "favorite" => Mark::Favorite,
            "banned" => Mark::Banned,
            other => return Err(err(format!("unknown mark {other}"))),
        };
        let store = self.store();
        let mut marks = store.marks(&realm_id);
        marks.set(&find_id, mark);
        store.save_marks(&realm_id, &marks).map_err(err)
    }

    pub fn home(&self) -> Option<GeoPoint> {
        self.store().home().map(gp)
    }

    pub fn set_home(&self, p: GeoPoint) -> Result<(), CoreError> {
        self.store().set_home(pt(&p)).map_err(err)
    }

    // ---------- game setup ----------
    pub fn build_yaml(&self, player: String, o: SoloOptionsIn) -> Result<String, CoreError> {
        Ok(build_yaml(&player, &to_core(o)?))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_solo(
        &self,
        game_id: String,
        name: String,
        o: SoloOptionsIn,
        zone_realms: Vec<String>,
        seed: u64,
        surface: String,
        avoid_stairs: bool,
    ) -> Result<(), CoreError> {
        let opts = to_core(o)?;
        if zone_realms.len() != opts.zone_modes.len() {
            return Err(err("pick one realm per zone"));
        }
        let generated = generate(&opts, seed).map_err(err)?;
        let realms = self.realm_atlases(&zone_realms)?;
        let home = self.home_for(&realms);
        let game = Game::create(
            NewGame {
                id: game_id,
                name,
                backend: Backend::Solo,
                seed_name: format!("solo-{seed}"),
                slot: generated.slot,
                zone_realms,
                realms: &realms,
                home,
                seed,
                solo_rewards: generated.rewards,
                surface: SurfacePref::parse(&surface),
                avoid_stairs,
            },
            &self.catalog,
        )
        .map_err(err)?;
        game.save(&self.dir).map_err(err)?;
        *self.game.lock().unwrap_or_else(|e| e.into_inner()) = Some(game);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_archipelago(
        &self,
        game_id: String,
        name: String,
        slot_json: String,
        seed_name: String,
        zone_realms: Vec<String>,
        seed: u64,
        surface: String,
        avoid_stairs: bool,
    ) -> Result<(), CoreError> {
        let slot = SlotData::from_json(&slot_json).map_err(err)?;
        let realms = self.realm_atlases(&zone_realms)?;
        let home = self.home_for(&realms);
        let game = Game::create(
            NewGame {
                id: game_id,
                name,
                backend: Backend::Archipelago,
                seed_name,
                slot,
                zone_realms,
                realms: &realms,
                home,
                seed,
                solo_rewards: Default::default(),
                surface: SurfacePref::parse(&surface),
                avoid_stairs,
            },
            &self.catalog,
        )
        .map_err(err)?;
        game.save(&self.dir).map_err(err)?;
        *self.game.lock().unwrap_or_else(|e| e.into_inner()) = Some(game);
        Ok(())
    }

    /// The zone modes a connected Archipelago game needs, so the app can ask for matching realms.
    pub fn slot_zone_modes(&self, slot_json: String) -> Result<Vec<String>, CoreError> {
        Ok(SlotData::from_json(&slot_json).map_err(err)?.zones.iter().map(|z| z.mode.name().to_string()).collect())
    }

    pub fn games(&self) -> Vec<GameInfo> {
        Game::list_ids(&self.dir).into_iter().map(|(id, name)| GameInfo { id, name }).collect()
    }

    pub fn open_game(&self, id: String) -> Result<(), CoreError> {
        let g = Game::load(&self.dir, &id).map_err(err)?;
        *self.game.lock().unwrap_or_else(|e| e.into_inner()) = Some(g);
        Ok(())
    }

    pub fn close_game(&self) {
        *self.game.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn delete_game(&self, id: String) -> Result<(), CoreError> {
        let _ = std::fs::remove_file(Game::path_for(&self.dir, &id));
        let mut g = self.game.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_ref().is_some_and(|x| x.id == id) {
            *g = None;
        }
        Ok(())
    }

    pub fn has_game(&self) -> bool {
        self.game.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    // ---------- play ----------
    pub fn quests(&self) -> Vec<QuestOut> {
        self.with_game(|g| {
            g.quest_views()
                .into_iter()
                .map(|q| {
                    let (shape, anchor, anchor_b, radius_m, path, detail) = describe(&q.target);
                    QuestOut {
                        location_id: q.location_id,
                        zone: q.zone,
                        name: q.name,
                        place: q.place,
                        family: q.family,
                        kind_id: q.kind_id,
                        difficulty: q.difficulty,
                        tier: q.tier,
                        effort_min: q.effort_min,
                        mode: q.mode.name().into(),
                        state: match q.state {
                            QuestState::Locked => "locked",
                            QuestState::Hidden => "hidden",
                            QuestState::Open => "open",
                            QuestState::InProgress => "progress",
                            QuestState::Done => "done",
                        }
                        .into(),
                        progress: q.progress,
                        shape: shape.into(),
                        anchor: anchor.map(gp),
                        anchor_b: anchor_b.map(gp),
                        radius_m,
                        path: path.into_iter().map(gp).collect(),
                        detail,
                        fallback: q.fallback,
                        boss: q.boss,
                        blurb: q.blurb,
                        reward: q.reward,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn zones(&self) -> Vec<ZoneOut> {
        let store = self.store();
        self.with_game(|g| {
            g.slot
                .zones
                .iter()
                .enumerate()
                .map(|(i, z)| ZoneOut {
                    id: z.id,
                    mode: z.mode.name().into(),
                    unlocked: g.zone_unlocked(z.id),
                    keys_needed: z.zone_keys_needed,
                    tool: z.tool.clone(),
                    realm_name: g.zone_realms.get(i).and_then(|id| store.get(id)).map(|r| r.name).unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn hud(&self, now_ms: i64) -> Option<HudOut> {
        self.with_game(|g| {
            let s = g.goal_status(now_ms);
            let views = g.quest_views();
            let mut letters: Vec<char> = g.items.iter().filter_map(|i| i.strip_prefix("Letter ").and_then(|s| s.chars().next())).collect();
            letters.sort_unstable();
            let tools: Vec<String> = ["Running Shoes", "Bike", "Car"].iter().filter(|t| g.items.iter().any(|i| i == *t)).map(|t| t.to_string()).collect();
            HudOut {
                goal_label: s.label,
                goal_progress: s.progress,
                goal_achieved: s.achieved,
                done: views.iter().filter(|v| v.state == QuestState::Done).count() as u32,
                total: views.len() as u32,
                keys: g.items.iter().filter(|i| *i == "Progressive Zone Key").count() as u32,
                tools,
                letters: letters.into_iter().collect(),
                traps: g.trap_labels(),
                thaw: g.traps.thaw_point().map(gp),
                waypoint: g.traps.waypoint().map(gp),
                blocked: g.blocked_reason(),
                distance_km: g.stats.distance_m / 1000.0,
                streak_days: g.streak_days(now_ms),
                fog: g.slot.fog_of_war,
                backend: if g.backend == Backend::Solo { "solo".into() } else { "archipelago".into() },
                game_name: g.name.clone(),
            }
        })
    }

    pub fn on_fix(&self, lat: f64, lon: f64, t_ms: i64, accuracy_m: f64, steps: Option<i64>) -> Vec<EventOut> {
        let dir = self.dir.clone();
        self.with_game(|g| {
            let ev = g.on_fix(Fix { lat, lon, t_ms, accuracy_m }, steps);
            if !ev.is_empty() {
                let _ = g.save(&dir);
            }
            ev.into_iter().map(ev_out).collect()
        })
        .unwrap_or_default()
    }

    /// Archipelago: pass the full received-item name list; new items trigger unlocks/traps.
    pub fn sync_items(&self, items: Vec<String>, now_ms: i64, pos: Option<GeoPoint>) -> Vec<EventOut> {
        let dir = self.dir.clone();
        self.with_game(|g| {
            let ev = g.sync_items(&items, now_ms, pos.as_ref().map(pt));
            if !ev.is_empty() {
                let _ = g.save(&dir);
            }
            ev.into_iter().map(ev_out).collect()
        })
        .unwrap_or_default()
    }

    pub fn mark_checked(&self, ids: Vec<i64>, now_ms: i64) {
        let dir = self.dir.clone();
        self.with_game(|g| {
            g.mark_checked(&ids, now_ms);
            let _ = g.save(&dir);
        });
    }

    /// Reroll unfinished quests (all if `ids` is empty). Returns how many were re-placed.
    pub fn reroll(&self, ids: Vec<i64>, seed: u64) -> Result<u32, CoreError> {
        let (realm_ids, all): (Vec<String>, Vec<i64>) =
            self.with_game(|g| (g.zone_realms.clone(), g.assignments.iter().map(|a| a.location_id).collect())).ok_or_else(|| err("no game open"))?;
        let realms = self.realm_atlases(&realm_ids)?;
        let ids = if ids.is_empty() { all } else { ids };
        let dir = self.dir.clone();
        let catalog = &self.catalog;
        self.with_game(|g| {
            g.reroll(&ids, &realms, seed, catalog).map(|n| {
                let _ = g.save(&dir);
                n as u32
            })
        })
        .ok_or_else(|| err("no game open"))?
        .map_err(err)
    }

    pub fn save_game(&self) {
        let dir = self.dir.clone();
        self.with_game(|g| {
            let _ = g.save(&dir);
        });
    }
}
