use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFile {
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseEntry {
    pub local_sha256: String,
    pub remote_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    BothModified,
    /// Changed locally, deleted remotely.
    LocalModifiedRemoteDeleted,
    /// Deleted locally, changed remotely.
    LocalDeletedRemoteModified,
    /// No common history and contents differ.
    Unrelated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Keep,
    Upload,
    Download,
    DeleteRemote,
    DeleteLocal,
    /// Both sides exist without history: contents must be compared.
    Compare,
    Conflict(ConflictKind),
    ForgetBase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    LocalRootMissing,
    LocalEmptyWithHistory,
    RemoteEmptyWithHistory,
    RootChanged,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub files: BTreeMap<String, Action>,
    pub warnings: Vec<Warning>,
}

impl Plan {
    /// Remote deletions are never allowed when the local side looks wrong.
    pub fn remote_deletions_blocked(&self) -> bool {
        self.warnings.iter().any(|w| {
            matches!(
                w,
                Warning::LocalRootMissing | Warning::LocalEmptyWithHistory | Warning::RootChanged
            )
        })
    }

    /// Local deletions are never allowed when the remote side looks wrong.
    pub fn local_deletions_blocked(&self) -> bool {
        self.warnings
            .iter()
            .any(|w| matches!(w, Warning::RemoteEmptyWithHistory | Warning::RootChanged))
    }

    pub fn count(&self, action: Action) -> usize {
        self.files.values().filter(|a| **a == action).count()
    }

    pub fn conflicts(&self) -> impl Iterator<Item = (&String, ConflictKind)> {
        self.files.iter().filter_map(|(p, a)| match a {
            Action::Conflict(k) => Some((p, *k)),
            _ => None,
        })
    }

    pub fn is_noop(&self) -> bool {
        self.files.values().all(|a| matches!(a, Action::Keep))
    }
}

pub struct Inputs<'a> {
    pub local: &'a BTreeMap<String, LocalFile>,
    pub local_root_exists: bool,
    pub remote: &'a BTreeMap<String, RemoteFile>,
    pub base: &'a BTreeMap<String, BaseEntry>,
    pub root_changed: bool,
}

pub fn plan(inputs: &Inputs<'_>) -> Plan {
    let Inputs {
        local,
        remote,
        base,
        ..
    } = inputs;
    let paths: BTreeSet<&String> = local
        .keys()
        .chain(remote.keys())
        .chain(base.keys())
        .collect();
    let files = paths
        .into_iter()
        .map(|p| {
            (
                p.clone(),
                classify(local.get(p), remote.get(p), base.get(p)),
            )
        })
        .collect();

    let mut warnings = Vec::new();
    if !inputs.local_root_exists {
        warnings.push(Warning::LocalRootMissing);
    } else if local.is_empty() && !base.is_empty() {
        warnings.push(Warning::LocalEmptyWithHistory);
    }
    if remote.is_empty() && !base.is_empty() {
        warnings.push(Warning::RemoteEmptyWithHistory);
    }
    if inputs.root_changed {
        warnings.push(Warning::RootChanged);
    }
    Plan { files, warnings }
}

