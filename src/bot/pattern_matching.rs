use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;
use log::debug;
use base64::engine::{Engine, general_purpose};

/// Enhanced pattern matching capabilities that go far beyond NightBot
#[derive(Debug, Clone)]
pub enum AdvancedPattern {
    /// Fuzzy matching with similarity threshold (0.0-1.0)
    FuzzyMatch { pattern: String, threshold: f32 },
    
    /// Phonetic matching using Soundex algorithm
    Phonetic(String),
    
    /// Leetspeak detection and normalization
    Leetspeak(String),
    
    /// Unicode normalization for international characters
    UnicodeNormalized(String),
    
    /// Zalgo text detection (corrupted/glitched text)
    ZalgoText,
    
    /// Homoglyph detection (visually similar characters from different scripts)
    Homoglyph(String),
    
    /// Keyboard layout shift detection (qwerty -> azerty etc.)
    KeyboardShift { pattern: String, layouts: Vec<KeyboardLayout> },
    
    /// Repeated character compression (hellooooo -> hello)
    RepeatedCharCompression(String),
    
    /// Base64/URL encoded content detection
    EncodedContent(String),
}

#[derive(Debug, Clone)]
pub enum KeyboardLayout {
    Qwerty,
    Azerty,
    Qwertz,
    Dvorak,
}

impl AdvancedPattern {
    /// Check if this advanced pattern matches the given text
    pub fn matches(&self, text: &str) -> bool {
        match self {
            AdvancedPattern::FuzzyMatch { pattern, threshold } => {
                Self::fuzzy_match(text, pattern, *threshold)
            }
            AdvancedPattern::Phonetic(pattern) => {
                Self::phonetic_match(text, pattern)
            }
            AdvancedPattern::Leetspeak(pattern) => {
                Self::leetspeak_match(text, pattern)
            }
            AdvancedPattern::UnicodeNormalized(pattern) => {
                Self::unicode_normalized_match(text, pattern)
            }
            AdvancedPattern::ZalgoText => {
                Self::is_zalgo_text(text)
            }
            AdvancedPattern::Homoglyph(pattern) => {
                Self::homoglyph_match(text, pattern)
            }
            AdvancedPattern::KeyboardShift { pattern, layouts } => {
                Self::keyboard_shift_match(text, pattern, layouts)
            }
            AdvancedPattern::RepeatedCharCompression(pattern) => {
                Self::repeated_char_match(text, pattern)
            }
            AdvancedPattern::EncodedContent(pattern) => {
                Self::encoded_content_match(text, pattern)
            }
        }
    }

    /// Fuzzy string matching using Levenshtein distance
    fn fuzzy_match(text: &str, pattern: &str, threshold: f32) -> bool {
        let text_lower = text.to_lowercase();
        let pattern_lower = pattern.to_lowercase();
        
        // Split into words and check each word
        for word in text_lower.split_whitespace() {
            let similarity = Self::calculate_similarity(word, &pattern_lower);
            if similarity >= threshold {
                debug!("Fuzzy match found: '{}' ~= '{}' (similarity: {:.2})", word, pattern, similarity);
                return true;
            }
        }
        false
    }

    /// Calculate similarity between two strings (0.0 = no match, 1.0 = exact match)
    fn calculate_similarity(s1: &str, s2: &str) -> f32 {
        if s1 == s2 {
            return 1.0;
        }
        
        let max_len = s1.len().max(s2.len());
        if max_len == 0 {
            return 1.0;
        }
        
        let distance = Self::levenshtein_distance(s1, s2);
        1.0 - (distance as f32 / max_len as f32)
    }

    /// Calculate Levenshtein distance between two strings
    fn levenshtein_distance(s1: &str, s2: &str) -> usize {
        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();
        let len1 = chars1.len();
        let len2 = chars2.len();

        if len1 == 0 { return len2; }
        if len2 == 0 { return len1; }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }

