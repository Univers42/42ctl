/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   ui.rs                                                :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/20 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/20 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Terminal presentation — TTY-aware ANSI styling, an aligned table, relative timestamps,
//! and friendly error hints. Zero dependencies and zero global state: every call decides
//! styling from whether stdout is a terminal, so piped output stays plain and parseable.

use std::io::IsTerminal;
use std::time::{SystemTime, UNIX_EPOCH};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";

/// Whether to style output: `NO_COLOR` forces it off, `CLICOLOR_FORCE` forces it on, else it
/// follows whether stdout is a terminal — so piped output is plain by default but overridable.
pub fn styled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some() {
        return true;
    }
    std::io::stdout().is_terminal()
}

/// Wrap `text` in an ANSI `code`…RESET pair when styled, else return it unchanged.
fn paint(code: &str, text: &str) -> String {
    if styled() {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

/// Green success text.
pub fn ok(text: &str) -> String {
    paint(GREEN, text)
}

/// Red failure text.
pub fn bad(text: &str) -> String {
    paint(RED, text)
}

/// Yellow warning text.
pub fn warn(text: &str) -> String {
    paint(YELLOW, text)
}

/// Cyan accent for labels and identifiers.
pub fn accent(text: &str) -> String {
    paint(CYAN, text)
}

/// Dim secondary text.
pub fn dim(text: &str) -> String {
    paint(DIM, text)
}

/// Bold text for titles.
pub fn bold(text: &str) -> String {
    paint(BOLD, text)
}

/// Bold cyan — section titles and the overview card.
pub fn title(text: &str) -> String {
    paint(&format!("{BOLD}{CYAN}"), text)
}

/// A framed card: `title` in bold over `lines`, all padded to one width. Plain indented
/// lines when not styled, so piped output stays greppable.
pub fn boxed(title: &str, lines: &[&str]) -> String {
    let all = std::iter::once(title).chain(lines.iter().copied());
    let width = all.clone().map(|l| l.chars().count()).max().unwrap_or(0);
    if !styled() {
        return all.map(|l| format!("  {l}\n")).collect();
    }
    let bar = "─".repeat(width + 4);
    let side = accent("│");
    let mut out = format!("  {}\n", accent(&format!("╭{bar}╮")));
    out.push_str(&format!(
        "  {side}  {}  {side}\n",
        bold(&format!("{title:<width$}"))
    ));
    for line in lines {
        out.push_str(&format!("  {side}  {line:<width$}  {side}\n"));
    }
    out.push_str(&format!("  {}\n", accent(&format!("╰{bar}╯"))));
    out
}

/// A section heading: `▸ Title` in bold cyan over a dim rule, with a blank line before.
pub fn section(text: &str) -> String {
    let rule = "─".repeat(text.chars().count() + 2);
    format!("\n  {}\n  {}\n", title(&format!("▸ {text}")), dim(&rule))
}

/// Write a block of text to stdout, treating a closed pipe (`42ctl help | head`) as done
/// rather than a panic — the reader chose to stop, nothing is lost.
pub fn emit(text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

/// Print a success line — a green check plus `message` on a TTY, plain `message` when piped.
pub fn success(message: &str) {
    if styled() {
        println!("{} {message}", ok("✓"));
    } else {
        println!("{message}");
    }
}

/// Print a `label: value` row with a cyan, left-padded label (plain when piped).
pub fn field(label: &str, value: &str) {
    println!("{} {value}", accent(&format!("{label:<9}")));
}

/// How the operator asked for a listing to be shaped. Lives here rather than in `cli/` so the
/// inner layers can render without depending on the clap types; `cli::Output::shape` builds it.
#[derive(Clone, Copy, Default)]
pub struct Shape<'a> {
    /// `--format`: `json`, a `table …` template, or a bare `{{.Field}}` template.
    pub format: Option<&'a str>,
    /// `--filter KEY=VALUE`, ANDed.
    pub filter: &'a [String],
    /// `-q`: the first column alone, for `$( … )` composition.
    pub quiet: bool,
}

impl Shape<'_> {
    /// Whether the operator asked for nothing in particular, so a verb may keep its own
    /// default presentation (a hint on an empty vault, tab-separated lines down a pipe).
    pub fn is_default(&self) -> bool {
        self.format.is_none() && self.filter.is_empty() && !self.quiet
    }
}

/// Render listing `rows` (JSON objects keyed by the names in `headers`) as the operator asked.
///
/// No `--format` is today's table, so nothing changes by default. `-q` prints the first column
/// alone. `json` is the kept rows as a JSON array. `table {{.A}}\t{{.B}}` is a table whose
/// columns the template chooses; anything else is that template rendered once per row.
/// `--filter` narrows first and is ANDed. A key that is not a column is refused by name, and a
/// filter that keeps no row is refused too — both are typos an operator must hear about.
pub fn render(
    headers: &[&str],
    rows: Vec<serde_json::Value>,
    shape: Shape<'_>,
) -> anyhow::Result<()> {
    let kept = kept_rows(rows, shape.filter, headers)?;
    if shape.quiet {
        return emit(&ids(headers, &kept));
    }
    match shape.format {
        None => table(headers, &cells(headers, &kept)),
        Some("json") => println!("{}", serde_json::to_string_pretty(&kept)?),
        Some(spec) => templated(spec, &kept)?,
    }
    Ok(())
}

/// Apply `filter` to `rows` after checking that every key it names is a column that exists.
///
/// A filter that keeps nothing is an error, as `docs/vault.md` has always said. A mistyped
/// VALUE (`Role=memebr`) otherwise prints the same clean nothing as a true "nobody", and the key
/// check cannot catch it. The rule is as old as `env files --filter` and was briefly reversed
/// in favour of docker's empty success; that reversal broke `s38`, contradicted the manual, and
/// bought nothing for `rm $(… -q --filter …)`, since every `rm` refuses an empty target list.
fn kept_rows(
    rows: Vec<serde_json::Value>,
    filter: &[String],
    headers: &[&str],
) -> anyhow::Result<Vec<serde_json::Value>> {
    crate::core::template::check_keys(filter, headers)?;
    let kept: Vec<serde_json::Value> = rows
        .into_iter()
        .filter(|row| crate::core::template::matches(row, filter, headers))
        .collect();
    if kept.is_empty() && !filter.is_empty() {
        anyhow::bail!("no row matches {}", filter.join(", "));
    }
    Ok(kept)
}

/// `-q`: the first header's value on each row and nothing else, so the listing composes into
/// the next command's arguments. The first column is the identifier every verb takes back.
///
/// A caller with no columns gets nothing, because an empty path reads as the whole row and
/// `-q` is spliced straight into the next command's argument list: printing the JSON object
/// there would hand a verb an argument nobody typed.
fn ids(headers: &[&str], rows: &[serde_json::Value]) -> String {
    let Some(key) = headers.first().copied().filter(|name| !name.is_empty()) else {
        return String::new();
    };
    rows.iter()
        .filter_map(|row| crate::core::template::lookup(row, key))
        .map(|value| format!("{}\n", crate::core::template::display(value)))
        .collect()
}

/// A `--format` template: `table …` prints a header the template names, anything else prints
/// one rendered line per row with no header at all.
fn templated(spec: &str, rows: &[serde_json::Value]) -> anyhow::Result<()> {
    let spec = crate::core::template::unescape(spec);
    match spec.strip_prefix("table ") {
        Some(line) => {
            templated_table(line, rows);
            Ok(())
        }
        None => emit(&lines(&spec, rows)),
    }
}

/// `template` rendered once per row, each line terminated, as one block to write.
fn lines(template: &str, rows: &[serde_json::Value]) -> String {
    rows.iter()
        .map(|row| format!("{}\n", crate::core::template::render(template, row)))
        .collect()
}

/// Render `line` per row, split on tabs into columns, under the header the template names.
fn templated_table(line: &str, rows: &[serde_json::Value]) {
    let names = crate::core::template::columns(line);
    let headers: Vec<&str> = names.iter().map(String::as_str).collect();
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            crate::core::template::render(line, row)
                .split('\t')
                .map(ToString::to_string)
                .collect()
        })
        .collect();
    table(&headers, &body);
}

