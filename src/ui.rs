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

/// Print `rows` under `headers` as a bold-headed, dim-ruled, space-aligned table. Callers
/// use this only on a TTY; piped call sites stay tab-separated so scripts keep working.
pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    let head: Vec<String> = headers.iter().map(|h| (*h).to_string()).collect();
    let widths = column_widths(&head, rows);
    println!(
        "{}",
        paint(&format!("{BOLD}{CYAN}"), &render(&head, &widths))
    );
    println!("{}", dim(&rule(&widths)));
    for row in rows {
        println!("{}", render(row, &widths));
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
fn render(cells: &[String], widths: &[usize]) -> String {
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

    #[test]
    fn column_widths_take_the_widest_cell() {
        let head = vec!["A".to_string(), "BB".to_string()];
        let rows = vec![vec!["xxxx".to_string(), "y".to_string()]];
        assert_eq!(column_widths(&head, &rows), vec![4, 2]);
    }

    #[test]
    fn render_pads_to_width_with_a_gutter() {
        let row = vec!["a".to_string(), "b".to_string()];
        assert_eq!(render(&row, &[3, 1]), "a    b");
    }
}
