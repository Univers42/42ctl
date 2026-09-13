/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   bulk.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Multi-target verbs: `rm`, `project grant rm`, and anything else spelled `TARGET...`.
//!
//! The rule these helpers exist to enforce is that one bad target must not abort the rest.
//! A bulk removal is usually fed by `$(… -q)`, which is exactly where a target goes stale
//! between the listing and the removal; stopping at the first failure would leave the
//! operator not knowing which of the remaining targets were touched. So every target is
//! attempted, each failure is named on stderr as it happens, and the command fails once at
//! the end with a count and the names.
//!
//! Each target stays a separate request on purpose. Batching them into one would move the
//! authorization check off the server's per-target path, and that check is the point.

use crate::ui;

/// The targets to attempt: the order given, with repeats dropped.
///
/// A repeat is never what the operator meant — it comes from a listing that named something
/// twice — and the second attempt on an already-removed target reports a failure that is not
/// one, which is how a clean run ends up with a non-zero exit.
pub fn targets(given: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    given
        .iter()
        .filter(|target| seen.insert((*target).clone()))
        .cloned()
        .collect()
}

/// Name one target's failure on stderr, so the run's progress stays readable while it
/// continues past it.
pub fn failure(target: &str, error: &anyhow::Error) {
    eprintln!("{} {target}: {error:#}", ui::bad("error:"));
}

/// Fail once for the whole run when any target did, naming them.
///
/// The names matter more than the count: after a partial failure the operator's next move is
/// to retry exactly those, and re-running the whole list is how the successful removals get
/// reported as errors the second time.
pub fn report(failed: &[String], attempted: usize) -> anyhow::Result<()> {
    if failed.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "{} of {attempted} failed: {}",
        failed.len(),
        failed.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Order is what the operator sees in the listing they piped in; repeats are dropped so a
    /// duplicated name cannot report a second, phantom failure.
    #[test]
    fn targets_keep_their_order_and_lose_their_repeats() {
        let given: Vec<String> = ["b", "a", "b", "c", "a"]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(targets(&given), vec!["b", "a", "c"]);
        assert!(targets(&[]).is_empty());
    }

    /// A clean run succeeds; a partial one fails and names every target to retry.
    #[test]
    fn a_partial_failure_names_what_to_retry() {
        assert!(report(&[], 3).is_ok());
        let error = report(&["a".to_string(), "c".to_string()], 3)
            .expect_err("two failed")
            .to_string();
        assert!(error.contains("2 of 3"), "counts both sides: {error}");
        assert!(
            error.contains('a') && error.contains('c'),
            "names them: {error}"
        );
    }
}
