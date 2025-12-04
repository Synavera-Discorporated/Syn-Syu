/// Build-time provenance baked in by build.rs
pub struct BuildInfo {
    pub source: &'static str,
    pub git_commit: &'static str,
    pub aur_commit: &'static str,
    pub aur_pkgver: &'static str,
    pub aur_pkgrel: &'static str,
    pub aur_epoch: &'static str,
    pub build_time: &'static str,
    pub features: &'static str,
    pub version: &'static str,
    pub rustc_version: &'static str,
    pub build_profile: &'static str,
    pub target: &'static str,
}

pub const BUILD_INFO: BuildInfo = BuildInfo {
    source: env!("SYN_SYU_BUILD_SOURCE"),
    git_commit: env!("SYN_SYU_GIT_COMMIT"),
    aur_commit: env!("SYN_SYU_AUR_COMMIT"),
    aur_pkgver: env!("SYN_SYU_PKGVER"),
    aur_pkgrel: env!("SYN_SYU_PKGREL"),
    aur_epoch: env!("SYN_SYU_EPOCH"),
    build_time: env!("SYN_SYU_BUILD_TIME"),
    features: env!("SYN_SYU_FEATURES"),
    version: env!("CARGO_PKG_VERSION"),
    rustc_version: env!("SYN_SYU_RUSTC_VERSION"),
    build_profile: env!("SYN_SYU_BUILD_PROFILE"),
    target: env!("SYN_SYU_TARGET"),
};

#[cfg(test)]
mod tests {
    use super::BUILD_INFO;

    #[test]
    fn build_info_consistency() {
        assert_eq!(BUILD_INFO.version, env!("CARGO_PKG_VERSION"));
        assert!(!BUILD_INFO.source.is_empty(), "build source should be set");
        assert!(
            !BUILD_INFO.git_commit.is_empty(),
            "git commit should be set or 'unknown'"
        );
        assert!(
            !BUILD_INFO.rustc_version.is_empty(),
            "rustc version should be set"
        );
        assert!(
            !BUILD_INFO.build_profile.is_empty(),
            "build profile should be set"
        );
        assert!(!BUILD_INFO.target.is_empty(), "target should be set");
    }
}
