//! The pure parts of the maintenance and rollback scenarios: the test Arch news feed, and the
//! readings of pacman's, snapper's and btrfs's output that their checks rest on.

use std::collections::BTreeSet;

use super::boot_checks::Snapshot;

/// The title of the unread news item that announces a manual step. Arch news titles say
/// "requires manual intervention" for such updates.
pub const MANUAL_TITLE: &str = "inayos-test-marker 1-1 update requires manual intervention";
pub const MANUAL_LINK: &str =
    "https://archlinux.org/news/inayos-test-marker-update-requires-manual-intervention/";
/// The title of an unread item that announces nothing to do, which the pacman hook leaves out.
pub const PLAIN_TITLE: &str = "InayOS test news without a step to take";

/// The test Arch news feed: an RSS 2.0 file in the shape of https://archlinux.org/feeds/news/,
/// with the manual-step item published at `now` and the plain item a minute before, both
/// in Unix seconds. Dates from the run keep both items newer than whatever the installed
/// system last read.
pub fn news_feed(now: u64) -> String {
    let item = |title: &str, link: &str, description: &str, date: u64| {
        format!(
            "    <item>\n      <title>{title}</title>\n      <link>{link}</link>\n      \
             <description>{description}</description>\n      <dc:creator>InayOS VM test</dc:creator>\n      \
             <pubDate>{}</pubDate>\n      <guid isPermaLink=\"false\">{link}</guid>\n    </item>\n",
            rfc2822(date)
        )
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <rss version=\"2.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n  <channel>\n    \
         <title>Arch Linux: Recent news updates</title>\n    <link>https://archlinux.org/news/</link>\n    \
         <description>The latest and greatest news from the Arch Linux distribution.</description>\n    \
         <language>en-us</language>\n    <lastBuildDate>{}</lastBuildDate>\n{}{}  </channel>\n</rss>\n",
        rfc2822(now),
        item(
            MANUAL_TITLE,
            MANUAL_LINK,
            "&lt;p&gt;The update moves a file. Delete it before you upgrade.&lt;/p&gt;",
            now
        ),
        item(
            PLAIN_TITLE,
            "https://archlinux.org/news/inayos-test-news-without-a-step-to-take/",
            "&lt;p&gt;Nothing to do.&lt;/p&gt;",
            now.saturating_sub(60)
        ),
    )
}

/// A Unix time as RSS writes it, such as `Thu, 19 Nov 2026 10:00:00 +0000`.
pub fn rfc2822(seconds: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_date(days);
    format!(
        "{}, {day:02} {} {year} {:02}:{:02}:{:02} +0000",
        DAYS[(days % 7) as usize],
        MONTHS[(month - 1) as usize],
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The year, month and day of the day `days` after 1970-01-01, by Howard Hinnant's
/// days-to-civil algorithm.
fn civil_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

/// The installed version of `package` in the output of `pacman -Q <package>`.
pub fn installed_version<'a>(pacman_q: &'a str, package: &str) -> Option<&'a str> {
    pacman_q.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next()? == package).then(|| words.next()).flatten()
    })
}

/// The limits of snapper's number cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub number: usize,
    pub important: usize,
}

/// Reads `NUMBER_LIMIT` and `NUMBER_LIMIT_IMPORTANT` from a snapper configuration. A range
/// such as `2-10` counts as its upper end, which is what the cleanup keeps when space allows.
pub fn snapper_limits(config: &str) -> Result<Limits, String> {
    let value = |key: &str| -> Result<usize, String> {
        let text = config
            .lines()
            .find_map(|line| line.trim().strip_prefix(key)?.strip_prefix('='))
            .ok_or_else(|| format!("the snapper configuration has no {key}"))?;
        let text = text.trim().trim_matches('"');
        let upper = text.rsplit('-').next().unwrap_or(text);
        upper
            .parse()
            .map_err(|_| format!("the snapper configuration's {key} is not a number: {text:?}"))
    };
    Ok(Limits {
        number: value("NUMBER_LIMIT")?,
        important: value("NUMBER_LIMIT_IMPORTANT")?,
    })
}

/// The snapshots that the number cleanup handles.
pub fn number_snapshots(snapshots: &[Snapshot]) -> Vec<&Snapshot> {
    snapshots
        .iter()
        .filter(|snapshot| snapshot.cleanup == "number")
        .collect()
}

