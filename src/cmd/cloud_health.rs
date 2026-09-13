/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   cloud_health.rs                                      :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `cloud health` — is this vault42 deployment actually working?
//!
//! This is the part flyctl cannot answer. It knows a machine is `started`; it does not know
//! that the authority's contract key is 64 hex characters, that the server is pinned to that
//! key rather than to a stale one, or that `>1 machine` is not redundancy here but two
//! processes writing the same SQLite file on the same volume.
//!
//! Failures and warnings are different on purpose. A `fail` is something that is broken now;
//! a `warn` is something that will cost you later — an ageing snapshot, a machine left
//! running when it is meant to scale to zero. The exit status follows the failures only, so
//! this can be a gate without the warnings making it flap.

use crate::adapters::fly;
use crate::adapters::flyctl::Flyctl;
use crate::cli::Output;
use crate::cmd::cloud::targets;
use crate::profile::Endpoint;
use crate::ui;

/// A snapshot older than this is worth mentioning.
const SNAPSHOT_WARN_HOURS: i64 = 36;

/// A snapshot older than this is a failure: a week of lost writes is not a backup.
const SNAPSHOT_FAIL_HOURS: i64 = 24 * 7;

/// One check's verdict.
struct Verdict {
    check: String,
    status: &'static str,
    detail: String,
}

/// Run every check and render the table; exit non-zero if any failed.
pub async fn run(
    fly: &Flyctl,
    endpoint: &Endpoint,
    no_wake: bool,
    out: &Output,
) -> anyhow::Result<()> {
    let mut verdicts = Vec::new();
    for app in targets(endpoint, None)? {
        verdicts.extend(check_app(fly, &app).await);
    }
    if no_wake {
        verdicts.push(skipped("authority /healthz"));
        verdicts.push(skipped("contract key"));
    } else {
        verdicts.push(check_healthz(endpoint).await);
        verdicts.push(check_contract_key(endpoint).await);
    }
    let failures = verdicts.iter().filter(|v| v.status == "fail").count();
    let rows = verdicts
        .iter()
        .map(|v| serde_json::json!({"Check": v.check, "Status": v.status, "Detail": v.detail}))
        .collect();
    ui::render(&["Check", "Status", "Detail"], rows, out.shape())?;
    if failures > 0 {
        anyhow::bail!("{failures} check(s) failed");
    }
    Ok(())
}

/// A check that was deliberately not run.
fn skipped(check: &str) -> Verdict {
    Verdict {
        check: check.to_string(),
        status: "skip",
        detail: "--no-wake: would have started a stopped machine".to_string(),
    }
}

/// Everything flyctl can answer about one app, cold.
async fn check_app(fly: &Flyctl, app: &str) -> Vec<Verdict> {
    let machines = match fly::machines(fly, app).await {
        Ok(found) => found,
        Err(error) => {
            return vec![Verdict {
                check: format!("{app} reachable"),
                status: "fail",
                detail: format!("{error:#}"),
            }]
        }
    };
    let mut verdicts = vec![count_verdict(app, machines.len())];
    for machine in &machines {
        verdicts.push(state_verdict(app, machine));
        verdicts.extend(scope_flag_verdict(app, machine));
    }
    verdicts.extend(volume_verdicts(fly, app, &machines).await);
    verdicts
}

/// One machine per app, no more: both apps keep SQLite on a volume, so a second machine is
/// two writers on one file, not redundancy.
fn count_verdict(app: &str, count: usize) -> Verdict {
    let (status, detail) = match count {
        1 => ("ok", "one machine".to_string()),
        0 => ("fail", "no machine — the app is not deployed".to_string()),
        many => (
            "fail",
            format!(
                "{many} machines share one SQLite volume — that is two writers, not redundancy"
            ),
        ),
    };
    Verdict {
        check: format!("{app} machine count"),
        status,
        detail,
    }
}

/// A machine in a state that is not one of the three it is ever meant to sit in.
fn state_verdict(app: &str, machine: &fly::Machine) -> Verdict {
    let known = ["started", "stopped", "suspended"];
    let status = if known.contains(&machine.state.as_str()) {
        "ok"
    } else {
        "fail"
    };
    Verdict {
        check: format!("{app} machine state"),
        status,
        detail: format!(
            "{} is {} ({})",
            machine.id,
            machine.state,
            machine.check_state()
        ),
    }
}

