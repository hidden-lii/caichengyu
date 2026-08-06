use crate::entity::idiom::{CharData, IdiomInput};
use serde::{Deserialize, Serialize};

const INITIALS: &[&str] = &[
    "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "r",
    "z", "c", "s", "y", "w",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToneChar {
    base: char,
    tone: Option<u8>,
}

fn tone_chars() -> &'static [(&'static str, ToneChar)] {
    &[
        ("ā", ToneChar { base: 'a', tone: Some(1) }),
        ("á", ToneChar { base: 'a', tone: Some(2) }),
        ("ǎ", ToneChar { base: 'a', tone: Some(3) }),
        ("à", ToneChar { base: 'a', tone: Some(4) }),
        ("ē", ToneChar { base: 'e', tone: Some(1) }),
        ("é", ToneChar { base: 'e', tone: Some(2) }),
        ("ě", ToneChar { base: 'e', tone: Some(3) }),
        ("è", ToneChar { base: 'e', tone: Some(4) }),
        ("ī", ToneChar { base: 'i', tone: Some(1) }),
        ("í", ToneChar { base: 'i', tone: Some(2) }),
        ("ǐ", ToneChar { base: 'i', tone: Some(3) }),
        ("ì", ToneChar { base: 'i', tone: Some(4) }),
        ("ō", ToneChar { base: 'o', tone: Some(1) }),
        ("ó", ToneChar { base: 'o', tone: Some(2) }),
        ("ǒ", ToneChar { base: 'o', tone: Some(3) }),
        ("ò", ToneChar { base: 'o', tone: Some(4) }),
        ("ū", ToneChar { base: 'u', tone: Some(1) }),
        ("ú", ToneChar { base: 'u', tone: Some(2) }),
        ("ǔ", ToneChar { base: 'u', tone: Some(3) }),
        ("ù", ToneChar { base: 'u', tone: Some(4) }),
        ("ǖ", ToneChar { base: 'ü', tone: Some(1) }),
        ("ǘ", ToneChar { base: 'ü', tone: Some(2) }),
        ("ǚ", ToneChar { base: 'ü', tone: Some(3) }),
        ("ǜ", ToneChar { base: 'ü', tone: Some(4) }),
    ]
}

fn tone_marks() -> &'static [(&'static str, [&'static str; 5])] {
    &[
        ("a", ["", "ā", "á", "ǎ", "à"]),
        ("e", ["", "ē", "é", "ě", "è"]),
        ("i", ["", "ī", "í", "ǐ", "ì"]),
        ("o", ["", "ō", "ó", "ǒ", "ò"]),
        ("u", ["", "ū", "ú", "ǔ", "ù"]),
        ("v", ["", "ǖ", "ǘ", "ǚ", "ǜ"]),
    ]
}

fn has_marked_vowel(s: &str) -> bool {
    s.chars().any(|c| "āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜ".contains(c))
}

fn tone_vowel_index(base: &str) -> isize {
    let s = base.to_lowercase().replace('v', "ü");
    if let Some(i) = s.find('a') {
        return i as isize;
    }
    if let Some(i) = s.find('e') {
        return i as isize;
    }
    if let Some(i) = s.find("ou") {
        return i as isize;
    }
    if let Some(i) = s.find("iu") {
        return i as isize;
    }
    if let Some(i) = s.find("ui") {
        return i as isize;
    }
    let chars: Vec<char> = s.chars().collect();
    for i in (0..chars.len()).rev() {
        let ch = chars[i];
        if "aeiouü".contains(ch) {
            return i as isize;
        }
    }
    -1
}

fn mark_syllable(syl: &str) -> String {
    let raw = syl.trim();
    if raw.is_empty() || has_marked_vowel(raw) {
        return raw.to_string();
    }
    let lower = raw.to_lowercase();
    let re = regex_like_digit_tone(&lower);
    if let Some((base, tone)) = re {
        if tone == 5 {
            return base.replace('v', "ü");
        }
        let idx = tone_vowel_index(&base);
        if idx < 0 {
            return base.replace('v', "ü");
        }
        let chars: Vec<char> = base.chars().collect();
        let ch = chars[idx as usize];
        let key = if ch == 'ü' { 'v' } else { ch };
        if let Some((_, marks)) = tone_marks().iter().find(|(k, _)| *k == key.to_string()) {
            let marked = marks[tone as usize];
            let mut out = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i as isize == idx {
                    out.push_str(marked);
                } else {
                    out.push(*c);
                }
            }
            return out;
        }
        return base.replace('v', "ü");
    }
    raw.to_string()
}

