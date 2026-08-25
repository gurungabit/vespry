fn main() {
    // Embed the commit the binary was built from so the About tab can show
    // exactly what's running — version strings alone can't distinguish a
    // release build from a local one built past the tag.
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=VESPRY_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    // models.rs bakes this in via option_env!; without this line a changed
    // endpoint wouldn't trigger a rebuild and the stale one would ship.
    println!("cargo:rerun-if-env-changed=VESPRY_HF_ENDPOINT");

    tauri_build::build()
}
