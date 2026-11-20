//! The Arch news feed: its items, read from the RSS text, and which of them announce a step
//! that the user must take by hand.

/// One news item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub link: String,
    /// The publication time, in seconds since 1970 UTC.
    pub published: i64,
    /// The text of the item, without its HTML markup.
    pub body: String,
}

/// Phrases that the Arch news uses when an update needs the user to act, compared without
/// case. They come from real titles and texts, such as "linux-firmware >= 20250508 requires
/// manual intervention", "Manual intervention for pacman 7.0.0 and local repositories
/// required", "plasma 6.4.0 will need manual intervention if you are on X11" and "The sshd
/// service needs to be restarted after upgrading to openssh-9.8p1".
const MANUAL_STEP_PHRASES: &[&str] = &[
    "manual intervention",
    "intervention required",
    "requires manual",
    "action required",
    "action is required",
    "needs to be restarted",
    "must be restarted",
];

impl Item {
    /// Whether the title or the text announces a step that the user must take by hand.
    pub fn announces_manual_step(&self) -> bool {
        let text = format!("{}\n{}", self.title, self.body).to_lowercase();
        MANUAL_STEP_PHRASES
            .iter()
            .any(|phrase| text.contains(phrase))
    }
}

/// The items of the RSS document `xml`, in the feed's order. An item without a title, a link
/// or a date it can read is left out, because it could never be marked read.
pub fn parse(xml: &str) -> Result<Vec<Item>, String> {
    let document = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let channel = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name("channel"))
        .ok_or("the feed has no channel")?;
    Ok(channel
        .children()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|item| {
            let field = |name: &str| {
                item.children()
                    .find(|node| node.has_tag_name(name))
                    .map(|node| {
                        node.children()
                            .filter_map(|child| child.text())
                            .collect::<String>()
                    })
            };
            Some(Item {
                title: field("title")?.trim().to_owned(),
                link: field("link")?.trim().to_owned(),
                published: rfc2822(&field("pubDate")?)?,
                body: plain_text(&field("description").unwrap_or_default()),
            })
        })
        .collect())
}

/// The items published after `read`, oldest first.
pub fn unread(items: &[Item], read: i64) -> Vec<&Item> {
    let mut unread: Vec<&Item> = items.iter().filter(|item| item.published > read).collect();
    unread.sort_by_key(|item| item.published);
    unread
}

/// The seconds since 1970 UTC of an RFC 2822 date, such as `Sat, 08 Nov 2025 14:22:36 +0000`,
/// the form of RSS `pubDate`. The weekday and the seconds are optional.
pub fn rfc2822(text: &str) -> Option<i64> {
    let text = text.trim();
    let text = match text.split_once(',') {
        Some((_, rest)) => rest,
        None => text,
    };
    let mut words = text.split_whitespace();
    let day: i64 = words.next()?.parse().ok()?;
    let month = month_number(words.next()?)?;
    let year: i64 = words.next()?.parse().ok()?;
    let mut clock = words.next()?.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = clock.next().map_or(Some(0), |s| s.parse().ok())?;
    let offset = zone_offset(words.next().unwrap_or("+0000"))?;
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

fn month_number(name: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let name = name.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|month| name.starts_with(month))
        .map(|index| index as i64 + 1)
}

