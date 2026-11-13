//! Light English text normalisation before G2P: things misaki doesn't expand.

use once_cell::sync::Lazy;
use regex::Regex;

static TIME: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(\d{1,2}):(\d{2})\b").expect("valid regex"));
static MONTH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.?(\s+\d)")
        .expect("valid regex")
});
static AMP: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s&\s").expect("valid regex"));
static PERCENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\s?%").expect("valid regex"));

fn month(abbrev: &str) -> &'static str {
    match abbrev {
        "Jan" => "January",
        "Feb" => "February",
        "Mar" => "March",
        "Apr" => "April",
        "Jun" => "June",
        "Jul" => "July",
        "Aug" => "August",
        "Sep" | "Sept" => "September",
        "Oct" => "October",
        "Nov" => "November",
        _ => "December",
    }
}

pub fn english(text: &str) -> String {
    let t = TIME.replace_all(text, |c: &regex::Captures| {
        let (h, m) = (&c[1], &c[2]);
        if m == "00" {
            format!("{h} o'clock")
        } else if let Some(rest) = m.strip_prefix('0') {
            format!("{h} oh {rest}")
        } else {
            format!("{h} {m}")
        }
    });
    let t = MONTH.replace_all(&t, |c: &regex::Captures| {
        format!("{}{}", month(&c[1]), &c[2])
    });
    let t = AMP.replace_all(&t, " and ");
    let t = PERCENT.replace_all(&t, "$1 percent");
    t.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_times_months_and_symbols() {
        assert_eq!(english("at 3:45"), "at 3 45");
        assert_eq!(english("at 9:00"), "at 9 o'clock");
        assert_eq!(english("at 7:05"), "at 7 oh 5");
        assert_eq!(english("Nov. 11"), "November 11");
        assert_eq!(english("salt & pepper"), "salt and pepper");
        assert_eq!(english("50% off"), "50 percent off");
        assert_eq!(english("Novel 3"), "Novel 3");
    }
}
