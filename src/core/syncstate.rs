/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   syncstate.rs                                         :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Local per-file sync base — the last-synced `{rev, hash}` for each project file,
//! persisted at `.42ctl/sync.json`. It holds NO secret material: only a blake3 content
//! hash and the vault revision. This is what lets `pull` distinguish "local unchanged
//! since last sync" (fast-forward) from "both sides diverged since the base" (conflict),
//! exactly the way git uses the index as the merge base.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const STATE_FILE: &str = ".42ctl/sync.json";

/// The recorded base for one file: the vault revision last synced here and the blake3
/// hash of that synced content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Base {
    pub rev: u64,
    pub hash: String,
}

/// The project's local sync state: `relative_path` → its last-synced [`Base`].
#[derive(Default, Serialize, Deserialize)]
pub struct SyncState {
    pub bases: BTreeMap<String, Base>,
}

impl SyncState {
    /// Load the state from `<root>/.42ctl/sync.json`, or an empty state when absent or
    /// unparsable (a missing base just means "no merge base yet").
    pub fn load(root: &Path) -> Self {
        std::fs::read(root.join(STATE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Persist the state (pretty JSON), creating `.42ctl/` if needed.
    pub fn save(&self, root: &Path) -> anyhow::Result<()> {
        let path = root.join(STATE_FILE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// Record (or replace) the base for `rel`.
    pub fn set(&mut self, rel: &str, rev: u64, hash: String) {
        self.bases.insert(rel.to_string(), Base { rev, hash });
    }
}

/// The blake3 content hash (hex) used to compare local/base/remote bytes.
pub fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
