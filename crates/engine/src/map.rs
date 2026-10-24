//! Static map data (territories, sea zones, adjacency, canals) loaded from `data/map.json`.
use crate::power::Power;
use crate::units::UnitType;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};
use std::ops::{Index, IndexMut};
use std::sync::OnceLock;

const MAP_JSON: &str = include_str!("../data/map.json");

/// Small integer index of a land territory or sea zone. Serialises as its string id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocationId(pub u16);

impl LocationId {
    /// Index into per-location vectors.
    pub fn idx(self) -> usize {
        self.0 as usize
    }
    /// String id (e.g. `"russia"`, `"sz_baltic"`).
    pub fn id(self) -> &'static str {
        &MapData::get().locs[self.idx()].id
    }
    /// Look up by string id.
    pub fn from_id(s: &str) -> Option<LocationId> {
        MapData::get().id_of(s)
    }
    /// True for sea zones.
    pub fn is_sea(self) -> bool {
        MapData::get().is_sea(self)
    }
}

impl Serialize for LocationId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id())
    }
}
impl<'de> Deserialize<'de> for LocationId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        MapData::get()
            .id_of(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown location id {s:?}")))
    }
}

/// A value per location. Serialises as an object keyed by location id, omitting default values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocMap<T>(pub Vec<T>);

impl<T: Default + Clone> LocMap<T> {
    /// A map with every entry defaulted.
    pub fn new() -> Self {
        LocMap(vec![T::default(); MapData::get().len()])
    }
}
impl<T: Default + Clone> Default for LocMap<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> Index<LocationId> for LocMap<T> {
    type Output = T;
    fn index(&self, l: LocationId) -> &T {
        &self.0[l.idx()]
    }
}
impl<T> IndexMut<LocationId> for LocMap<T> {
    fn index_mut(&mut self, l: LocationId) -> &mut T {
        &mut self.0[l.idx()]
    }
}
impl<T: Serialize + Default + PartialEq> Serialize for LocMap<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let def = T::default();
        let mut m = s.serialize_map(None)?;
        for (i, v) in self.0.iter().enumerate() {
            if *v != def {
                m.serialize_entry(LocationId(i as u16).id(), v)?;
            }
        }
        m.end()
    }
}
impl<'de, T: Deserialize<'de> + Default + Clone> Deserialize<'de> for LocMap<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw: BTreeMap<String, T> = BTreeMap::deserialize(d)?;
        let mut out = LocMap::<T>::new();
        for (k, v) in raw {
            let id = MapData::get()
                .id_of(&k)
                .ok_or_else(|| serde::de::Error::custom(format!("unknown location id {k:?}")))?;
            out[id] = v;
        }
        Ok(out)
    }
}

/// One territory or sea zone.
#[derive(Clone, Debug)]
pub struct Location {
    /// String id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// True for sea zones.
    pub is_sea: bool,
    /// IPC value (0 for sea).
    pub ipc: u32,
    /// Original owner (None for neutrals / sea).
    pub original_owner: Option<Power>,
    /// Untouched neutral territory.
    pub neutral: bool,
    /// Capital of this power, if any.
    pub capital_of: Option<Power>,
    /// Starts with an industrial complex.
    pub has_original_complex: bool,
    /// Island flag.
    pub is_island: bool,
    /// All adjacent locations.
    pub neighbors: Vec<LocationId>,
}

/// A canal restricting ship movement between two sea zones.
#[derive(Clone, Debug)]
pub struct Canal {
    /// Name ("Suez Canal").
    pub name: String,
    /// The two sea zones joined.
    pub sea_zones: [LocationId; 2],
    /// Territories that must be friendly-controlled at turn start for passage.
    pub land_territories: Vec<LocationId>,
}

/// A starting stack from the setup.
#[derive(Clone, Debug)]
pub struct StartingStack {
    /// Where.
    pub location: LocationId,
    /// Who.
    pub owner: Power,
    /// What.
    pub unit: UnitType,
    /// How many.
    pub count: u32,
}

/// Immutable map and setup data.
#[derive(Debug)]
pub struct MapData {
    locs: Vec<Location>,
    by_id: HashMap<String, LocationId>,
    by_name: HashMap<String, LocationId>,
    land_count: usize,
    canals: Vec<Canal>,
    starting: Vec<StartingStack>,
    capitals: [LocationId; 5],
    starting_ipcs: [u32; 5],
}

