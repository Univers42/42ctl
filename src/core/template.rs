/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   template.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Row shaping for every listing verb: a `{{.Field}}` template and a `key=value` filter over
//! rows that are plain JSON objects. Deliberately small — the three forms below cover what
//! `docker --format` is used for in practice, and a template engine would be a dependency
//! carried for the sake of features nobody asked for.
//!
//! Forms: `{{.Key}}`, `{{.Labels.app}}` (dotted descent), `{{json .}}` (the row). An unknown
//! key renders as nothing rather than failing, so a template written for one verb can be
//! pointed at another. `--filter k=v` is equality on the stringified field; `label=k=v`
//! descends into `Labels`. Filters are ANDed.

use serde_json::Value;

/// Render `template` against one `row`, replacing every `{{ … }}` with its expansion.
pub fn render(template: &str, row: &Value) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        out.push_str(&expand(after[..end].trim(), row));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Expand one template expression: `json .` is the row, `.a.b` is a field, else nothing.
fn expand(expr: &str, row: &Value) -> String {
    if expr == "json ." {
        return row.to_string();
    }
    expr.strip_prefix('.')
        .and_then(|path| lookup(row, path))
        .map(display)
        .unwrap_or_default()
}

/// Descend a dotted path (`Labels.app`) into `row`; an empty path is the row itself.
pub fn lookup<'a>(row: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .filter(|key| !key.is_empty())
        .try_fold(row, |current, key| current.get(key))
}

/// A field as a person reads it: strings bare, null empty, everything else as JSON.
pub fn display(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Whether `row` satisfies every filter.
pub fn matches(row: &Value, filters: &[String]) -> bool {
    filters.iter().all(|filter| matches_one(row, filter))
}

/// One filter: `key=value`, or `label=key=value` which reads `Labels.key`.
fn matches_one(row: &Value, filter: &str) -> bool {
    let Some((key, want)) = filter.split_once('=') else {
        return false;
    };
    let (path, want) = match (key, want.split_once('=')) {
        ("label", Some((name, value))) => (format!("Labels.{name}"), value),
        _ => (key.to_string(), want),
    };
    lookup(row, &path)
        .map(display)
        .is_some_and(|got| got == want)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row() -> Value {
        json!({"Path": "srcs/.env", "Size": 1314, "Private": false,
               "Labels": {"app": "wordpress", "tier": "web"}})
    }

    /// The three forms an operator actually types, plus literal text around them.
    #[test]
    fn the_three_expression_forms_render() {
        assert_eq!(render("{{.Path}}", &row()), "srcs/.env");
        assert_eq!(render("{{.Size}} bytes", &row()), "1314 bytes");
        assert_eq!(
            render("{{.Labels.app}}/{{.Labels.tier}}", &row()),
            "wordpress/web"
        );
        assert_eq!(
            render("{{ .Private }}", &row()),
            "false",
            "whitespace inside braces is fine"
        );
        assert!(
            render("{{json .}}", &row()).starts_with('{'),
            "json . is the whole row"
        );
    }

    /// An unknown key renders as nothing rather than failing, so one template can be aimed
    /// at several verbs; an unterminated brace is passed through literally.
    #[test]
    fn unknown_keys_and_broken_braces_do_not_fail() {
        assert_eq!(render("[{{.Nope}}]", &row()), "[]");
        assert_eq!(render("[{{.Labels.nope}}]", &row()), "[]");
        assert_eq!(render("a {{.Path", &row()), "a {{.Path");
        assert_eq!(render("no braces", &row()), "no braces");
    }

    /// Filters are equality on the displayed value, `label=` descends, and they AND.
    #[test]
    fn filters_and_and_descend_into_labels() {
        let f = |list: &[&str]| {
            matches(
                &row(),
                &list.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )
        };
        assert!(f(&["Path=srcs/.env"]));
        assert!(f(&["Size=1314"]), "a number compares by its display form");
        assert!(f(&["label=app=wordpress"]));
        assert!(
            f(&["label=app=wordpress", "Private=false"]),
            "both must hold"
        );
        assert!(
            !f(&["label=app=wordpress", "Private=true"]),
            "one failing filter rejects"
        );
        assert!(!f(&["label=app=mariadb"]));
        assert!(!f(&["Nope=x"]), "an absent key never matches");
        assert!(!f(&["malformed"]), "a filter with no '=' matches nothing");
        assert!(f(&[]), "no filters keeps everything");
    }
}
