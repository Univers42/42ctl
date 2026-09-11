/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_ls.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault ls-env` — an environment's file inventory from its manifests, fetching no file.
//!
//! The shared manifest names every file the environment holds for its members; the
//! caller's private manifest adds theirs. Size, mode and labels were recorded at push, so the
//! listing costs two small reads however large the tree. Rendered through `ui::render`, so
//! `--format '{{.Path}} {{.Labels.app}}'`, `--format json` and `--filter label=app=web` all
//! apply — this is the "database" view of what a project stores.

use crate::adapters::api::Session;
use crate::adapters::scope as crypto;
use crate::cli::Output;
use crate::cmd::scope::Ctx;
use crate::cmd::scope_private::{self, Pick};
use crate::cmd::scope_recover::recover_scope_secret;
use crate::ui;
use vault42_core::Kind;

/// Render `Path Size Mode Kind Private Labels` for every file the caller can restore.
pub async fn ls_env(session: &mut Session, ctx: &Ctx, out: &Output) -> anyhow::Result<()> {
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let owner = hex::encode(scope_id);
    let secret =
        recover_scope_secret(session, scope_id, ctx.epoch(), ctx.scope_pubkey.as_deref()).await?;
    let (shared, mine) = scope_private::both_manifests(session, ctx, (&owner, &secret)).await?;
    let merged = scope_private::merge(&shared, mine.as_ref());
    scope_private::report_shadows(&merged.shadowed);
    let rows = merged.picks.iter().map(row).collect();
    ui::render(
        &["Path", "Size", "Mode", "Kind", "Private", "Labels"],
        rows,
        out.format.as_deref(),
        &out.filter,
    )
}

/// One listing row; `Labels` stays an object so `{{.Labels.k}}` and `label=k=v` can reach in.
fn row(pick: &Pick) -> serde_json::Value {
    let entry = pick.entry;
    serde_json::json!({
        "Path": entry.relative_path,
        "Size": entry.size,
        "Mode": format!("{:04o}", entry.mode),
        "Kind": if entry.kind == Kind::Note as u8 { "note" } else { "file" },
        "Private": pick.private,
        "Labels": entry.labels,
    })
}
