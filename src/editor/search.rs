use aho_corasick::AhoCorasick;
use memchr::{memchr_iter, memmem};
use rayon::prelude::*;

const PARALLEL_SEARCH_THRESHOLD: usize = 8 * 1024 * 1024;
const PARALLEL_CHUNK_SIZE: usize = 4 * 1024 * 1024;

/// Byte-oriented search engine used by the headless benchmark path.
pub struct SearchEngine {
    pattern: String,
    needle: Vec<u8>,
}

impl SearchEngine {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            needle: pattern.as_bytes().to_vec(),
        }
    }

    pub fn count_in_bytes(&self, haystack: &[u8]) -> usize {
        match self.needle.as_slice() {
            [] => 0,
            [byte] => memchr_iter(*byte, haystack).count(),
            needle => {
                if is_plain_literal(&self.pattern) {
                    count_literal(haystack, needle)
                } else {
                    AhoCorasick::new([self.pattern.as_str()])
                        .map(|ac| ac.find_iter(haystack).count())
                        .unwrap_or(0)
                }
            }
        }
    }

    pub fn find_in_buffer(&self, haystack: &[u8]) -> Vec<usize> {
        search_in_bytes(haystack, &self.pattern)
    }

    pub fn find_in_buffer_parallel(&self, haystack: &[u8]) -> Vec<usize> {
        self.find_in_buffer(haystack)
    }
}

pub fn search_in_bytes(haystack: &[u8], pattern: &str) -> Vec<usize> {
    if pattern.is_empty() {
        return Vec::new();
    }

    if is_plain_literal(pattern) {
        let finder = memmem::Finder::new(pattern.as_bytes());
        finder.find_iter(haystack).collect()
    } else {
        AhoCorasick::new([pattern])
            .map(|ac| ac.find_iter(haystack).map(|m| m.start()).collect())
            .unwrap_or_default()
    }
}

fn count_literal(haystack: &[u8], needle: &[u8]) -> usize {
    if haystack.len() < PARALLEL_SEARCH_THRESHOLD || needle.len() <= 1 {
        let finder = memmem::Finder::new(needle);
        return finder.find_iter(haystack).count();
    }

    let overlap = needle.len() - 1;
    let chunks = haystack.len().div_ceil(PARALLEL_CHUNK_SIZE);
    let finder = memmem::Finder::new(needle);

    (0..chunks)
        .into_par_iter()
        .map(|chunk| {
            let start = chunk * PARALLEL_CHUNK_SIZE;
            let end = ((chunk + 1) * PARALLEL_CHUNK_SIZE).min(haystack.len());
            let scan_end = (end + overlap).min(haystack.len());
            finder
                .find_iter(&haystack[start..scan_end])
                .filter(|idx| start + *idx < end)
                .count()
        })
        .sum()
}

fn is_plain_literal(pattern: &str) -> bool {
    !pattern
        .chars()
        .any(|ch| matches!(ch, '*' | '?' | '[' | '(' | '|' | '+' | '.' | '^' | '$'))
}
