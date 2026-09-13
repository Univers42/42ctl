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
//!
//! A template also carries `\t` and `\n` as the two characters a shell hands over inside
//! quotes, so `--format 'table {{.A}}\t{{.B}}'` means columns rather than a literal backslash.

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

/// Interpret the `\t`, `\n` and `\\` escapes that a shell passes through literally inside
/// quotes, so a tab in a `--format` template separates columns instead of printing as two
/// characters. Any other backslash is left alone rather than swallowed.
pub fn unescape(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(current) = chars.next() {
        match (current, chars.peek()) {
            ('\\', Some('t')) => out.push('\t'),
            ('\\', Some('n')) => out.push('\n'),
            ('\\', Some('\\')) => out.push('\\'),
            _ => {
                out.push(current);
                continue;
            }
        }
        chars.next();
    }
    out
}

/// The column names a `table` template implies: per tab-separated cell, the last dotted
/// segment of the first field it mentions, or nothing when the cell is literal text.
///
/// One name per cell, even for a cell that names no field, because the header and the row
/// must have the same number of columns or the table shifts under its own heading.
pub fn columns(template: &str) -> Vec<String> {
    template
        .split('\t')
        .map(|cell| leading_field(cell).unwrap_or_default())
        .collect()
}

/// The last dotted segment of the first `{{.field}}` in `cell`, if it holds one.
fn leading_field(cell: &str) -> Option<String> {
    let after = &cell[cell.find("{{")? + 2..];
    let expr = after[..after.find("}}")?].trim().strip_prefix('.')?;
    expr.rsplit('.').next().map(ToString::to_string)
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

/// Whether `row` satisfies every filter, with each key resolved against the `known` columns.
pub fn matches(row: &Value, filters: &[String], known: &[&str]) -> bool {
    filters.iter().all(|filter| matches_one(row, filter, known))
}

/// One filter: `key=value`, or `label=key=value` which reads `Labels.key`.
fn matches_one(row: &Value, filter: &str, known: &[&str]) -> bool {
    let Some((key, want)) = filter.split_once('=') else {
        return false;
    };
    let (path, want) = match (key.eq_ignore_ascii_case("label"), want.split_once('=')) {
        (true, Some((name, value))) => (format!("Labels.{name}"), value),
        _ => (resolve(key, known), want),
    };
    lookup(row, &path)
        .map(display)
        .is_some_and(|got| got == want)
}

/// The column `key` names, matched without regard to case, so `--filter role=member` reaches
/// the `Role` column a table prints. An unrecognised key is used as written, which is what
/// lets a dotted path into a nested object still work.
fn resolve(key: &str, known: &[&str]) -> String {
    known
        .iter()
        .find(|column| column.eq_ignore_ascii_case(key))
        .map_or_else(|| key.to_string(), |column| (*column).to_string())
}

/// Refuse a filter whose key names no column, before it is applied.
///
/// A filter that keeps nothing is refused too (`ui::kept_rows`), but that message cannot say
/// whether the key or the value was wrong. Checking the key first gives the mistyped key its
/// own error — `--filter rol=member` names `rol` rather than reporting that nobody matched.
pub fn check_keys(filters: &[String], known: &[&str]) -> anyhow::Result<()> {
    for filter in filters {
        let Some((key, _)) = filter.split_once('=') else {
            anyhow::bail!("filter '{filter}' is not KEY=VALUE");
        };
        let head = key.split('.').next().unwrap_or(key);
        let ok = head.eq_ignore_ascii_case("label")
            || known.iter().any(|column| column.eq_ignore_ascii_case(head));
        if !ok {
            anyhow::bail!(
                "unknown filter key '{key}' — this list has {}",
                known.join(", ")
            );
        }
    }
    Ok(())
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

    /// The escapes a shell hands over literally become the characters they name; anything
    /// else keeps its backslash rather than vanishing.
    #[test]
    fn shell_escapes_become_the_characters_they_name() {
        assert_eq!(unescape(r"a\tb"), "a\tb");
        assert_eq!(unescape(r"a\nb"), "a\nb");
        assert_eq!(
            unescape(r"a\\tb"),
            "a\\tb",
            "an escaped backslash is not a tab"
        );
        assert_eq!(
            unescape(r"C:\path"),
            r"C:\path",
            "an unknown escape is left alone"
        );
        assert_eq!(unescape("plain"), "plain");
    }

    /// A `table` template names its own columns, one per tab-separated cell so the header
    /// cannot drift out of step with the row beneath it.
    #[test]
    fn a_table_template_names_one_column_per_cell() {
        assert_eq!(columns(r"{{.Path}}"), vec!["Path"]);
        assert_eq!(columns("{{.ID}}\t{{.Labels.app}}"), vec!["ID", "app"]);
        assert_eq!(
            columns("{{.A}}\tliteral\t{{.B}}"),
            vec!["A", "", "B"],
            "a literal cell still takes a column"
        );
        assert_eq!(columns("{{json .}}"), vec![""], "json . names no column");
    }

    /// Filters are equality on the displayed value, `label=` descends, and they AND.
    #[test]
    fn filters_and_and_descend_into_labels() {
        let f = |list: &[&str]| {
            matches(
                &row(),
                &list.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                &["Path", "Size", "Private", "Labels"],
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
        assert!(
            f(&["path=srcs/.env", "PRIVATE=false"]),
            "a key matches its column whatever the case"
        );
    }

    /// A key that names no column is refused by name, so a typo cannot read as "nothing
    /// here"; a key that does name one is accepted even when it will match no row.
    #[test]
    fn an_unknown_filter_key_is_refused_and_a_known_one_is_not() {
        let known = &["Path", "Size", "Labels"];
        let keys = |list: &[&str]| {
            check_keys(
                &list.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                known,
            )
        };
        assert!(keys(&["Path=x"]).is_ok(), "an exact column name");
        assert!(keys(&["size=99"]).is_ok(), "case does not matter");
        assert!(
            keys(&["label=app=web"]).is_ok(),
            "labels are always allowed"
        );
        assert!(
            keys(&["Labels.app=web"]).is_ok(),
            "a dotted path into a column"
        );
        assert!(keys(&[]).is_ok());

        let error = keys(&["pth=x"]).expect_err("typo").to_string();
        assert!(error.contains("pth"), "names the offending key: {error}");
        assert!(
            error.contains("Path"),
            "names the columns there are: {error}"
        );
        assert!(
            keys(&["malformed"]).is_err(),
            "a filter with no '=' is refused"
        );
    }
}
