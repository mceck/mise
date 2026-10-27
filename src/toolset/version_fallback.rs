//! Fallback versions: an installed version used in place of a requested version
//! that is not installed, for the tools listed in the `ignore_patch`,
//! `ignore_minor` and `ignore_version` settings.
//!
//! Version strings stay opaque. A request is only shortened to its first dot-separated
//! parts, and the installed version that matches it is chosen by the backend.

use std::collections::BTreeSet;

use crate::args::BackendArg;
use crate::backend::{ABackend, unalias_backend};
use crate::config::Settings;

/// How much of a requested version may be disregarded when choosing a fallback version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IgnoreLevel {
    /// Keep the first two dot-separated parts (`20.11.1` -> `20.11`).
    Patch,
    /// Keep the first dot-separated part (`20.11.1` -> `20`).
    Minor,
    /// Disregard the requested version entirely.
    Version,
}

impl IgnoreLevel {
    /// The widest level the settings give this tool, if any.
    pub(super) fn for_tool(ba: &BackendArg) -> Option<Self> {
        let settings = Settings::get();
        Self::from_lists(
            ba,
            &settings.ignore_patch,
            &settings.ignore_minor,
            &settings.ignore_version,
        )
    }

    fn from_lists(
        ba: &BackendArg,
        patch: &BTreeSet<String>,
        minor: &BTreeSet<String>,
        version: &BTreeSet<String>,
    ) -> Option<Self> {
        let names = [unalias_backend(&ba.short).into_owned(), ba.full()];
        let lists = |list: &BTreeSet<String>| {
            list.iter()
                .any(|entry| entry == "*" || names.contains(&unalias_backend(entry).into_owned()))
        };
        [
            (Self::Version, version),
            (Self::Minor, minor),
            (Self::Patch, patch),
        ]
        .into_iter()
        .find(|(_, list)| lists(list))
        .map(|(level, _)| level)
    }

    /// The prefix an installed version must match to stand in for `version`, or
    /// `None` when this level keeps all of it (or, for `Version`, none of it).
    fn prefix(self, version: &str) -> Option<String> {
        let keep = match self {
            Self::Patch => 2,
            Self::Minor => 1,
            Self::Version => return None,
        };
        let parts = version.split('.').collect::<Vec<_>>();
        (parts.len() > keep).then(|| parts[..keep].join("."))
    }

    /// The newest installed version of `backend`'s tool that may stand in for `version`.
    pub(super) fn find(self, backend: &ABackend, version: &str) -> Option<String> {
        match self {
            Self::Version => backend.latest_installed_version(None).ok().flatten(),
            _ => backend
                .list_installed_versions_matching(&self.prefix(version)?)
                .pop(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IgnoreLevel::*;
    use super::*;

    fn set(entries: &[&str]) -> BTreeSet<String> {
        entries.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn prefix_keeps_leading_dot_separated_parts() {
        assert_eq!(Patch.prefix("20.11.1").as_deref(), Some("20.11"));
        assert_eq!(Minor.prefix("20.11.1").as_deref(), Some("20"));
        assert_eq!(Patch.prefix("v1.2.3").as_deref(), Some("v1.2"));
        assert_eq!(Patch.prefix("3.12.0a1").as_deref(), Some("3.12"));
        assert_eq!(Minor.prefix("2024.01.15").as_deref(), Some("2024"));
        assert_eq!(
            Patch.prefix("temurin-21.0.1+12").as_deref(),
            Some("temurin-21.0")
        );
        assert_eq!(Minor.prefix("1.22rc1").as_deref(), Some("1"));
    }

    #[test]
    fn prefix_is_none_when_nothing_is_dropped() {
        assert_eq!(Patch.prefix("20.11"), None);
        assert_eq!(Patch.prefix("1.22rc1"), None);
        assert_eq!(Minor.prefix("20"), None);
        assert_eq!(Minor.prefix("latest"), None);
        assert_eq!(Version.prefix("20.11.1"), None);
    }

    #[test]
    fn widest_listed_level_wins() {
        let node = BackendArg::from("node".to_string());
        let empty = set(&[]);
        assert_eq!(IgnoreLevel::from_lists(&node, &empty, &empty, &empty), None);
        assert_eq!(
            IgnoreLevel::from_lists(&node, &set(&["*"]), &empty, &empty),
            Some(Patch)
        );
        assert_eq!(
            IgnoreLevel::from_lists(&node, &set(&["*"]), &set(&["node"]), &empty),
            Some(Minor)
        );
        assert_eq!(
            IgnoreLevel::from_lists(&node, &set(&["node"]), &set(&["*"]), &set(&["*"])),
            Some(Version)
        );
        assert_eq!(
            IgnoreLevel::from_lists(&node, &set(&["go", "rust"]), &empty, &empty),
            None
        );
    }

    #[test]
    fn tools_match_by_short_name_alias_or_backend() {
        let node = BackendArg::from("node".to_string());
        let go = BackendArg::from("go".to_string());
        let empty = set(&[]);
        let asdf_names = set(&["nodejs", "golang"]);
        assert_eq!(
            IgnoreLevel::from_lists(&node, &asdf_names, &empty, &empty),
            Some(Patch)
        );
        assert_eq!(
            IgnoreLevel::from_lists(&go, &asdf_names, &empty, &empty),
            Some(Patch)
        );
        assert_eq!(
            IgnoreLevel::from_lists(&node, &set(&["core:node"]), &empty, &empty),
            Some(Patch)
        );
        let full = node.full();
        assert_eq!(
            IgnoreLevel::from_lists(&node, &set(&[full.as_str()]), &empty, &empty),
            Some(Patch)
        );
    }
}
