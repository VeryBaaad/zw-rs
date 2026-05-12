use std::process::Command;
use chrono::Local;

fn main() {
    let shortcommit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    let count = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(shortcommit.stdout).unwrap().trim().to_string();
    let git_count_str = String::from_utf8(count.stdout).unwrap().trim().to_string();
    let git_count: u64 = git_count_str.parse().unwrap_or(0);
    let build_time = Local::now().format("%Y-%m-%dT%H:%M:%S%.3f");
    let target = std::env::var("TARGET").unwrap();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=VER_CODE={}", git_count + 500);
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
    println!("cargo:rustc-env=BUILD_TARGET={}", target);
    println!("cargo:rerun-if-changed=.git/HEAD");
}