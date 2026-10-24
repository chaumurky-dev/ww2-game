//! Copies shared source-of-truth files into `assets/` so trunk serves them:
//! `../engine/data/map.json` -> `assets/map.json`, `../../assets/icons/{sprite.svg,units,powers}`
//! -> `assets/icons/`. Files are only rewritten when their content changed (keeps `trunk serve`
//! from looping). Missing sources are skipped, so builds from a pruned tree still work.
use std::fs;
use std::path::Path;

fn sync_file(src: &Path, dst: &Path) {
    println!("cargo:rerun-if-changed={}", src.display());
    let Ok(new) = fs::read(src) else { return };
    if fs::read(dst).map(|old| old == new).unwrap_or(false) {
        return;
    }
    if let Some(dir) = dst.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Err(e) = fs::write(dst, new) {
        println!("cargo:warning=could not write {}: {e}", dst.display());
    }
}

fn sync_dir(src: &Path, dst: &Path) {
    println!("cargo:rerun-if-changed={}", src.display());
    let Ok(rd) = fs::read_dir(src) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_file() {
            sync_file(&p, &dst.join(e.file_name()));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    sync_file(
        Path::new("../engine/data/map.json"),
        Path::new("assets/map.json"),
    );
    let icons = Path::new("../../assets/icons");
    let out = Path::new("assets/icons");
    sync_file(&icons.join("sprite.svg"), &out.join("sprite.svg"));
    sync_dir(&icons.join("units"), &out.join("units"));
    sync_dir(&icons.join("powers"), &out.join("powers"));
}
