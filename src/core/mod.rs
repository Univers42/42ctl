/* ************************************************************************** */
/*                                                                            */
/*                                                        :::      ::::::::   */
/*   mod.rs                                             :+:      :+:    :+:   */
/*                                                    +:+ +:+         +:+     */
/*   By: dlesieur <dlesieur@student.42.fr>          +#+  +:+       +#+        */
/*                                                +#+#+#+#+#+   +#+           */
/*   Created: 2026/06/21 00:00:00 by dlesieur          #+#    #+#             */
/*   Updated: 2026/06/21 04:36:59 by dlesieur         ###   ########.fr       */
/*                                                                            */
/* ************************************************************************** */

//! core — the pure, I/O-light use-case logic for path-aware project sync: project
//! discovery + scan, the encrypted manifest, the traversal-defended path model, and
//! byte-exact materialization. The `ops` layer orchestrates these over a gRPC session.

pub mod manifest;
pub mod materialize;
pub mod project;
pub mod projpath;
