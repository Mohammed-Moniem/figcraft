//! Validate bounded browser UTF-16 without silently replacing unmatched surrogates.
pub fn well_formed(units: impl IntoIterator<Item = u16>) -> bool {
    let mut low = false;
    for unit in units {
        if low {
            if !(0xdc00..=0xdfff).contains(&unit) {
                return false;
            }
            low = false;
        } else if (0xd800..=0xdbff).contains(&unit) {
            low = true;
        } else if (0xdc00..=0xdfff).contains(&unit) {
            return false;
        }
    }
    !low
}
#[cfg(test)]
mod tests {
    #[test]
    fn browser_strings_preserve_valid_unicode_and_refuse_surrogate_aliases() {
        assert!(super::well_formed("نموذج 한글 😀.figcraft".encode_utf16()));
        for bad in [vec![0xd800], vec![0xdc00], vec![0xd800, 65], vec![0xd800, 0xd800, 0xdc00]] {
            assert!(!super::well_formed(bad));
        }
    }
}
