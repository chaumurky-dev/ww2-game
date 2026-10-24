//! Map geometry (`assets/map_geometry.json`, or the placeholder) loaded at runtime.
//!
//! Tolerant schema: `{ "viewBox": [x,y,w,h] | "x y w h", "locations": [ { id, name?, kind:
//! "land"|"sea", path, anchor?: [x,y], label?: [x,y], ipc? } ] }` (`territories`/`regions` accepted
//! as aliases for `locations`, `d` for `path`, `type` for `kind`).
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LocKind {
    #[default]
    Land,
    Sea,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Location {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, alias = "type")]
    pub kind: LocKind,
    #[serde(alias = "d")]
    pub path: String,
    #[serde(default)]
    pub anchor: Option<(f64, f64)>,
    #[serde(default)]
    pub label: Option<(f64, f64)>,
    #[serde(default)]
    pub ipc: Option<u32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawViewBox {
    Arr([f64; 4]),
    Str(String),
}

#[derive(Deserialize)]
struct Raw {
    #[serde(rename = "viewBox", alias = "view_box")]
    view_box: RawViewBox,
    #[serde(alias = "territories", alias = "regions")]
    locations: Vec<Location>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub view_box: [f64; 4],
    pub locations: Vec<Location>,
}

/// Rough centroid from the numbers in an SVG path (good enough as an anchor fallback).
fn rough_center(path: &str) -> (f64, f64) {
    let nums: Vec<f64> = path
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter_map(|t| t.parse().ok())
        .collect();
    let pairs: Vec<&[f64]> = nums.chunks(2).filter(|c| c.len() == 2).collect();
    if pairs.is_empty() {
        return (0.0, 0.0);
    }
    let n = pairs.len() as f64;
    (pairs.iter().map(|p| p[0]).sum::<f64>() / n, pairs.iter().map(|p| p[1]).sum::<f64>() / n)
}

impl Geometry {
    pub fn parse(json: &str) -> Result<Geometry, String> {
        let raw: Raw = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let view_box = match raw.view_box {
            RawViewBox::Arr(a) => a,
            RawViewBox::Str(s) => {
                let v: Vec<f64> = s.split_whitespace().filter_map(|t| t.parse().ok()).collect();
                <[f64; 4]>::try_from(v).map_err(|_| "bad viewBox".to_string())?
            }
        };
        let mut locations = raw.locations;
        for l in &mut locations {
            if l.name.is_empty() {
                l.name = l.id.clone();
            }
            let c = rough_center(&l.path);
            l.anchor.get_or_insert(c);
            l.label.get_or_insert(c);
        }
        Ok(Geometry { view_box, locations })
    }

    pub fn find(&self, id: &str) -> Option<&Location> {
        self.locations.iter().find(|l| l.id == id)
    }
}

async fn fetch_text(url: &str) -> Option<String> {
    let resp = Request::get(url).send().await.ok()?;
    // Dev servers may answer 200 with index.html for unknown paths, so also try to parse.
    resp.ok().then_some(())?;
    resp.text().await.ok()
}

/// Load the real geometry if present, else the bundled placeholder.
pub async fn load() -> Result<Geometry, String> {
    for url in ["/assets/map_geometry.json", "/assets/map_geometry.placeholder.json"] {
        if let Some(text) = fetch_text(url).await {
            if let Ok(g) = Geometry::parse(&text) {
                return Ok(g);
            }
        }
    }
    Err("map geometry could not be loaded".into())
}