/// Project JSON rows onto the header order as display strings, for the table.
fn cells(headers: &[&str], rows: &[serde_json::Value]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| {
            headers
                .iter()
                .map(|key| {
                    crate::core::template::lookup(row, key)
                        .map(crate::core::template::display)
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect()
}

/// Print `rows` under `headers` as a bold-headed, dim-ruled, space-aligned table. Callers
/// use this only on a TTY; piped call sites stay tab-separated so scripts keep working.
pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    let head: Vec<String> = headers.iter().map(|h| (*h).to_string()).collect();
    let widths = column_widths(&head, rows);
    println!(
        "{}",
        paint(&format!("{BOLD}{CYAN}"), &render_row(&head, &widths))
    );
    println!("{}", dim(&rule(&widths)));
    for row in rows {
        println!("{}", render_row(row, &widths));
    }
}

/// The max display width per column across the header and every row.
fn column_widths(head: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = head.iter().map(|c| c.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let width = cell.chars().count();
            if i < widths.len() && width > widths[i] {
                widths[i] = width;
            }
        }
    }
    widths
}

/// Render one row as left-padded columns separated by a two-space gutter.
fn render_row(cells: &[String], widths: &[usize]) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let pad = widths.get(i).copied().unwrap_or(0);
        out.push_str(&format!("{cell:<pad$}"));
        if i + 1 < cells.len() {
            out.push_str("  ");
        }
    }
    out
}

/// A horizontal rule sized to the rendered row width (columns plus two-space gutters).
fn rule(widths: &[usize]) -> String {
    let total: usize = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 2;
    "─".repeat(total)
}

/// Format a Unix-epoch second count as a short relative age: `5s ago`, `3m ago`, `2h ago`, `9d ago`.
pub fn reltime(epoch_secs: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(epoch_secs);
    let delta = (now - epoch_secs).max(0);
    match delta {
        0..=59 => format!("{delta}s ago"),
        60..=3599 => format!("{}m ago", delta / 60),
        3600..=86_399 => format!("{}h ago", delta / 3600),
        _ => format!("{}d ago", delta / 86_400),
    }
}