        matrix[len1][len2]
    }

    /// Phonetic matching using simplified Soundex algorithm
    fn phonetic_match(text: &str, pattern: &str) -> bool {
        let text_lower = text.to_lowercase();
        let pattern_soundex = Self::soundex(pattern);
        
        for word in text_lower.split_whitespace() {
            let word_soundex = Self::soundex(word);
            if word_soundex == pattern_soundex {
                debug!("Phonetic match found: '{}' sounds like '{}'", word, pattern);
                return true;
            }
        }
        false
    }

    /// Simplified Soundex algorithm for phonetic matching
    fn soundex(word: &str) -> String {
        if word.is_empty() {
            return String::new();
        }

        let word = word.to_lowercase();
        let chars: Vec<char> = word.chars().collect();
        let mut soundex = String::new();
        
        // First character
        soundex.push(chars[0].to_uppercase().next().unwrap_or(chars[0]));
        
        let soundex_map: HashMap<char, char> = [
            ('b', '1'), ('f', '1'), ('p', '1'), ('v', '1'),
            ('c', '2'), ('g', '2'), ('j', '2'), ('k', '2'), ('q', '2'), ('s', '2'), ('x', '2'), ('z', '2'),
            ('d', '3'), ('t', '3'),
            ('l', '4'),
            ('m', '5'), ('n', '5'),
            ('r', '6'),
        ].iter().cloned().collect();

        let mut prev_code = None;
        
        for ch in chars.iter().skip(1) {
            if let Some(&code) = soundex_map.get(ch) {
                if prev_code != Some(code) {
                    soundex.push(code);
                    prev_code = Some(code);
                    if soundex.len() >= 4 {
                        break;
                    }
                }
            } else {
                prev_code = None;
            }
        }
        
        // Pad with zeros
        while soundex.len() < 4 {
            soundex.push('0');
        }
        
        soundex.truncate(4);
        soundex
    }

    /// Leetspeak detection and normalization
    fn leetspeak_match(text: &str, pattern: &str) -> bool {
        let normalized_text = Self::normalize_leetspeak(text);
        let normalized_pattern = Self::normalize_leetspeak(pattern);
        
        normalized_text.to_lowercase().contains(&normalized_pattern.to_lowercase())
    }

    /// Convert leetspeak to normal text
    fn normalize_leetspeak(text: &str) -> String {
        let leetspeak_map: HashMap<char, char> = [
            ('0', 'o'), ('1', 'i'), ('3', 'e'), ('4', 'a'), ('5', 's'),
            ('6', 'g'), ('7', 't'), ('8', 'b'), ('9', 'g'),
            ('@', 'a'), ('$', 's'), ('+', 't'), ('!', 'i'),
            ('|', 'l'), ('(', 'c'), (')', 'c'), ('[', 'c'), (']', 'c'),
            ('{', 'c'), ('}', 'c'), ('/', 'l'), ('\\', 'l'),
        ].iter().cloned().collect();

        text.chars()
            .map(|c| leetspeak_map.get(&c.to_lowercase().next().unwrap_or(c)).copied().unwrap_or(c))
            .collect()
    }

    /// Unicode normalization for international characters
    fn unicode_normalized_match(text: &str, pattern: &str) -> bool {
        let normalized_text: String = text.nfd().collect();
        let normalized_pattern: String = pattern.nfd().collect();
        
        // Remove diacritics/accents
        let clean_text = Self::remove_diacritics(&normalized_text);
        let clean_pattern = Self::remove_diacritics(&normalized_pattern);
        
        clean_text.to_lowercase().contains(&clean_pattern.to_lowercase())
    }

    /// Remove diacritical marks from text
    fn remove_diacritics(text: &str) -> String {
        text.chars()
            .filter(|c| !c.is_ascii_punctuation() && !Self::is_combining_mark(*c))
            .collect()
    }

    /// Check if character is a combining mark (diacritic)
    fn is_combining_mark(c: char) -> bool {
        matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
    }

    /// Detect Zalgo text (corrupted/glitched text with excessive combining characters)
    fn is_zalgo_text(text: &str) -> bool {
        let total_chars = text.chars().count();
        if total_chars == 0 {
            return false;
        }
        
        let combining_chars = text.chars().filter(|&c| Self::is_combining_mark(c)).count();
        let ratio = combining_chars as f32 / total_chars as f32;
        
        // If more than 30% of characters are combining marks, likely Zalgo
        ratio > 0.3
    }

    /// Homoglyph detection (visually similar characters from different scripts)
    fn homoglyph_match(text: &str, pattern: &str) -> bool {
        let normalized_text = Self::normalize_homoglyphs(text);
        let normalized_pattern = Self::normalize_homoglyphs(pattern);
        
        normalized_text.to_lowercase().contains(&normalized_pattern.to_lowercase())
    }

    /// Normalize common homoglyphs to ASCII equivalents
    fn normalize_homoglyphs(text: &str) -> String {
        let homoglyph_map: HashMap<char, char> = [
            // Cyrillic lookalikes
            ('а', 'a'), ('е', 'e'), ('о', 'o'), ('р', 'p'), ('с', 'c'),
            ('х', 'x'), ('у', 'y'), ('А', 'A'), ('В', 'B'), ('Е', 'E'),
            ('К', 'K'), ('М', 'M'), ('Н', 'H'), ('О', 'O'), ('Р', 'P'),
            ('С', 'C'), ('Т', 'T'), ('У', 'Y'), ('Х', 'X'),
            
            // Greek lookalikes
            ('α', 'a'), ('ο', 'o'), ('ρ', 'p'), ('υ', 'u'), ('Α', 'A'),
            ('Β', 'B'), ('Ε', 'E'), ('Ζ', 'Z'), ('Η', 'H'), ('Ι', 'I'),
            ('Κ', 'K'), ('Μ', 'M'), ('Ν', 'N'), ('Ο', 'O'), ('Ρ', 'P'),
            ('Τ', 'T'), ('Υ', 'Y'), ('Χ', 'X'),
            
            // Mathematical symbols
            ('𝐀', 'A'), ('𝐁', 'B'), ('𝐂', 'C'), ('𝐃', 'D'), ('𝐄', 'E'),
            ('𝐚', 'a'), ('𝐛', 'b'), ('𝐜', 'c'), ('𝐝', 'd'), ('𝐞', 'e'),
            
            // Other common substitutions
            ('０', '0'), ('１', '1'), ('２', '2'), ('３', '3'), ('４', '4'),
            ('５', '5'), ('６', '6'), ('７', '7'), ('８', '8'), ('９', '9'),
        ].iter().cloned().collect();

        text.chars()
            .map(|c| homoglyph_map.get(&c).copied().unwrap_or(c))
            .collect()
    }

    /// Keyboard layout shift detection
    fn keyboard_shift_match(text: &str, pattern: &str, _layouts: &[KeyboardLayout]) -> bool {
        // Simplified implementation - check common QWERTY shifts
        let shifted_pattern = Self::apply_qwerty_shift(pattern);
        text.to_lowercase().contains(&shifted_pattern.to_lowercase()) ||
        text.to_lowercase().contains(&pattern.to_lowercase())
    }

    /// Apply common QWERTY keyboard shifts
    fn apply_qwerty_shift(text: &str) -> String {
        let shift_map: HashMap<char, char> = [
            ('q', 'a'), ('w', 's'), ('e', 'd'), ('r', 'f'), ('t', 'g'),
            ('y', 'h'), ('u', 'j'), ('i', 'k'), ('o', 'l'), ('p', ';'),
            ('a', 'q'), ('s', 'w'), ('d', 'e'), ('f', 'r'), ('g', 't'),
            ('h', 'y'), ('j', 'u'), ('k', 'i'), ('l', 'o'), (';', 'p'),
        ].iter().cloned().collect();

        text.chars()
            .map(|c| shift_map.get(&c.to_lowercase().next().unwrap_or(c)).copied().unwrap_or(c))
            .collect()
    }

    /// Repeated character compression matching
    fn repeated_char_match(text: &str, pattern: &str) -> bool {
        let compressed_text = Self::compress_repeated_chars(text);
        let compressed_pattern = Self::compress_repeated_chars(pattern);
        
        compressed_text.to_lowercase().contains(&compressed_pattern.to_lowercase())
    }

    /// Compress repeated characters (e.g., "hellooooo" -> "hello")
    fn compress_repeated_chars(text: &str) -> String {
        let mut result = String::new();
        let mut prev_char = None;
        let mut repeat_count = 0;

        for ch in text.chars() {
            match prev_char {
                Some(prev) if prev == ch => {
                    repeat_count += 1;
                    // Allow up to 2 consecutive characters
                    if repeat_count <= 2 {
                        result.push(ch);
                    }
                }
                _ => {
                    result.push(ch);
                    repeat_count = 1;
                }
            }
            prev_char = Some(ch);
        }

        result
    }

    /// Encoded content detection (Base64, URL encoding, etc.)
    fn encoded_content_match(text: &str, pattern: &str) -> bool {
        // Check Base64 patterns
        // base64::decode is deprecated, use general_purpose::STANDARD.decode
        // if let Ok(decoded) = base64::decode(text) {  
        if let Ok(decoded) = general_purpose::STANDARD.decode(text) {
            if let Ok(decoded_str) = String::from_utf8(decoded) {
                if decoded_str.to_lowercase().contains(&pattern.to_lowercase()) {
                    debug!("Base64 encoded content match found: {} -> {}", text, decoded_str);
                    return true;
                }
            }
        }

        // Check URL encoding
        if let Ok(decoded) = urlencoding::decode(text) {
            if decoded.to_lowercase().contains(&pattern.to_lowercase()) {
                debug!("URL encoded content match found: {} -> {}", text, decoded);
                return true;
            }
        }

        false
    }
}