#[derive(Deserialize)]
struct RawPower {
    id: String,
    capital: String,
    starting_ipcs: u32,
}
#[derive(Deserialize)]
struct RawTerr {
    id: String,
    name: String,
    ipc: u32,
    original_owner: Option<String>,
    neutral: bool,
    is_capital: bool,
    industrial_complex: bool,
    is_island: bool,
    neighbors: Vec<String>,
}
#[derive(Deserialize)]
struct RawSea {
    id: String,
    name: String,
    neighbors: Vec<String>,
}
#[derive(Deserialize)]
struct RawCanal {
    name: String,
    sea_zones: Vec<String>,
    land_territories: Vec<String>,
}
#[derive(Deserialize)]
struct RawStart {
    location: String,
    owner: String,
    unit: String,
    count: u32,
}
#[derive(Deserialize)]
struct RawMap {
    powers: Vec<RawPower>,
    territories: Vec<RawTerr>,
    sea_zones: Vec<RawSea>,
    starting_units: Vec<RawStart>,
    canals: Vec<RawCanal>,
}

static MAP: OnceLock<MapData> = OnceLock::new();

impl MapData {
    /// The global map, parsed once from the embedded JSON.
    pub fn get() -> &'static MapData {
        MAP.get_or_init(|| MapData::parse(MAP_JSON).expect("embedded map.json is valid"))
    }

    fn parse(json: &str) -> Result<MapData, String> {
        let raw: RawMap = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut by_id = HashMap::new();
        let mut by_name = HashMap::new();
        let mut ids: Vec<(String, String)> = vec![];
        for t in &raw.territories {
            ids.push((t.id.clone(), t.name.clone()));
        }
        let land_count = ids.len();
        for s in &raw.sea_zones {
            ids.push((s.id.clone(), s.name.clone()));
        }
        for (i, (id, name)) in ids.iter().enumerate() {
            by_id.insert(id.clone(), LocationId(i as u16));
            by_name.insert(name.clone(), LocationId(i as u16));
        }
        let look = |s: &str| {
            by_id
                .get(s)
                .copied()
                .ok_or_else(|| format!("unknown id {s}"))
        };
        let mut capitals = [LocationId(0); 5];
        let mut starting_ipcs = [0; 5];
        let mut cap_of: HashMap<LocationId, Power> = HashMap::new();
        for p in &raw.powers {
            let pw = Power::from_id(&p.id).ok_or("bad power")?;
            capitals[pw.index()] = look(&p.capital)?;
            starting_ipcs[pw.index()] = p.starting_ipcs;
            cap_of.insert(look(&p.capital)?, pw);
        }
        let mut locs = vec![];
        for t in &raw.territories {
            let id = look(&t.id)?;
            let mut nb = vec![];
            for n in &t.neighbors {
                nb.push(look(n)?);
            }
            locs.push(Location {
                id: t.id.clone(),
                name: t.name.clone(),
                is_sea: false,
                ipc: t.ipc,
                original_owner: match &t.original_owner {
                    Some(o) => Some(Power::from_id(o).ok_or("bad owner")?),
                    None => None,
                },
                neutral: t.neutral,
                capital_of: if t.is_capital {
                    cap_of.get(&id).copied()
                } else {
                    None
                },
                has_original_complex: t.industrial_complex,
                is_island: t.is_island,
                neighbors: nb,
            });
        }
        for s in &raw.sea_zones {
            let mut nb = vec![];
            for n in &s.neighbors {
                nb.push(look(n)?);
            }
            locs.push(Location {
                id: s.id.clone(),
                name: s.name.clone(),
                is_sea: true,
                ipc: 0,
                original_owner: None,
                neutral: false,
                capital_of: None,
                has_original_complex: false,
                is_island: false,
                neighbors: nb,
            });
        }
        let mut canals = vec![];
        for c in &raw.canals {
            canals.push(Canal {
                name: c.name.clone(),
                sea_zones: [look(&c.sea_zones[0])?, look(&c.sea_zones[1])?],
                land_territories: c
                    .land_territories
                    .iter()
                    .map(|s| look(s))
                    .collect::<Result<_, _>>()?,
            });
        }
        let mut starting = vec![];
        for s in &raw.starting_units {
            starting.push(StartingStack {
                location: look(&s.location)?,
                owner: Power::from_id(&s.owner).ok_or("bad owner")?,
                unit: UnitType::from_id(&s.unit).ok_or("bad unit")?,
                count: s.count,
            });
        }
        Ok(MapData {
            locs,
            by_id,
            by_name,
            land_count,
            canals,
            starting,
            capitals,
            starting_ipcs,
        })
    }

    /// Number of locations (territories first, then sea zones).
    pub fn len(&self) -> usize {
        self.locs.len()
    }
    /// Always false (the map is never empty).
    pub fn is_empty(&self) -> bool {
        self.locs.is_empty()
    }
    /// All location ids.
    pub fn locations(&self) -> impl Iterator<Item = LocationId> {
        (0..self.locs.len() as u16).map(LocationId)
    }
    /// Land territory ids.
    pub fn territories(&self) -> impl Iterator<Item = LocationId> {
        (0..self.land_count as u16).map(LocationId)
    }
    /// Sea zone ids.
    pub fn sea_zones(&self) -> impl Iterator<Item = LocationId> {
        (self.land_count as u16..self.locs.len() as u16).map(LocationId)
    }
    /// Location record.
    pub fn loc(&self, l: LocationId) -> &Location {
        &self.locs[l.idx()]
    }
    /// Lookup by string id.
    pub fn id_of(&self, id: &str) -> Option<LocationId> {
        self.by_id.get(id).copied()
    }
    /// Lookup by display name.
    pub fn id_by_name(&self, name: &str) -> Option<LocationId> {
        self.by_name.get(name).copied()
    }
    /// Adjacent locations.
    pub fn neighbours(&self, l: LocationId) -> &[LocationId] {
        &self.locs[l.idx()].neighbors
    }
    /// Are the two locations adjacent?
    pub fn adjacent(&self, a: LocationId, b: LocationId) -> bool {
        self.locs[a.idx()].neighbors.contains(&b)
    }
    /// True for sea zones.
    pub fn is_sea(&self, l: LocationId) -> bool {
        l.idx() >= self.land_count
    }
    /// IPC value.
    pub fn ipc(&self, l: LocationId) -> u32 {
        self.locs[l.idx()].ipc
    }
    /// Original owner of a territory.
    pub fn original_owner(&self, l: LocationId) -> Option<Power> {
        self.locs[l.idx()].original_owner
    }
    /// If `l` is a capital, whose.
    pub fn capital_of(&self, l: LocationId) -> Option<Power> {
        self.locs[l.idx()].capital_of
    }
    /// Capital of a power.
    pub fn capital(&self, p: Power) -> LocationId {
        self.capitals[p.index()]
    }
    /// Starting income/treasury.
    pub fn starting_ipcs(&self, p: Power) -> u32 {
        self.starting_ipcs[p.index()]
    }
    /// Canals.
    pub fn canals(&self) -> &[Canal] {
        &self.canals
    }
    /// Printed starting stacks.
    pub fn starting_units(&self) -> &[StartingStack] {
        &self.starting
    }
    /// Sea zones adjacent to a land territory.
    pub fn adjacent_seas(&self, l: LocationId) -> impl Iterator<Item = LocationId> + '_ {
        self.neighbours(l)
            .iter()
            .copied()
            .filter(|n| self.is_sea(*n))
    }
    /// Canal (if any) between two adjacent sea zones.
    pub fn canal_between(&self, a: LocationId, b: LocationId) -> Option<&Canal> {
        self.canals.iter().find(|c| {
            (c.sea_zones[0] == a && c.sea_zones[1] == b)
                || (c.sea_zones[0] == b && c.sea_zones[1] == a)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads() {
        let m = MapData::get();
        assert_eq!(m.territories().count(), 70);
        assert_eq!(m.sea_zones().count(), 58);
        let r = m.id_of("russia").unwrap();
        assert_eq!(m.ipc(r), 8);
        assert_eq!(m.capital_of(r), Some(Power::Ussr));
        assert_eq!(
            m.id_by_name("Anglo-Egypt Sudan"),
            m.id_of("anglo_egypt_sudan")
        );
        assert_eq!(m.canals().len(), 2);
        assert!(m.is_sea(m.id_of("sz_baltic").unwrap()));
    }
    #[test]
    fn adjacency_symmetric() {
        let m = MapData::get();
        for a in m.locations() {
            for &b in m.neighbours(a) {
                assert!(m.adjacent(b, a), "{} !~ {}", a.id(), b.id());
            }
        }
    }
    #[test]
    fn serde_ids() {
        let l = LocationId::from_id("sz_baltic").unwrap();
        assert_eq!(serde_json::to_string(&l).unwrap(), "\"sz_baltic\"");
        assert!(serde_json::from_str::<LocationId>("\"nowhere\"").is_err());
    }
}
