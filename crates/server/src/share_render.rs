//! Server-rendered, script-free HTML for read-only share links.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use std::collections::BTreeMap;
use tendly_core::api::EventOccurrence;

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

pub fn render(label: &str, items: &[EventOccurrence], tz: Tz, token: &str, now: DateTime<Utc>) -> String {
    let mut days: BTreeMap<String, Vec<&EventOccurrence>> = BTreeMap::new();
    for o in items {
        let key =
            if o.all_day { o.start_date.clone().unwrap_or_default() } else { o.start.with_timezone(&tz).format("%Y-%m-%d").to_string() };
        days.entry(key).or_default().push(o);
    }
    let mut body = String::new();
    if days.is_empty() {
        body.push_str("<p class=\"empty\">Nothing scheduled in this window.</p>");
    }
    for (day, list) in &days {
        let pretty = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .map(|d| d.format("%A, %B %-d, %Y").to_string())
            .unwrap_or_else(|_| day.clone());
        body.push_str(&format!("<section><h2>{}</h2><ul>", esc(&pretty)));
        for o in list {
            let when = if o.all_day {
                "All day".to_string()
            } else {
                format!("{}–{}", o.start.with_timezone(&tz).format("%H:%M"), o.end.with_timezone(&tz).format("%H:%M"))
            };
            let kind = if o.kind == "task" { "<span class=\"tag\">Task</span> " } else { "" };
            let cat = o.category.map(|c| format!("<span class=\"tag\">{}</span> ", esc(c.as_str()))).unwrap_or_default();
            body.push_str(&format!(
                "<li><span class=\"when\">{}</span><span class=\"what\">{kind}{cat}{}</span>",
                esc(&when),
                esc(&o.title)
            ));
            if let Some(l) = &o.location {
                body.push_str(&format!("<span class=\"meta\">{}</span>", esc(l)));
            }
            if let Some(d) = &o.description {
                let short: String = d.chars().take(400).collect();
                body.push_str(&format!("<span class=\"meta\">{}</span>", esc(&short).replace('\n', "<br>")));
            }
            body.push_str("</li>");
        }
        body.push_str("</ul></section>");
    }
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="robots" content="noindex, nofollow"><meta name="referrer" content="no-referrer"><title>{title} · Tendly</title>
<style>
:root{{--bg:#fbf8f3;--fg:#2b2a33;--muted:#5d5b6a;--card:#fff;--line:#e8e2d8;--accent:#6b5bd2}}
@media (prefers-color-scheme:dark){{:root{{--bg:#1d1c22;--fg:#f1eff7;--muted:#b9b6c6;--card:#27262e;--line:#3a3842;--accent:#b7adff}}}}
body{{margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif}}
main{{max-width:44rem;margin:0 auto;padding:1.5rem 1rem 3rem}}
h1{{font-size:1.5rem;margin:.2rem 0}} h2{{font-size:1rem;color:var(--muted);margin:1.6rem 0 .5rem}}
ul{{list-style:none;margin:0;padding:0}} li{{background:var(--card);border:1px solid var(--line);border-radius:14px;padding:.75rem 1rem;margin:.4rem 0;display:flex;flex-direction:column;gap:.15rem}}
.when{{font-variant-numeric:tabular-nums;color:var(--muted);font-size:.9rem}} .what{{font-weight:600}} .meta{{color:var(--muted);font-size:.9rem}}
.tag{{display:inline-block;font-size:.75rem;font-weight:600;border:1px solid var(--line);border-radius:999px;padding:0 .5rem;margin-right:.25rem;color:var(--muted)}}
.note{{color:var(--muted);font-size:.9rem}} a{{color:var(--accent)}} .empty{{color:var(--muted)}}
</style></head><body><main>
<p class="note">Read-only calendar shared from Tendly</p>
<h1>{title}</h1>
<p class="note">Times shown in {tz}. Updated {updated} UTC. <a href="/share/{token}/calendar.ics">Download .ics</a> · <a href="/share/{token}/calendar.json">JSON</a></p>
<p class="note">To subscribe, add the .ics link in your calendar app (“Add calendar from URL”). Calendar apps refresh subscriptions on their own schedule, often every few hours — changes are not instant.</p>
{body}
</main></body></html>"#,
        title = esc(label),
        tz = esc(tz.name()),
        updated = now.format("%Y-%m-%d %H:%M"),
        token = esc(token),
        body = body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_untrusted_text() {
        assert_eq!(esc("<script>alert('x')</script>&"), "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;&amp;");
    }
}