/// The zone's offset from UTC in seconds.
fn zone_offset(zone: &str) -> Option<i64> {
    match zone.to_ascii_uppercase().as_str() {
        "GMT" | "UT" | "UTC" | "Z" => return Some(0),
        _ => {}
    }
    let (sign, digits) = match zone.as_bytes().first()? {
        b'+' => (1, &zone[1..]),
        b'-' => (-1, &zone[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = digits[2..].parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

/// The days from 1970-01-01 to the given date of the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date of `seconds` since 1970 UTC as `YYYY-MM-DD`.
pub fn date(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    // The inverse of `days_from_civil`.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The text of an HTML fragment: tags dropped, paragraphs apart by one blank line, the
/// common entities decoded.
fn plain_text(html: &str) -> String {
    let mut text = String::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + end].trim_start_matches('/');
        let name: String = tag
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if matches!(name.as_str(), "p" | "br" | "li" | "pre" | "div") {
            text.push('\n');
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);
    let text = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    let mut lines = Vec::new();
    for line in text.lines().map(str::trim) {
        if !line.is_empty() || lines.last().is_some_and(|last: &&str| !last.is_empty()) {
            lines.push(line);
        }
    }
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two items of the Arch news feed as archlinux.org serves it, shortened, plus one item
    /// with no date.
    const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom"><channel><title>Arch Linux: Recent news updates</title><link>https://archlinux.org/news/</link><description>The latest and greatest news from the Arch Linux distribution.</description><language>en-us</language>
<item><title>Valkey to replace Redis in the [extra] Repository</title><link>https://archlinux.org/news/valkey-to-replace-redis-in-the-extra-repository/</link><description>&lt;p&gt;Valkey, a high-performance key/value datastore, will be replacing redis in the [extra] repository.&lt;/p&gt;</description><dc:creator xmlns:dc="http://purl.org/dc/elements/1.1/">Andrew Crerar</dc:creator><pubDate>Thu, 17 Apr 2025 15:44:13 +0000</pubDate><guid isPermaLink="false">tag:archlinux.org,2025-04-17:/news/valkey-to-replace-redis-in-the-extra-repository/</guid></item>
<item><title>linux-firmware &gt;= 20250508.788aadc8-2 requires manual intervention</title><link>https://archlinux.org/news/linux-firmware-2025050878aadc8-2-requires-manual-intervention/</link><description>&lt;p&gt;With &lt;code&gt;20250508.788aadc8-2&lt;/code&gt; the NVIDIA firmware was split.&lt;/p&gt;&lt;p&gt;To upgrade, remove it first:&lt;/p&gt;&lt;pre&gt;&lt;code&gt;pacman -Rdd linux-firmware
pacman -Syu linux-firmware
&lt;/code&gt;&lt;/pre&gt;</description><pubDate>Sat, 21 Jun 2025 18:43:36 +0000</pubDate></item>
<item><title>No date</title><link>https://archlinux.org/news/no-date/</link></item>
</channel></rss>"#;

    #[test]
    fn the_feed_gives_each_dated_item_with_its_text() {
        let items = parse(FEED).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[1].title,
            "linux-firmware >= 20250508.788aadc8-2 requires manual intervention"
        );
        assert_eq!(
            items[1].link,
            "https://archlinux.org/news/linux-firmware-2025050878aadc8-2-requires-manual-intervention/"
        );
        assert_eq!(date(items[1].published), "2025-06-21");
        assert_eq!(
            items[1].body,
            "With 20250508.788aadc8-2 the NVIDIA firmware was split.\n\n\
             To upgrade, remove it first:\n\n\
             pacman -Rdd linux-firmware\n\
             pacman -Syu linux-firmware"
        );
    }

    #[test]
    fn only_the_items_that_announce_a_manual_step_count() {
        let items = parse(FEED).unwrap();
        let manual: Vec<&str> = items
            .iter()
            .filter(|item| item.announces_manual_step())
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(
            manual,
            ["linux-firmware >= 20250508.788aadc8-2 requires manual intervention"]
        );
        let in_body = Item {
            title: "OpenSSH 9.8p1".to_owned(),
            link: String::new(),
            published: 0,
            body: "The sshd service needs to be restarted after upgrading.".to_owned(),
        };
        assert!(in_body.announces_manual_step());
    }

    #[test]
    fn items_published_after_the_read_mark_are_unread_oldest_first() {
        let items = parse(FEED).unwrap();
        let valkey = items[0].published;
        assert_eq!(unread(&items, 0).len(), 2);
        assert_eq!(unread(&items, 0)[0].published, valkey);
        let after_valkey = unread(&items, valkey);
        assert_eq!(after_valkey.len(), 1);
        assert!(after_valkey[0].title.starts_with("linux-firmware"));
        assert!(unread(&items, items[1].published).is_empty());
    }

    #[test]
    fn dates_read_with_their_zone() {
        assert_eq!(rfc2822("Thu, 01 Jan 1970 00:00:00 +0000"), Some(0));
        assert_eq!(
            rfc2822("Sat, 21 Jun 2025 18:43:36 +0000"),
            Some(1_750_531_416)
        );
        assert_eq!(rfc2822("21 Jun 2025 20:43:36 +0200"), Some(1_750_531_416));
        assert_eq!(
            rfc2822("Sat, 21 Jun 2025 18:43:36 GMT"),
            Some(1_750_531_416)
        );
        assert_eq!(rfc2822("Sat, 21 Jun 2025 18:43 GMT"), Some(1_750_531_380));
        assert_eq!(rfc2822("yesterday"), None);
        assert_eq!(date(1_750_531_416), "2025-06-21");
        assert_eq!(date(951_782_400), "2000-02-29");
    }
}
