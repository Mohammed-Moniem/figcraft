//! Bounded, network-free English spell checking shared by native and WASM sessions.
//! Wordnik's MIT word list is normalized and compressed locally; see WORDNIK-LICENSE.txt.
use std::sync::OnceLock;

include!("dictionary_meta.rs");
const MAX_WORD: usize = 64;
const MAX_CANDIDATES: usize = 30_000;

fn dictionary() -> Option<&'static [&'static str]> {
    static TEXT: OnceLock<Option<String>> = OnceLock::new();
    static WORDS: OnceLock<Option<Vec<&'static str>>> = OnceLock::new();
    WORDS
        .get_or_init(|| {
            let text = TEXT
                .get_or_init(|| {
                    if DICTIONARY_BYTES == 0 || DICTIONARY_BYTES > 4 * 1024 * 1024 {
                        return None;
                    }
                    let mut bytes = Vec::new();
                    bytes.try_reserve_exact(DICTIONARY_BYTES).ok()?;
                    bytes.resize(DICTIONARY_BYTES, 0);
                    let packed = include_bytes!("english.txt.zlib");
                    let mut decoder = flate2::Decompress::new(true);
                    let status = decoder.decompress(packed, &mut bytes, flate2::FlushDecompress::Finish).ok()?;
                    if status != flate2::Status::StreamEnd
                        || decoder.total_in() != packed.len() as u64
                        || decoder.total_out() != DICTIONARY_BYTES as u64
                    {
                        return None;
                    }
                    String::from_utf8(bytes).ok()
                })
                .as_deref()?;
            let words: Vec<_> = text.lines().collect();
            if words.len() != DICTIONARY_WORDS
                || words.len() > 400_000
                || words.iter().any(|w| w.is_empty() || w.len() > MAX_WORD || !w.bytes().all(|b| b.is_ascii_lowercase()))
                || words.windows(2).any(|p| p[0] >= p[1])
            {
                return None;
            }
            Some(words)
        })
        .as_deref()
}

/// The checked backend is always available offline for English; other scripts are left alone.
pub(super) struct English;
impl super::SpellChecker for English {
    fn backend(&self) -> &'static str {
        "offline"
    }
    fn language(&self) -> String {
        "en".into()
    }
    fn misspelled(&mut self, word: &str) -> bool {
        if word.len() > MAX_WORD || !word.is_ascii() {
            return false;
        }
        let lower = word.to_ascii_lowercase().replace('\u{2019}', "'");
        if [
            "don't",
            "doesn't",
            "isn't",
            "aren't",
            "wasn't",
            "weren't",
            "can't",
            "couldn't",
            "wouldn't",
            "shouldn't",
            "won't",
            "it's",
            "that's",
            "there's",
            "we're",
            "they're",
            "you're",
            "i'm",
            "i've",
            "we've",
            "you've",
            "they've",
            "i'll",
            "we'll",
            "you'll",
            "they'll",
        ]
        .contains(&lower.as_str())
        {
            return false;
        }
        let base = lower.strip_suffix("'s").unwrap_or(&lower);
        dictionary().is_some_and(|words| words.binary_search(&base).is_err())
    }

    fn suggest(&mut self, word: &str) -> Vec<String> {
        if word.len() > MAX_WORD || !word.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Vec::new();
        }
        let word = word.to_ascii_lowercase();
        let Some(words) = dictionary() else { return Vec::new() };
        let first = word.as_bytes().first().copied();
        let mut choices = Vec::new();
        let mut examined = 0;
        // Prioritize the same first letter, then consider initial-letter typos if needed.
        for same_initial in [true, false] {
            for &candidate in words {
                if (candidate.as_bytes().first().copied() == first) != same_initial || candidate.len().abs_diff(word.len()) > 2 {
                    continue;
                }
                if examined >= MAX_CANDIDATES {
                    break;
                }
                examined += 1;
                let score = distance(word.as_bytes(), candidate.as_bytes());
                if score <= 2 {
                    choices.push((score, candidate));
                }
            }
            if choices.iter().any(|(d, _)| *d <= 1) || examined >= MAX_CANDIDATES {
                break;
            }
        }
        choices.sort_unstable();
        choices.dedup();
        choices.into_iter().take(5).map(|(_, word)| word.to_owned()).collect()
    }
}

/// Levenshtein distance with adjacent transpositions, bounded by the word limit.
fn distance(a: &[u8], b: &[u8]) -> usize {
    if a.len() > MAX_WORD || b.len() > MAX_WORD {
        return usize::MAX;
    }
    let mut previous_previous = [0usize; MAX_WORD + 1];
    let mut previous = [0usize; MAX_WORD + 1];
    for (j, v) in previous.iter_mut().enumerate() {
        *v = j;
    }
    for (i, &x) in a.iter().enumerate() {
        let mut current = [0usize; MAX_WORD + 1];
        current[0] = i + 1;
        for (j, &y) in b.iter().enumerate() {
            let k = j + 1;
            current[k] = (previous[k] + 1).min(current[j] + 1).min(previous[j] + usize::from(x != y));
            if i > 0 && j > 0 && x == b[j - 1] && a[i - 1] == y {
                current[k] = current[k].min(previous_previous[j - 1] + 1);
            }
        }
        previous_previous = previous;
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spelling::SpellChecker;
    #[test]
    fn real_offline_dictionary_checks_and_suggests_without_platform_services() {
        let words = dictionary().expect("bundled dictionary must decode");
        assert!(words.len() > 150_000);
        let mut checker = English;
        for word in ["sentence", "design", "components", "hello", "don't"] {
            assert!(!checker.misspelled(word), "{word}");
        }
        for word in ["sentense", "mispeled"] {
            assert!(checker.misspelled(word), "{word}");
        }
        assert!(checker.suggest("sentense").iter().any(|s| s == "sentence"));
        assert!(!checker.misspelled("مرحبا"));
        assert!(checker.suggest(&"a".repeat(1000)).is_empty());
        assert_eq!(distance(b"form", b"from"), 1);
        assert!(std::ptr::eq(words, dictionary().unwrap()));
    }
}
