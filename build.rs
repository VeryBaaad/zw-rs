use std::process::Command;
use chrono::Local;

fn main() {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
    let build_time = Local::now().format("%Y-%m-%dT%H:%M:%S%.3f");
    let target = std::env::var("TARGET").unwrap();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
    println!("cargo:rustc-env=BUILD_TARGET={}", target);
    println!("cargo:rerun-if-changed=.git/HEAD");
}