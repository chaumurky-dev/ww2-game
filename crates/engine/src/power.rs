//! Powers, teams and per-power containers.
use crate::map::{LocationId, MapData};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::ops::{Index, IndexMut};

/// The five great powers, in turn order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    /// Soviet Union (moves first).
    Ussr,
    /// Germany.
    Germany,
    /// United Kingdom.
    Uk,
    /// Japan.
    Japan,
    /// United States.
    Usa,
}

/// The two sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Team {
    /// USSR, UK, USA.
    Allies,
    /// Germany, Japan.
    Axis,
}

impl Team {
    /// The opposing team.
    pub fn other(self) -> Team {
        match self {
            Team::Allies => Team::Axis,
            Team::Axis => Team::Allies,
        }
    }
    /// Members in turn order.
    pub fn powers(self) -> &'static [Power] {
        match self {
            Team::Allies => &[Power::Ussr, Power::Uk, Power::Usa],
            Team::Axis => &[Power::Germany, Power::Japan],
        }
    }
}

impl Power {
    /// All powers in turn order.
    pub const ALL: [Power; 5] = [
        Power::Ussr,
        Power::Germany,
        Power::Uk,
        Power::Japan,
        Power::Usa,
    ];

    /// Index 0..5 in turn order.
    pub fn index(self) -> usize {
        self as usize
    }
    /// Power for an index (mod 5).
    pub fn from_index(i: usize) -> Power {
        Power::ALL[i % 5]
    }
    /// Next power in turn order (wraps).
    pub fn next(self) -> Power {
        Power::from_index(self.index() + 1)
    }
    /// Team of this power.
    pub fn team(self) -> Team {
        match self {
            Power::Germany | Power::Japan => Team::Axis,
            _ => Team::Allies,
        }
    }
    /// True if `other` is on the same team (a power is its own ally).
    pub fn is_ally(self, other: Power) -> bool {
        self.team() == other.team()
    }
    /// Snake-case id used in JSON and map data.
    pub fn id(self) -> &'static str {
        match self {
            Power::Ussr => "ussr",
            Power::Germany => "germany",
            Power::Uk => "uk",
            Power::Japan => "japan",
            Power::Usa => "usa",
        }
    }
    /// Parse a snake-case id.
    pub fn from_id(s: &str) -> Option<Power> {
        Power::ALL.into_iter().find(|p| p.id() == s)
    }
    /// Snake-case id (same as [`Power::id`]).
    pub fn as_str(self) -> &'static str {
        self.id()
    }
    /// Parse a snake-case id (same as [`Power::from_id`]).
    pub fn parse(s: &str) -> Option<Power> {
        Power::from_id(s)
    }
    /// Display name.
    pub fn name(self) -> &'static str {
        match self {
            Power::Ussr => "Soviet Union",
            Power::Germany => "Germany",
            Power::Uk => "United Kingdom",
            Power::Japan => "Japan",
            Power::Usa => "United States",
        }
    }
    /// Capital territory.
    pub fn capital(self) -> LocationId {
        MapData::get().capital(self)
    }
}

/// A value for each power. Serialises as an object keyed by power id.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PerPower<T>(pub [T; 5]);

impl<T> Index<Power> for PerPower<T> {
    type Output = T;
    fn index(&self, p: Power) -> &T {
        &self.0[p.index()]
    }
}
impl<T> IndexMut<Power> for PerPower<T> {
    fn index_mut(&mut self, p: Power) -> &mut T {
        &mut self.0[p.index()]
    }
}

impl<T: Serialize> Serialize for PerPower<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(5))?;
        for p in Power::ALL {
            m.serialize_entry(p.id(), &self[p])?;
        }
        m.end()
    }
}

impl<'de, T: Deserialize<'de> + Default> Deserialize<'de> for PerPower<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m: BTreeMap<Power, T> = BTreeMap::deserialize(d)?;
        let mut out = PerPower::<T>::default();
        for (p, v) in m {
            out[p] = v;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn order_and_teams() {
        assert_eq!(Power::Usa.next(), Power::Ussr);
        assert_eq!(Power::Germany.team(), Team::Axis);
        assert!(Power::Uk.is_ally(Power::Usa));
        assert!(!Power::Uk.is_ally(Power::Japan));
        assert_eq!(Power::from_id("japan"), Some(Power::Japan));
    }
    #[test]
    fn per_power_roundtrip() {
        let mut pp = PerPower::<u32>::default();
        pp[Power::Uk] = 7;
        let j = serde_json::to_string(&pp).unwrap();
        assert!(j.contains("\"uk\":7"));
        let back: PerPower<u32> = serde_json::from_str(&j).unwrap();
        assert_eq!(back, pp);
    }
}
