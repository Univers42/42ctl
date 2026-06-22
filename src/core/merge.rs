/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   merge.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The `pull` reconciliation: decide what to do with each file from (local-on-disk,
//! remote-in-vault, last-synced base) and, when both sides diverged, produce git-style
//! conflict markers. Dependency-free: a 2-way line merge that keeps the common
//! prefix/suffix and wraps only the divergent middle. ponytail: one conflict block per
//! divergent region (not git's minimal hunks) — correct + clear for small env files.

use crate::core::syncstate::{hash, Base};

/// What `pull` should do with one file.
pub enum Action {
    /// No local file — write the remote bytes and record the base.
    Create(Vec<u8>),
    /// Local already equals remote — nothing to write.
    InSync,
    /// Local is unchanged since the base — take remote (the "older → override" case).
    FastForward(Vec<u8>),
    /// Remote is unchanged since the base but local changed — keep local (unpushed).
    KeepLocal,
    /// Both sides diverged — these bytes carry git-style conflict markers.
    Conflict(Vec<u8>),
    /// Non-text content that diverged and can't be line-merged; carries the remote bytes
    /// (the caller writes them to a `.remote` sidecar and keeps local).
    Binary(Vec<u8>),
}

/// Classify one file. `base` is the last-synced record (None = never synced here).
/// `force` takes remote unconditionally (overwrite local, no conflict markers).
pub fn decide(
    force: bool,
    local: Option<&[u8]>,
    remote: &[u8],
    remote_rev: u64,
    base: Option<&Base>,
) -> Action {
    let Some(local) = local else {
        return Action::Create(remote.to_vec());
    };
    if local == remote {
        return Action::InSync;
    }
    if force {
        return Action::FastForward(remote.to_vec());
    }
    if let Some(base) = base {
        if hash(local) == base.hash {
            return Action::FastForward(remote.to_vec());
        }
        if remote_rev == base.rev {
            return Action::KeepLocal;
        }
    }
    match line_merge(local, remote) {
        Merged::Clean => Action::InSync,
        Merged::Conflict(bytes) => Action::Conflict(bytes),
        Merged::Binary => Action::Binary(remote.to_vec()),
    }
}

/// The outcome of a 2-way line merge.
enum Merged {
    Clean,
    Conflict(Vec<u8>),
    Binary,
}

/// Merge `local` against `remote`: identical → Clean; non-UTF-8 → Binary; otherwise a
/// conflict keeping the common prefix/suffix and wrapping the divergent middle in
/// `<<<<<<< local / ======= / >>>>>>> remote` markers.
fn line_merge(local: &[u8], remote: &[u8]) -> Merged {
    if local == remote {
        return Merged::Clean;
    }
    let (Ok(left), Ok(right)) = (std::str::from_utf8(local), std::str::from_utf8(remote)) else {
        return Merged::Binary;
    };
    let lhs: Vec<&str> = left.lines().collect();
    let rhs: Vec<&str> = right.lines().collect();
    let pre = lhs.iter().zip(&rhs).take_while(|(a, b)| a == b).count();
    let suf = common_suffix(&lhs[pre..], &rhs[pre..]);
    Merged::Conflict(render(&lhs, &rhs, pre, suf))
}

/// Count trailing lines equal in both remainders (never overlapping the prefix).
fn common_suffix(a: &[&str], b: &[&str]) -> usize {
    let max = a.len().min(b.len());
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take(max)
        .take_while(|(x, y)| x == y)
        .count()
}

/// Assemble `prefix + <local middle | remote middle> + suffix` with conflict markers,
/// every line `\n`-terminated.
fn render(lhs: &[&str], rhs: &[&str], pre: usize, suf: usize) -> Vec<u8> {
    let mut out = String::new();
    let emit = |out: &mut String, lines: &[&str]| {
        lines.iter().for_each(|l| {
            out.push_str(l);
            out.push('\n');
        })
    };
    emit(&mut out, &lhs[..pre]);
    out.push_str("<<<<<<< local (on disk)\n");
    emit(&mut out, &lhs[pre..lhs.len() - suf]);
    out.push_str("=======\n");
    emit(&mut out, &rhs[pre..rhs.len() - suf]);
    out.push_str(">>>>>>> remote (vault)\n");
    emit(&mut out, &lhs[lhs.len() - suf..]);
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(rev: u64, content: &[u8]) -> Base {
        Base {
            rev,
            hash: hash(content),
        }
    }

    #[test]
    fn create_when_no_local() {
        assert!(matches!(
            decide(false, None, b"x", 1, None),
            Action::Create(_)
        ));
    }

    #[test]
    fn insync_when_equal() {
        assert!(matches!(
            decide(false, Some(b"x"), b"x", 1, None),
            Action::InSync
        ));
    }

    #[test]
    fn fast_forward_when_local_unchanged_since_base() {
        let b = base(1, b"v1");
        assert!(matches!(
            decide(false, Some(b"v1"), b"v2", 2, Some(&b)),
            Action::FastForward(_)
        ));
    }

    #[test]
    fn keep_local_when_remote_unchanged_since_base() {
        let b = base(2, b"v1");
        assert!(matches!(
            decide(false, Some(b"local-edit"), b"v1", 2, Some(&b)),
            Action::KeepLocal
        ));
    }

    #[test]
    fn conflict_keeps_common_context_outside_markers() {
        let b = base(1, b"K=base\nSHARED=c\n");
        let action = decide(
            false,
            Some(b"K=mine\nSHARED=c\n"),
            b"K=theirs\nSHARED=c\n",
            2,
            Some(&b),
        );
        let Action::Conflict(bytes) = action else {
            panic!("expected conflict");
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("<<<<<<< local") && text.contains(">>>>>>> remote"));
        assert!(text.contains("K=mine") && text.contains("K=theirs"));
        assert!(text.ends_with("SHARED=c\n"));
    }

    #[test]
    fn force_takes_remote_even_when_diverged() {
        let b = base(1, b"v1");
        assert!(matches!(
            decide(true, Some(b"local-edit"), b"v2", 2, Some(&b)),
            Action::FastForward(_)
        ));
    }
}