/// Enhanced pattern matching system that combines multiple detection methods
pub struct EnhancedPatternMatcher {
    pub patterns: Vec<AdvancedPattern>,
    effectiveness_stats: HashMap<String, PatternStats>,
}

#[derive(Debug, Clone)]
pub struct PatternStats {
    pub matches: u64,
    pub false_positives: u64,
    pub last_matched: Option<chrono::DateTime<chrono::Utc>>,
    pub effectiveness_score: f32, // 0.0 - 1.0 based on accuracy
}

impl EnhancedPatternMatcher {
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
            effectiveness_stats: HashMap::new(),
        }
    }

    /// Add an advanced pattern to the matcher
    pub fn add_pattern(&mut self, pattern: AdvancedPattern) {
        let pattern_id = format!("{:?}", pattern);
        self.patterns.push(pattern);
        self.effectiveness_stats.insert(pattern_id, PatternStats {
            matches: 0,
            false_positives: 0,
            last_matched: None,
            effectiveness_score: 1.0,
        });
    }

    /// Check if text matches any of the advanced patterns
    pub fn matches(&mut self, text: &str) -> Vec<String> {
        let mut matches = Vec::new();
        
        for (i, pattern) in self.patterns.iter().enumerate() {
            if pattern.matches(text) {
                let pattern_id = format!("{:?}", pattern);
                matches.push(pattern_id.clone());
                
                // Update statistics
                if let Some(stats) = self.effectiveness_stats.get_mut(&pattern_id) {
                    stats.matches += 1;
                    stats.last_matched = Some(chrono::Utc::now());
                }
                
                debug!("Advanced pattern match: {} matched by pattern {}", text, i);
            }
        }
        
        matches
    }

    /// Report a false positive to improve pattern effectiveness
    pub fn report_false_positive(&mut self, pattern_id: &str) {
        if let Some(stats) = self.effectiveness_stats.get_mut(pattern_id) {
            stats.false_positives += 1;
            stats.effectiveness_score = stats.matches as f32 / (stats.matches + stats.false_positives) as f32;
        }
    }

    /// Get effectiveness statistics for all patterns
    pub fn get_effectiveness_stats(&self) -> &HashMap<String, PatternStats> {
        &self.effectiveness_stats
    }

    /// Get patterns with low effectiveness scores (candidates for removal)
    pub fn get_ineffective_patterns(&self, threshold: f32) -> Vec<String> {
        self.effectiveness_stats
            .iter()
            .filter(|(_, stats)| stats.effectiveness_score < threshold && stats.matches > 10)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_matching() {
        let pattern = AdvancedPattern::FuzzyMatch {
            pattern: "badword".to_string(),
            threshold: 0.8,
        };
        
        assert!(pattern.matches("badword")); // Exact match
        assert!(pattern.matches("badwrd")); // Missing character
        assert!(pattern.matches("badwore")); // Character substitution
        assert!(!pattern.matches("goodword")); // Too different
    }

    #[test]
    fn test_leetspeak_detection() {
        let pattern = AdvancedPattern::Leetspeak("badword".to_string());
        
        assert!(pattern.matches("b4dw0rd"));    // 4->a, 0->o
        assert!(pattern.matches("b@dw0rd"));    // @->a, 0->o  
        assert!(pattern.matches("badword"));     // Normal text
        assert!(!pattern.matches("goodword"));  // Different word
    }

    #[test]
    fn test_unicode_normalization() {
        let pattern = AdvancedPattern::UnicodeNormalized("cafe".to_string());
        
        assert!(pattern.matches("café")); // With accent
        assert!(pattern.matches("cafe")); // Without accent
        assert!(pattern.matches("CAFÉ")); // Case insensitive
    }

    #[test]
    fn test_zalgo_detection() {
        let pattern = AdvancedPattern::ZalgoText;
        
        assert!(pattern.matches("h̸̡̪̯ͨ͊̽̅̾̎Ȩ̬̩̾͛ͪ̈́̀́͘ ̶̧̨̱̹̭̯ͧ̾ͬC̷̙̲̝͖ͭ̏ͥͮ͟Oͮ͏̮̪̝͍M̲̖͊̒ͪͩͬ̚̚͜Ȇ̴̟̟͙̞ͩ͌͝S̨̥̫͎̭ͯ̿̔̀ͅ"));
        assert!(!pattern.matches("normal text"));
    }

    #[test]
    fn test_homoglyph_detection() {
        let pattern = AdvancedPattern::Homoglyph("badword".to_string());
        
        assert!(pattern.matches("bаdword")); // Cyrillic 'а' instead of 'a'
        assert!(pattern.matches("badwοrd")); // Greek 'ο' instead of 'o'
        assert!(pattern.matches("badword")); // Normal text
    }

    #[test]
    fn test_repeated_char_compression() {
        let pattern = AdvancedPattern::RepeatedCharCompression("hello".to_string());
        
        assert!(pattern.matches("hellooooo"));
        assert!(pattern.matches("hellllllo"));
        assert!(pattern.matches("hello"));
        assert!(!pattern.matches("goodbye"));
    }

    #[test]
    fn test_phonetic_matching() {
        let pattern = AdvancedPattern::Phonetic("smith".to_string());
        
        assert!(pattern.matches("smyth"));
        assert!(pattern.matches("smith"));
        // Note: Simplified Soundex might not catch all variations
    }
}