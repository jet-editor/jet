use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub fn fuzzy_match(candidate: &str, query: &str) -> bool {
    fuzzy_score(candidate, query).is_some()
}

pub fn fuzzy_score(candidate: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buffer = Vec::new();
    let haystack = Utf32Str::new(candidate, &mut buffer);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    pattern
        .score(haystack, &mut matcher)
        .filter(|score| *score > 0)
        .map(|score| score as i32)
}
