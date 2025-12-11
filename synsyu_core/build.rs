use std::env;
use std::process::Command;

fn main() {
    emit_env(
        "SYN_SYU_BUILD_SOURCE",
        env_value("SYN_SYU_BUILD_SOURCE").unwrap_or_else(|| "git".into()),
    );
    emit_env(
        "SYN_SYU_GIT_COMMIT",
        env_value("SYN_SYU_GIT_COMMIT")
            .or_else(git_commit)
            .unwrap_or_else(|| "unknown".into()),
    );
    emit_env(
        "SYN_SYU_RUSTC_VERSION",
        env_value("SYN_SYU_RUSTC_VERSION")
            .or_else(rustc_version)
            .unwrap_or_else(|| "unknown".into()),
    );
    emit_env(
        "SYN_SYU_BUILD_TIME",
        env_value("SYN_SYU_BUILD_TIME").unwrap_or_else(|| "".into()),
    );
    emit_env(
        "SYN_SYU_FEATURES",
        env_value("SYN_SYU_FEATURES").unwrap_or_else(|| "".into()),
    );

    for key in [
        "SYN_SYU_AUR_COMMIT",
        "SYN_SYU_PKGVER",
        "SYN_SYU_PKGREL",
        "SYN_SYU_EPOCH",
    ] {
        emit_env(key, env_value(key).unwrap_or_else(|| "".into()));
    }

    emit_env(
        "SYN_SYU_BUILD_PROFILE",
        env::var("PROFILE").unwrap_or_else(|_| "unknown".into()),
    );
    emit_env(
        "SYN_SYU_TARGET",
        env::var("TARGET").unwrap_or_else(|_| "unknown".into()),
    );
}

fn emit_env(key: &str, value: String) {
    println!("cargo:rustc-env={}={}", key, value);
}

fn env_value(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn git_commit() -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn rustc_version() -> Option<String> {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