/// The server needs `VAULT42_SCOPE_KEYS_ENABLED`; without it every `env init`, `env secret set`,
/// `env keys sync` and `env keys rotate` answers UNIMPLEMENTED, which reads as a client bug.
fn scope_flag_verdict(app: &str, machine: &fly::Machine) -> Option<Verdict> {
    if !app.ends_with("-server") {
        return None;
    }
    let on = machine.config.env.get("VAULT42_SCOPE_KEYS_ENABLED");
    let (status, detail) = match on.map(String::as_str) {
        Some("1" | "true") => ("ok", "scope keys enabled".to_string()),
        other => (
            "fail",
            format!(
                "VAULT42_SCOPE_KEYS_ENABLED is {} — every scope-key verb answers UNIMPLEMENTED",
                other.unwrap_or("unset")
            ),
        ),
    };
    Some(Verdict {
        check: format!("{app} scope keys"),
        status,
        detail,
    })
}

/// The volume is attached, encrypted, and recently snapshotted.
async fn volume_verdicts(fly: &Flyctl, app: &str, machines: &[fly::Machine]) -> Vec<Verdict> {
    let volumes = match fly::volumes(fly, app).await {
        Ok(found) => found,
        Err(error) => {
            return vec![Verdict {
                check: format!("{app} volumes"),
                status: "fail",
                detail: format!("{error:#}"),
            }]
        }
    };
    let mut verdicts = Vec::new();
    for volume in &volumes {
        let attached = volume
            .attached_machine_id
            .as_deref()
            .is_some_and(|id| machines.iter().any(|m| m.id == id));
        let (status, detail) = match (attached, volume.encrypted) {
            (true, true) => ("ok", format!("{} attached, encrypted", volume.name)),
            (false, _) => ("fail", format!("{} is attached to nothing", volume.name)),
            (_, false) => ("fail", format!("{} is NOT encrypted", volume.name)),
        };
        verdicts.push(Verdict {
            check: format!("{app} volume"),
            status,
            detail,
        });
        verdicts.push(snapshot_verdict(fly, app, &volume.id).await);
    }
    verdicts
}

/// How much a mistake would cost: the age of the newest snapshot.
async fn snapshot_verdict(fly: &Flyctl, app: &str, volume: &str) -> Verdict {
    let check = format!("{app} snapshot age");
    let newest = match fly::snapshots(fly, app, volume).await {
        Ok(all) => all.into_iter().map(|s| s.created_at).max(),
        Err(error) => {
            return Verdict {
                check,
                status: "warn",
                detail: format!("could not list snapshots: {error:#}"),
            }
        }
    };
    let Some(created) = newest else {
        return Verdict {
            check,
            status: "warn",
            detail: "no snapshot yet — expected on a volume created today".to_string(),
        };
    };
    age_verdict(check, &created)
}

/// Grade a snapshot by how old it is.
fn age_verdict(check: String, created: &str) -> Verdict {
    let Some(hours) = hours_since(created) else {
        return Verdict {
            check,
            status: "warn",
            detail: format!("newest snapshot {created} (could not read its age)"),
        };
    };
    let status = match hours {
        h if h >= SNAPSHOT_FAIL_HOURS => "fail",
        h if h >= SNAPSHOT_WARN_HOURS => "warn",
        _ => "ok",
    };
    Verdict {
        check,
        status,
        detail: format!("newest snapshot is {hours}h old"),
    }
}

/// The authority answers at all.
async fn check_healthz(endpoint: &Endpoint) -> Verdict {
    let url = format!("{}/healthz", endpoint.authority.trim_end_matches('/'));
    match reqwest::get(&url).await {
        Ok(response) => {
            let code = response.status();
            let body = response.text().await.unwrap_or_default();
            let ok = code.is_success() && body.trim() == "ok";
            Verdict {
                check: "authority /healthz".to_string(),
                status: if ok { "ok" } else { "fail" },
                detail: format!("{code} {}", body.trim()),
            }
        }
        Err(error) => Verdict {
            check: "authority /healthz".to_string(),
            status: "fail",
            detail: format!("{error}"),
        },
    }
}

/// The contract key is there and is a key: 64 lowercase hex characters.
///
/// A malformed key is the failure that looks healthy from outside — the authority answers,
/// the server starts, and every contract it issues is worthless.
async fn check_contract_key(endpoint: &Endpoint) -> Verdict {
    let url = format!(
        "{}/v1/contract-key",
        endpoint.authority.trim_end_matches('/')
    );
    let key = match reqwest::get(&url).await {
        Ok(response) => response
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|body| body.get("public_key")?.as_str().map(ToString::to_string)),
        Err(error) => {
            return Verdict {
                check: "contract key".to_string(),
                status: "fail",
                detail: format!("{error}"),
            }
        }
    };
    let valid = key.as_deref().is_some_and(|k| {
        k.len() == 64
            && k.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    });
    Verdict {
        check: "contract key".to_string(),
        status: if valid { "ok" } else { "fail" },
        detail: key.map_or_else(
            || "the authority returned no public_key".to_string(),
            |k| format!("{}…", &k[..k.len().min(16)]),
        ),
    }
}

