//! Native macOS spelling through the existing safe objc2 AppKit API.
//!
//! Detection examines one bounded word once; suggestions are requested lazily.
//! No learned-word or system language settings are changed by this adapter.

use figcraft_engine::spelling::SpellChecker;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSRange, NSString, NSThread};

pub struct SystemSpelling;

fn bounded(word: &str) -> bool {
    !word.is_empty() && word.len() <= 256 && word.chars().count() <= 64
}

impl SpellChecker for SystemSpelling {
    fn backend(&self) -> &'static str {
        "macos"
    }

    fn available(&self) -> bool {
        NSThread::isMainThread_class()
    }

    fn language(&self) -> String {
        if !self.available() {
            return "und".into();
        }
        NSSpellChecker::sharedSpellChecker().language().to_string()
    }

    fn misspelled(&mut self, word: &str) -> bool {
        if !self.available() || !bounded(word) {
            return false;
        }
        // This convenience API can wrap, but there is only one word and one
        // call. No iterative scan can revisit the same misspelling forever.
        NSSpellChecker::sharedSpellChecker().checkSpellingOfString_startingAt(&NSString::from_str(word), 0).length > 0
    }

    fn suggest(&mut self, word: &str) -> Vec<String> {
        if !self.available() || !bounded(word) {
            return Vec::new();
        }
        let checker = NSSpellChecker::sharedSpellChecker();
        let language = checker.language();
        checker
            .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                NSRange::new(0, word.encode_utf16().count()),
                &NSString::from_str(word),
                Some(&language),
                0,
            )
            .map(|guesses| guesses.iter().take(5).map(|word| word.to_string()).filter(|word| word.len() <= 256).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_adapter_never_calls_appkit_from_a_worker_or_for_unbounded_words() {
        assert!(!bounded(""));
        assert!(!bounded(&"a".repeat(65)));
        assert!(bounded("Привет"));
        assert!(bounded("don't"));
        // Rust test functions run on worker threads, not the native UI thread.
        let mut checker = SystemSpelling;
        assert!(!checker.available());
        assert!(!checker.misspelled("sentense"));
        assert!(checker.suggest("sentense").is_empty());
        assert_eq!(checker.backend(), "macos");
        assert_eq!(checker.language(), "und");
    }
}
