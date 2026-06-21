/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   notes.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl note` — project notes (kind=Note) over the project's encrypted manifest. Opens
//! a signed session like the vault verbs (reusing `cmd::sync::open_session`) and delegates
//! to the `ops::notes` use-cases. All sealing/decryption is local; only opaque ciphertext
//! crosses the wire.

use crate::cli::Note;

/// Dispatch a `note` subcommand for `profile`.
pub async fn run(cmd: &Note, profile: &str) -> anyhow::Result<()> {
    let mut session = super::sync::open_session(profile).await?;
    match cmd {
        Note::Add { path, project, file } => {
            let bytes = super::vault::read_input(file.as_deref())?;
            session.cmd_note_add(project.as_deref(), path, &bytes).await
        }
        Note::Get { path, project } => session.cmd_note_get(project.as_deref(), path).await,
        Note::Ls { project } => session.cmd_note_ls(project.as_deref()).await,
        Note::Rm { path, project } => session.cmd_note_rm(project.as_deref(), path).await,
    }
}