/// What is wrong with the snapshots left after snapper's number cleanup, or none.
///
/// The cleanup keeps the `limits.number` newest number snapshots, the `limits.important`
/// newest of those marked important, and the pre or post partner of each kept snapshot, since
/// snapper deletes a pre and post pair only together. Every snapshot it keeps from the earlier
/// list is still kept by the same rule applied to the list it leaves, so a snapshot outside
/// that rule is one the cleanup should have deleted.
pub fn cleanup_problem(snapshots: &[Snapshot], limits: Limits) -> Option<String> {
    let mut numbered = number_snapshots(snapshots);
    numbered.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.number));
    let mut kept: BTreeSet<u32> = numbered
        .iter()
        .take(limits.number)
        .map(|snapshot| snapshot.number)
        .collect();
    kept.extend(
        numbered
            .iter()
            .filter(|snapshot| snapshot.important)
            .take(limits.important)
            .map(|snapshot| snapshot.number),
    );
    let partners: Vec<u32> = numbered
        .iter()
        .filter_map(|snapshot| {
            let pre = snapshot.pre_number?;
            if kept.contains(&snapshot.number) {
                Some(pre)
            } else if kept.contains(&pre) {
                Some(snapshot.number)
            } else {
                None
            }
        })
        .collect();
    kept.extend(partners);
    let extra: Vec<String> = numbered
        .iter()
        .filter(|snapshot| !kept.contains(&snapshot.number))
        .map(|snapshot| snapshot.number.to_string())
        .collect();
    (!extra.is_empty()).then(|| {
        format!(
            "the number cleanup kept {} snapshots, more than its limit of {} and {} important: snapshots {} should be gone",
            numbered.len(),
            limits.number,
            limits.important,
            extra.join(", ")
        )
    })
}

/// Whether the pacman hook `hook` has a path trigger on `target`, such as
/// `usr/bin/inayos-guard`, so that a transaction that changes that file runs it.
pub fn hook_has_path_target(hook: &str, target: &str) -> bool {
    let mut path_trigger = false;
    let mut targets = Vec::new();
    let mut found = false;
    let mut close = |path_trigger: bool, targets: &mut Vec<String>| {
        found |= path_trigger && targets.iter().any(|listed| listed == target);
        targets.clear();
    };
    for line in hook.lines().map(str::trim) {
        if line.starts_with('[') {
            close(path_trigger, &mut targets);
            path_trigger = false;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("Type", kind) => path_trigger = kind == "Path",
            ("Target", listed) => targets.push(listed.to_owned()),
            _ => {}
        }
    }
    close(path_trigger, &mut targets);
    found
}

/// The snapshot number that `snapper create --print-number` prints.
pub fn created_number(output: &str) -> Option<u32> {
    output
        .lines()
        .rev()
        .find_map(|line| line.trim().parse().ok())
}