fn classify(
    local: Option<&LocalFile>,
    remote: Option<&RemoteFile>,
    base: Option<&BaseEntry>,
) -> Action {
    let Some(base) = base else {
        return match (local, remote) {
            (Some(_), None) => Action::Upload,
            (None, Some(_)) => Action::Download,
            (Some(_), Some(_)) => Action::Compare,
            (None, None) => Action::Keep,
        };
    };
    let local_changed = local.map(|l| l.sha256 != base.local_sha256);
    let remote_changed = remote.map(|r| r.hash != base.remote_hash);
    match (local_changed, remote_changed) {
        (Some(false), Some(false)) => Action::Keep,
        (Some(true), Some(false)) => Action::Upload,
        (Some(false), Some(true)) => Action::Download,
        (Some(true), Some(true)) => Action::Conflict(ConflictKind::BothModified),
        (None, Some(false)) => Action::DeleteRemote,
        (None, Some(true)) => Action::Conflict(ConflictKind::LocalDeletedRemoteModified),
        (Some(false), None) => Action::DeleteLocal,
        (Some(true), None) => Action::Conflict(ConflictKind::LocalModifiedRemoteDeleted),
        (None, None) => Action::ForgetBase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(sha: &str) -> LocalFile {
        LocalFile {
            sha256: sha.into(),
            size: 1,
        }
    }
    fn r(hash: &str) -> RemoteFile {
        RemoteFile { hash: hash.into() }
    }
    fn b(sha: &str, hash: &str) -> BaseEntry {
        BaseEntry {
            local_sha256: sha.into(),
            remote_hash: hash.into(),
        }
    }

    #[test]
    fn every_three_way_case() {
        let cases = [
            (Some(l("a")), Some(r("x")), Some(b("a", "x")), Action::Keep),
            (
                Some(l("a2")),
                Some(r("x")),
                Some(b("a", "x")),
                Action::Upload,
            ),
            (
                Some(l("a")),
                Some(r("x2")),
                Some(b("a", "x")),
                Action::Download,
            ),
            (
                Some(l("a2")),
                Some(r("x2")),
                Some(b("a", "x")),
                Action::Conflict(ConflictKind::BothModified),
            ),
            (None, Some(r("x")), Some(b("a", "x")), Action::DeleteRemote),
            (
                None,
                Some(r("x2")),
                Some(b("a", "x")),
                Action::Conflict(ConflictKind::LocalDeletedRemoteModified),
            ),
            (Some(l("a")), None, Some(b("a", "x")), Action::DeleteLocal),
            (
                Some(l("a2")),
                None,
                Some(b("a", "x")),
                Action::Conflict(ConflictKind::LocalModifiedRemoteDeleted),
            ),
            (None, None, Some(b("a", "x")), Action::ForgetBase),
            (Some(l("a")), None, None, Action::Upload),
            (None, Some(r("x")), None, Action::Download),
            (Some(l("a")), Some(r("x")), None, Action::Compare),
        ];
        for (local, remote, base, expected) in cases {
            assert_eq!(
                classify(local.as_ref(), remote.as_ref(), base.as_ref()),
                expected
            );
        }
    }

    fn run(
        local: &[(&str, &str)],
        root_exists: bool,
        remote: &[(&str, &str)],
        base: &[(&str, &str, &str)],
    ) -> Plan {
        let local = local.iter().map(|(p, s)| (p.to_string(), l(s))).collect();
        let remote = remote.iter().map(|(p, h)| (p.to_string(), r(h))).collect();
        let base = base
            .iter()
            .map(|(p, s, h)| (p.to_string(), b(s, h)))
            .collect();
        plan(&Inputs {
            local: &local,
            local_root_exists: root_exists,
            remote: &remote,
            base: &base,
            root_changed: false,
        })
    }

    #[test]
    fn missing_local_root_blocks_remote_deletions() {
        let p = run(&[], false, &[("s1", "x")], &[("s1", "a", "x")]);
        assert_eq!(p.files["s1"], Action::DeleteRemote);
        assert!(p.remote_deletions_blocked());
        assert!(!p.local_deletions_blocked());
    }

    #[test]
    fn empty_local_dir_with_history_blocks_remote_deletions() {
        let p = run(
            &[],
            true,
            &[("s1", "x"), ("s2", "y")],
            &[("s1", "a", "x"), ("s2", "b", "y")],
        );
        assert_eq!(p.warnings, vec![Warning::LocalEmptyWithHistory]);
        assert!(p.remote_deletions_blocked());
    }

    #[test]
    fn empty_remote_with_history_blocks_local_deletions() {
        let p = run(&[("s1", "a")], true, &[], &[("s1", "a", "x")]);
        assert_eq!(p.files["s1"], Action::DeleteLocal);
        assert!(p.local_deletions_blocked());
    }

    #[test]
    fn single_local_deletion_is_not_suspicious() {
        let p = run(
            &[("s1", "a")],
            true,
            &[("s1", "x"), ("s2", "y")],
            &[("s1", "a", "x"), ("s2", "b", "y")],
        );
        assert_eq!(p.files["s2"], Action::DeleteRemote);
        assert!(!p.remote_deletions_blocked());
    }

    #[test]
    fn fresh_machine_downloads_everything() {
        let p = run(&[], false, &[("s1", "x"), ("s2", "y")], &[]);
        assert_eq!(p.count(Action::Download), 2);
        assert!(p.warnings.contains(&Warning::LocalRootMissing));
    }
}
