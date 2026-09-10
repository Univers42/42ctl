/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   project.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Project-scoped RBAC calls: create and list an org's projects. A project is the parent
//! every environment, group and grant hangs off, so without one `env create`,
//! `team grant-project` and the whole scope-key suite answer 404. Both calls authenticate
//! with the grobase session JWT and require the caller to be an org admin.

use crate::adapters::rbac::{self, Project};
use serde_json::json;

/// Create project `slug` under `org` → the created `Project`
/// (`POST /v1/orgs/{org}/projects`).
pub async fn create(
    grobase: &str,
    token: &str,
    org: &str,
    slug: &str,
    name: &str,
) -> anyhow::Result<Project> {
    let path = format!("/v1/orgs/{org}/projects");
    let body = json!({ "slug": slug, "name": name });
    rbac::post_json(grobase, token, &path, &body).await
}

/// List `org`'s projects (`GET /v1/orgs/{org}/projects`).
pub async fn list(grobase: &str, token: &str, org: &str) -> anyhow::Result<Vec<Project>> {
    let path = format!("/v1/orgs/{org}/projects");
    rbac::get_json(grobase, token, &path).await
}

/// Resolve a project reference (slug or UUID) to its UUID within `org`.
///
/// The scope verbs need the canonical UUID rather than whatever the operator typed, and that
/// is cryptographic rather than cosmetic: `adapters::scope::scope_id` derives
/// `blake3(project_uuid_bytes ‖ env_name)[..16]`, so every member must feed it the same bytes
/// or they compute different scope ids and read nothing the others wrote. A slug cannot stand
/// in — it is renameable, and renaming one would silently move the scope out from under the
/// secrets already sealed to it. Resolving here rather than loosening the derivation is what
/// lets `--project inception` mean the same thing as `env create --project inception` while
/// the bytes stay canonical.
///
/// A value that already parses as a UUID is returned untouched, so this costs no request on
/// the path automation takes.
pub async fn resolve_id(
    grobase: &str,
    token: &str,
    org: &str,
    reference: &str,
) -> anyhow::Result<String> {
    if uuid::Uuid::parse_str(reference).is_ok() {
        return Ok(reference.to_string());
    }
    list(grobase, token, org)
        .await?
        .into_iter()
        .find(|p| p.slug == reference)
        .map(|p| p.id)
        .ok_or_else(|| anyhow::anyhow!("no project '{reference}' in org '{org}'"))
}