fn regex_like_digit_tone(lower: &str) -> Option<(String, u8)> {
    if lower.len() < 2 {
        return None;
    }
    let last = lower.chars().last()?;
    if !"12345".contains(last) {
        return None;
    }
    let tone: u8 = last.to_digit(10)? as u8;
    let base = lower[..lower.len() - 1].to_string();
    Some((base, tone))
}

pub fn normalize_pinyin_input(pinyin: &str) -> String {
    pinyin
        .trim()
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(mark_syllable)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn normalize_entry(item: &IdiomInput) -> Option<IdiomInput> {
    let word = item.word.trim().to_string();
    let pinyin = normalize_pinyin_input(&item.pinyin);
    if word.is_empty() || pinyin.is_empty() {
        return None;
    }
    Some(IdiomInput {
        word,
        pinyin,
        explanation: item.explanation.clone(),
    })
}

fn strip_tone(syl: &str) -> (String, i32) {
    let mut s = syl.to_lowercase().replace('ü', "v");
    let mut tone = 0i32;

    if let Some(last) = s.chars().last() {
        if "12345".contains(last) {
            tone = last.to_digit(10).unwrap_or(5) as i32;
            s.pop();
        }
    }

    for (marked, tc) in tone_chars() {
        if s.contains(marked) {
            s = s.replace(marked, &tc.base.to_string());
            if let Some(t) = tc.tone {
                tone = t as i32;
            }
            break;
        }
    }

    if tone == 0 {
        tone = 5;
    }

    (s, tone)
}

fn split_initial(py: &str) -> (String, String) {
    let lower = py.to_lowercase();
    let mut initials: Vec<&str> = INITIALS.to_vec();
    initials.sort_by_key(|b| std::cmp::Reverse(b.len()));
    for init in initials {
        if lower.starts_with(init) {
            return (init.to_string(), lower[init.len()..].to_string());
        }
    }
    (String::new(), lower)
}

pub fn parse_syllable(syl: &str) -> CharData {
    let (base, tone) = strip_tone(syl.trim());
    let (sm, ym) = split_initial(&base);
    CharData {
        c: String::new(),
        py: base,
        sm,
        ym,
        tone,
    }
}

pub fn build_chars(word: &str, pinyin: &str) -> Option<Vec<CharData>> {
    let chars: Vec<char> = word.chars().collect();
    let syls: Vec<&str> = pinyin.split_whitespace().filter(|s| !s.is_empty()).collect();
    if chars.len() != syls.len() {
        return None;
    }
    Some(
        chars
            .iter()
            .zip(syls.iter())
            .map(|(c, syl)| {
                let mut parsed = parse_syllable(syl);
                parsed.c = c.to_string();
                parsed
            })
            .collect(),
    )
}

fn all_distinct<T: Eq + std::hash::Hash + Clone>(values: &[T]) -> bool {
    let set: std::collections::HashSet<_> = values.iter().cloned().collect();
    set.len() == values.len()
}

pub fn is_blind_eligible(chars: &[CharData]) -> bool {
    if chars.is_empty() {
        return false;
    }
    all_distinct(&chars.iter().map(|c| c.c.clone()).collect::<Vec<_>>())
        && all_distinct(&chars.iter().map(|c| c.sm.clone()).collect::<Vec<_>>())
        && all_distinct(&chars.iter().map(|c| c.ym.clone()).collect::<Vec<_>>())
        && all_distinct(&chars.iter().map(|c| c.tone).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dan4() {
        let ch = parse_syllable("dan4");
        assert_eq!(ch.sm, "d");
        assert_eq!(ch.ym, "an");
        assert_eq!(ch.tone, 4);
    }

    #[test]
    fn normalize_marked_and_digit() {
        assert_eq!(
            normalize_pinyin_input("xián zhāng chě jù"),
            "xián zhāng chě jù"
        );
        assert_eq!(
            normalize_pinyin_input("xian2 zhang1 che3 ju4"),
            "xián zhāng chě jù"
        );
    }
}
