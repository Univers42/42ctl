/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   env.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Environment-scoped RBAC calls: create and list a project's environments (the key-bearing
//! scope grants can target). Both authenticate with the grobase session JWT.

use crate::adapters::rbac::{self, Environment};
use serde_json::json;

/// Create environment `name` under `project` → the created `Environment`
/// (`POST /v1/projects/{project}/environments`).
pub async fn create(
    grobase: &str,
    token: &str,
    project: &str,
    name: &str,
) -> anyhow::Result<Environment> {
    let path = format!("/v1/projects/{project}/environments");
    let body = json!({ "name": name });
    rbac::post_json(grobase, token, &path, &body).await
}

/// List `project`'s environments (`GET /v1/projects/{project}/environments`).
pub async fn list(grobase: &str, token: &str, project: &str) -> anyhow::Result<Vec<Environment>> {
    let path = format!("/v1/projects/{project}/environments");
    rbac::get_json(grobase, token, &path).await
}