/// Format a byte count the way `docker stats` does: binary units, one decimal above KiB.
///
/// Binary rather than decimal because these are memory figures, where a machine sized
/// "256MB" holds 256 MiB and rounding them as powers of ten would make a full machine read
/// as over-capacity.
pub fn bytes(count: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = count as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{count}B");
    }
    format!("{size:.1}{}", UNITS[unit])
}

/// Print `error` and its cause chain in red on stderr, plus a one-line next step for the
/// common transport/gRPC failures so the user knows what to do.
pub fn report_error(error: &anyhow::Error) {
    eprintln!("{} {error:#}", bad("error:"));
    if let Some(hint) = hint_for(error) {
        eprintln!("{}", dim(&format!("  → {hint}")));
    }
}

/// Map a transport error or a gRPC status to a short, actionable hint, if recognized.
fn hint_for(error: &anyhow::Error) -> Option<&'static str> {
    if error.downcast_ref::<tonic::transport::Error>().is_some() {
        return Some("can't reach the server — check `42ctl config show` and your connection");
    }
    let status = error.downcast_ref::<tonic::Status>()?;
    match status.code() {
        tonic::Code::NotFound => Some("no such secret — list yours with `42ctl vault ls`"),
        tonic::Code::Unauthenticated => Some("log in first: `42ctl auth login --tenant <name>`"),
        tonic::Code::PermissionDenied => Some("denied — your contract or role lacks this access"),
        tonic::Code::Unavailable => {
            Some("server unavailable — retry, or check `42ctl config show`")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_climbs_units_and_keeps_small_counts_exact() {
        assert_eq!(bytes(0), "0B");
        assert_eq!(bytes(1023), "1023B");
        assert_eq!(bytes(1024), "1.0KiB");
        assert_eq!(bytes(5_181_440), "4.9MiB");
        assert_eq!(bytes(268_435_456), "256.0MiB");
        assert_eq!(
            bytes(u64::MAX),
            "16777216.0TiB",
            "the largest count stops at the last unit rather than wrapping"
        );
    }

    #[test]
    fn reltime_buckets_scale_by_unit() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        assert!(reltime(now).ends_with("s ago"));
        assert!(reltime(now - 120).ends_with("m ago"));
        assert!(reltime(now - 7200).ends_with("h ago"));
        assert!(reltime(now - 172_800).ends_with("d ago"));
    }

    /// `-q` exists to be spliced into the next command, so it prints the first column and
    /// nothing else — no header, no rule, no second field.
    #[test]
    fn quiet_prints_the_first_column_alone() {
        let rows = vec![
            serde_json::json!({"ID": "org_a", "Name": "Acme"}),
            serde_json::json!({"ID": "org_b", "Name": "Bell"}),
        ];
        assert_eq!(ids(&["ID", "Name"], &rows), "org_a\norg_b\n");
        assert_eq!(ids(&["Name"], &rows), "Acme\nBell\n");
        assert_eq!(ids(&[], &rows), "", "no column to take is no output");
    }

    /// A filter that keeps no row is an error, and a key that is not a column is a DIFFERENT
    /// error that names the key. Both are typos an operator must hear about: a mistyped value
    /// read as "nobody matches" is a clean-looking answer to a question nobody asked.
    #[test]
    fn a_filter_that_keeps_nothing_is_an_error_and_a_typo_names_its_key() {
        let rows = vec![serde_json::json!({"ID": "org_a", "Role": "owner"})];
        let headers = &["ID", "Role"];
        let keep = |filter: &str| kept_rows(rows.clone(), &[filter.to_string()], headers);

        let empty = keep("Role=memebr")
            .expect_err("a value that matches nothing")
            .to_string();
        assert!(empty.contains("no row matches Role=memebr"), "{empty}");
        assert_eq!(
            keep("role=owner").expect("known key").len(),
            1,
            "case-insensitive"
        );
        let typo = keep("rol=member").expect_err("typo").to_string();
        assert!(
            typo.contains("unknown filter key 'rol'"),
            "names the key: {typo}"
        );
        assert_eq!(
            kept_rows(rows.clone(), &[], headers)
                .expect("unfiltered")
                .len(),
            1,
            "no filter keeps everything"
        );
        assert!(
            kept_rows(Vec::new(), &[], headers)
                .expect("unfiltered")
                .is_empty(),
            "an empty listing with no filter is an honest empty, not an error"
        );
    }

    #[test]
    fn column_widths_take_the_widest_cell() {
        let head = vec!["A".to_string(), "BB".to_string()];
        let rows = vec![vec!["xxxx".to_string(), "y".to_string()]];
        assert_eq!(column_widths(&head, &rows), vec![4, 2]);
    }

    #[test]
    fn render_pads_to_width_with_a_gutter() {
        let row = vec!["a".to_string(), "b".to_string()];
        assert_eq!(render_row(&row, &[3, 1]), "a    b");
    }
}