/// The top-level subvolumes in `btrfs subvolume list` output whose name starts with `prefix`,
/// such as the `@rollback-<date>` subvolume that a rollback leaves. The subvolumes nested in
/// one, such as `@rollback-<date>/var/lib/machines` that systemd-tmpfiles made inside `@`,
/// are not counted.
pub fn subvolumes_named<'a>(list: &'a str, prefix: &str) -> Vec<&'a str> {
    list.lines()
        .filter_map(|line| line.split_once(" path ").map(|(_, path)| path.trim()))
        .map(|path| path.trim_start_matches('/'))
        .filter(|path| !path.contains('/') && path.starts_with(prefix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_dates_are_written_in_utc() {
        assert_eq!(rfc2822(0), "Thu, 01 Jan 1970 00:00:00 +0000");
        // 2026-11-19 10:00:00 UTC.
        assert_eq!(rfc2822(1_795_082_400), "Thu, 19 Nov 2026 10:00:00 +0000");
        // 2024-02-29 23:59:59 UTC, a leap day.
        assert_eq!(rfc2822(1_709_251_199), "Thu, 29 Feb 2024 23:59:59 +0000");
    }

    #[test]
    fn the_feed_holds_both_items_with_their_dates() {
        let feed = news_feed(1_795_082_400);
        assert!(feed.contains(&format!("<title>{MANUAL_TITLE}</title>")));
        assert!(feed.contains(&format!("<link>{MANUAL_LINK}</link>")));
        assert!(feed.contains(&format!("<title>{PLAIN_TITLE}</title>")));
        assert!(feed.contains("<pubDate>Thu, 19 Nov 2026 10:00:00 +0000</pubDate>"));
        assert!(feed.contains("<pubDate>Thu, 19 Nov 2026 09:59:00 +0000</pubDate>"));
    }

    #[test]
    fn the_installed_version_is_the_second_word_of_the_package_line() {
        assert_eq!(
            installed_version("inayos 0.9.0-1001\n", "inayos"),
            Some("0.9.0-1001")
        );
        assert_eq!(installed_version("inayos-keyring 1-1\n", "inayos"), None);
    }

    #[test]
    fn the_limits_come_from_the_configuration_lines() {
        let config = "# limit for number cleanup\nNUMBER_MIN_AGE=\"3600\"\nNUMBER_LIMIT=\"10\"\nNUMBER_LIMIT_IMPORTANT=\"2-5\"\n";
        assert_eq!(
            snapper_limits(config),
            Ok(Limits {
                number: 10,
                important: 5
            })
        );
        assert!(snapper_limits("NUMBER_LIMIT=\"10\"\n").is_err());
    }

    fn snapshot(number: u32, important: bool, pre_number: Option<u32>) -> Snapshot {
        Snapshot {
            number,
            kind: if pre_number.is_some() { "post" } else { "pre" }.to_owned(),
            description: String::new(),
            cleanup: "number".to_owned(),
            important,
            pre_number,
        }
    }

    const LIMITS: Limits = Limits {
        number: 4,
        important: 1,
    };

    #[test]
    fn a_list_within_the_limits_has_no_problem() {
        // The newest four, an older important pre with its post, and the snapshot that is not
        // the number cleanup's.
        let mut list = vec![
            snapshot(3, true, None),
            snapshot(4, false, Some(3)),
            snapshot(20, false, None),
            snapshot(21, false, Some(20)),
            snapshot(22, false, None),
            snapshot(23, false, Some(22)),
        ];
        list.push(Snapshot {
            cleanup: String::new(),
            ..snapshot(1, false, None)
        });
        assert_eq!(cleanup_problem(&list, LIMITS), None);
    }

    #[test]
    fn the_pre_of_the_oldest_kept_post_stays_with_it() {
        let list = [
            snapshot(19, false, None),
            snapshot(20, false, Some(19)),
            snapshot(21, false, None),
            snapshot(22, false, Some(21)),
            snapshot(23, false, None),
        ];
        assert_eq!(cleanup_problem(&list, LIMITS), None);
    }

    #[test]
    fn snapshots_past_the_limits_are_reported() {
        let list = [
            snapshot(10, false, None),
            snapshot(11, false, Some(10)),
            snapshot(20, false, None),
            snapshot(21, false, Some(20)),
            snapshot(22, false, None),
            snapshot(23, false, Some(22)),
        ];
        let problem = cleanup_problem(&list, LIMITS).unwrap();
        assert!(
            problem.contains("snapshots 11, 10 should be gone"),
            "{problem}"
        );
    }

    #[test]
    fn only_a_path_trigger_on_the_file_counts() {
        let hook = "[Trigger]\nOperation = Upgrade\nType = Path\nTarget = usr/bin/snapper\n\
                    Target = usr/bin/inayos-guard\n\n\
                    [Trigger]\nType = Package\nTarget = inayos\n\n\
                    [Action]\nWhen = PostTransaction\nExec = /usr/bin/inayos-guard check-update\n";
        assert!(hook_has_path_target(hook, "usr/bin/inayos-guard"));
        assert!(!hook_has_path_target(hook, "inayos"));
        assert!(!hook_has_path_target(hook, "usr/bin/inayos"));
        let package_only = "[Trigger]\nType = Package\nTarget = usr/bin/inayos-guard\n";
        assert!(!hook_has_path_target(package_only, "usr/bin/inayos-guard"));
    }

    #[test]
    fn snapper_prints_the_number_of_the_new_snapshot() {
        assert_eq!(created_number("42\n"), Some(42));
        assert_eq!(created_number(""), None);
    }

    #[test]
    fn the_rollback_subvolume_is_found_by_its_name() {
        let list = "ID 256 gen 120 top level 5 path @\n\
                    ID 257 gen 118 top level 5 path @snapshots\n\
                    ID 300 gen 110 top level 5 path @rollback-20261119-100000\n\
                    ID 301 gen 60 top level 257 path @snapshots/12/snapshot\n\
                    ID 302 gen 90 top level 300 path @rollback-20261119-100000/var/lib/machines\n\
                    ID 303 gen 90 top level 300 path @rollback-20261119-100000/var/lib/portables\n";
        assert_eq!(
            subvolumes_named(list, "@rollback-"),
            ["@rollback-20261119-100000"]
        );
    }
}
