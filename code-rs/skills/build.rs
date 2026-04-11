use std::fs;
use std::path::Path;

fn main() {
    let system_skills_dir = Path::new("src/assets/system_skills");
    if !system_skills_dir.exists() {
        return;
    }

    println!("cargo:rerun-if-changed={}", system_skills_dir.display());
    visit_dir(system_skills_dir);
}

fn visit_dir(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            visit_dir(&path);
        }
    }
}