/// Whole hours between an RFC 3339 timestamp and now, or `None` if it cannot be read.
fn hours_since(timestamp: &str) -> Option<i64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((now - epoch_of(timestamp)?).max(0) / 3600)
}

/// An RFC 3339 UTC timestamp as Unix seconds. Fly emits exactly this shape and nothing else,
/// so this reads it directly rather than carrying a date library for one field.
fn epoch_of(timestamp: &str) -> Option<i64> {
    let (date, rest) = timestamp.split_once('T')?;
    let time = rest.split(['.', 'Z', '+']).next()?;
    let mut parts = date.split('-').map(|n| n.parse::<i64>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    let mut clock = time.split(':').map(|n| n.parse::<i64>().ok());
    let (hour, minute, second) = (clock.next()??, clock.next()??, clock.next()??);
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days between 1970-01-01 and a civil date — Howard Hinnant's algorithm, shifted so the leap
/// rule falls at the end of the internal year.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The timestamps fly emits read back as the instants they name.
    #[test]
    fn a_fly_timestamp_reads_back_as_its_instant() {
        assert_eq!(epoch_of("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_of("2026-09-13T12:10:13.025Z"), Some(1_789_301_413));
        assert_eq!(epoch_of("2026-09-13T12:10:13Z"), Some(1_789_301_413));
        assert_eq!(
            epoch_of("2024-02-29T00:00:00Z"),
            Some(1_709_164_800),
            "a leap day"
        );
        assert_eq!(epoch_of("not a timestamp"), None);
        assert_eq!(
            epoch_of("2026-09-13"),
            None,
            "a date alone is not an instant"
        );
    }

    /// A snapshot is graded by age, and an unreadable date warns rather than passing — a
    /// backup nobody can date is not one you can rely on.
    #[test]
    fn snapshot_age_grades_by_how_much_would_be_lost() {
        let hours_ago = |h: i64| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs() as i64;
            let secs = now - h * 3600;
            let days = secs / 86_400;
            let rest = secs % 86_400;
            format!(
                "{}T{:02}:{:02}:{:02}Z",
                civil_of(days),
                rest / 3600,
                (rest % 3600) / 60,
                rest % 60
            )
        };
        assert_eq!(age_verdict("x".into(), &hours_ago(1)).status, "ok");
        assert_eq!(age_verdict("x".into(), &hours_ago(48)).status, "warn");
        assert_eq!(age_verdict("x".into(), &hours_ago(24 * 9)).status, "fail");
        assert_eq!(age_verdict("x".into(), "garbage").status, "warn");
    }

    /// The inverse of `days_from_civil`, for the test above only.
    fn civil_of(days: i64) -> String {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        format!("{year:04}-{month:02}-{day:02}")
    }

    /// More than one machine on a SQLite volume is a failure, not redundancy.
    #[test]
    fn a_second_machine_on_one_volume_fails_rather_than_warns() {
        assert_eq!(count_verdict("vault42-server", 1).status, "ok");
        assert_eq!(count_verdict("vault42-server", 0).status, "fail");
        let split = count_verdict("vault42-server", 2);
        assert_eq!(split.status, "fail");
        assert!(
            split.detail.contains("two writers"),
            "says why: {}",
            split.detail
        );
    }

    /// The scope-key flag is checked on the server only, and anything but on is a failure:
    /// without it every scope verb answers UNIMPLEMENTED, which reads as a client bug.
    #[test]
    fn the_scope_key_flag_is_a_server_check_and_must_be_on() {
        let machine = |value: Option<&str>| {
            let mut m = fly::Machine::default();
            if let Some(v) = value {
                m.config
                    .env
                    .insert("VAULT42_SCOPE_KEYS_ENABLED".to_string(), v.to_string());
            }
            m
        };
        assert!(scope_flag_verdict("vault42-authority", &machine(None)).is_none());
        assert_eq!(
            scope_flag_verdict("vault42-server", &machine(Some("1")))
                .expect("checked")
                .status,
            "ok"
        );
        let off = scope_flag_verdict("vault42-server", &machine(None)).expect("checked");
        assert_eq!(off.status, "fail");
        assert!(off.detail.contains("UNIMPLEMENTED"), "says what breaks");
    }
}
