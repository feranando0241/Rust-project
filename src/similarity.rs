//! # similarity.rs
//!
//! Module for comparing document content and detecting similarities.
//!
//! ## How it works
//!
//! The analysis uses a **Composite Score** that combines three metrics:
//!
//! 1. **Shared vocabulary (50%)**: How similar the vocabulary is between documents.
//!    If two papers cover the same topics, their keywords will repeat.
//!    → Uses Jaccard coefficient over the set of unique words.
//!
//! 2. **Common phrases (40%)**: Whether the same words also appear consecutively
//!    in the same order (bigrams = pairs of words).
//!    → Uses Jaccard coefficient over bigrams (2-word shingles).
//!
//! 3. **Length ratio (10%)**: Penalizes documents of very different sizes,
//!    since a 5-word text will always share "similarity" with a 5000-word one.
//!
//! The final result is a percentage from 0% to 100% reflecting how similar
//! two documents are in **real content**, not just isolated words.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use crate::models::FileInfo;

// ─────────────────────────────────────────────────────────────
// DATA STRUCTURES
// ─────────────────────────────────────────────────────────────

/// Contains the result of comparing two documents by their content.
#[derive(Debug, Clone)]
pub struct SimilarityMatch {
    /// The file used as the base reference in the comparison.
    /// Stored to support future extensions (report export, etc.).
    #[allow(dead_code)]
    pub target_file: FileInfo,
    pub candidate_file: FileInfo,
    /// Composite similarity percentage (0.0 to 100.0).
    pub similarity_percentage: f64,
    /// Number of unique keywords both documents share.
    pub shared_keyword_count: usize,
    /// Number of bigrams (word pairs) both documents share.
    pub shared_bigram_count: usize,
}

// ─────────────────────────────────────────────────────────────
// TEXT EXTRACTION
// ─────────────────────────────────────────────────────────────

/// Extracts text content from a file based on its type.
///
/// - **PDF**: uses the `pdf-extract` crate to parse internal content.
/// - **Any other text format** (txt, md, rs, json, etc.): direct read.
///
/// Returns `None` if the file cannot be read or is empty.
pub fn extract_text_from_file(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if extension == "pdf" {
        pdf_extract::extract_text(path).ok()
    } else {
        fs::read_to_string(path).ok()
    }
}

// ─────────────────────────────────────────────────────────────
// TEXT PROCESSING
// ─────────────────────────────────────────────────────────────

/// Common words (stopwords) in Spanish and English that carry no thematic
/// meaning and should be excluded from the analysis.
///
/// This improves accuracy: if two documents share "de", "el", "la", "the",
/// that does not make them similar in content.
const STOPWORDS: &[&str] = &[
    // Spanish
    "de", "el", "la", "los", "las", "un", "una", "unos", "unas",
    "en", "con", "por", "para", "del", "al", "se", "lo", "le",
    "que", "es", "su", "son", "nos", "ha", "han", "si", "ya",
    "pero", "como", "mas", "yo", "tu", "el", "no", "a", "e",
    "o", "y", "ni", "se", "te", "me", "mi", "ti", "so",
    // English
    "the", "a", "an", "is", "are", "was", "were", "be", "been",
    "being", "have", "has", "had", "do", "does", "did", "will",
    "would", "should", "could", "may", "might", "shall", "can",
    "to", "of", "in", "for", "on", "with", "at", "by", "from",
    "this", "that", "these", "those", "it", "its", "he", "she",
    "they", "we", "you", "i", "and", "or", "but", "not", "so",
    "if", "as", "up", "all", "any", "each", "both", "few", "more",
];

/// Converts text into a list of clean, meaningful words:
///
/// 1. Splits on spaces and punctuation.
/// 2. Converts everything to lowercase.
/// 3. Removes non-alphanumeric characters.
/// 4. Discards single-letter words and common stopwords.
///
/// Performance note: the stopwords HashSet is built on every call.
/// If comparing thousands of files in the future, consider moving it to a
/// static `OnceLock<HashSet>` so it is built only once.
pub fn tokenize_words(text: &str) -> Vec<String> {
    let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();

    text.split(|c: char| !c.is_alphanumeric())
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| w.len() > 2 && !stopwords.contains(w.as_str()))
        .collect()
}

/// Generates a set of n-grams (phrases of `n` consecutive words)
/// from a token list.
///
/// ### Example (n=2, bigrams):
/// `["intelligence", "artificial", "applied"]`
/// → `{"intelligence artificial", "artificial applied"}`
///
/// Bigrams detect not just shared words, but whether they appear
/// in the same context (adjacent order).
pub fn create_shingles(words: &[String], n: usize) -> HashSet<String> {
    if words.len() < n {
        // Fewer words than the n-gram size: fall back to individual words
        // so we don't return an empty set.
        return words.iter().cloned().collect();
    }

    let mut shingles = HashSet::new();
    for window in words.windows(n) {
        shingles.insert(window.join(" "));
    }
    shingles
}

// ─────────────────────────────────────────────────────────────
// SIMILARITY CALCULATION
// ─────────────────────────────────────────────────────────────

/// Calculates the **Jaccard Coefficient** between two sets.
///
/// Formula: `|A ∩ B| / |A ∪ B|`
/// That is: elements in common / total unique elements.
///
/// Result: a value between 0.0% (nothing in common) and 100.0% (identical).
/// Also returns the count of shared elements.
pub fn calculate_jaccard_similarity(set_a: &HashSet<String>, set_b: &HashSet<String>) -> (f64, usize) {
    if set_a.is_empty() && set_b.is_empty() {
        return (100.0, 0);
    }
    if set_a.is_empty() || set_b.is_empty() {
        return (0.0, 0);
    }

    let intersection_count = set_a.intersection(set_b).count();
    let union_count = set_a.union(set_b).count();

    if union_count == 0 {
        return (0.0, 0);
    }

    let similarity = (intersection_count as f64 / union_count as f64) * 100.0;
    (similarity, intersection_count)
}

/// Calculates the **Composite Similarity Score** between two already-processed
/// documents (represented by their word and bigram sets).
///
/// ### Weighting:
/// - 50% → Vocabulary similarity (shared unique words)
/// - 40% → Phrase/bigram similarity (shared context)
/// - 10% → Length-difference penalty
///
/// ### Why this combination?
/// Vocabulary-only similarity can be misleading: two documents may share
/// keywords without being truly similar. Adding bigrams requires words to
/// appear together, which indicates genuinely shared structure and content.
fn calculate_composite_score(
    words_a: &HashSet<String>,
    words_b: &HashSet<String>,
    bigrams_a: &HashSet<String>,
    bigrams_b: &HashSet<String>,
) -> (f64, usize, usize) {
    let (vocab_sim, shared_words) = calculate_jaccard_similarity(words_a, words_b);
    let (bigram_sim, shared_bigrams) = calculate_jaccard_similarity(bigrams_a, bigrams_b);

    // Length penalty: if one document is much longer than the other,
    // slightly reduce the score. Computed as the ratio of the shorter
    // to the longer document (result between 0.0 and 1.0).
    let len_a = words_a.len() as f64;
    let len_b = words_b.len() as f64;
    let length_ratio = if len_a == 0.0 || len_b == 0.0 {
        0.0
    } else {
        len_a.min(len_b) / len_a.max(len_b)
    };
    let length_penalty_score = length_ratio * 100.0;

    // Final weighted score
    let composite = (vocab_sim * 0.50) + (bigram_sim * 0.40) + (length_penalty_score * 0.10);

    (composite, shared_words, shared_bigrams)
}

// ─────────────────────────────────────────────────────────────
// MAIN COMPARISON FUNCTION
// ─────────────────────────────────────────────────────────────

/// Compares a base document against a list of candidates and returns
/// those that exceed the minimum similarity threshold.
///
/// ### Process:
/// 1. Extracts and tokenizes the text of the base document.
/// 2. For each candidate, extracts its text and computes the composite score.
/// 3. Filters out those below the threshold and sorts the results.
///
/// ### Parameters:
/// - `target_file`: The document used as the reference.
/// - `candidates`: All files to compare against.
/// - `min_threshold_percentage`: Minimum percentage to include in results (0-100).
pub fn find_similar_documents(
    target_file: &FileInfo,
    candidates: &[FileInfo],
    min_threshold_percentage: f64,
) -> Vec<SimilarityMatch> {
    // Extract and tokenize the base file's text
    let target_text = match extract_text_from_file(&target_file.path) {
        Some(txt) if !txt.trim().is_empty() => txt,
        _ => {
            println!("Could not extract text from the base file. It may be empty or binary.");
            return Vec::new();
        }
    };

    let target_words_vec = tokenize_words(&target_text);
    let target_word_set: HashSet<String> = target_words_vec.iter().cloned().collect();
    let target_bigrams = create_shingles(&target_words_vec, 2);

    if target_word_set.is_empty() {
        println!("The base file contains no analyzable words after cleaning.");
        return Vec::new();
    }

    let mut matches = Vec::new();

    for candidate in candidates {
        // Skip comparing the file against itself
        if candidate.path == target_file.path {
            continue;
        }

        // Only process text or PDF files
        let ext = candidate.extension.as_deref().unwrap_or("").to_lowercase();
        let is_text_file = matches!(
            ext.as_str(),
            "pdf" | "txt" | "md" | "rs" | "json" | "csv" | "py" | "js" | "c" | "cpp" | "docx"
        );
        if !is_text_file {
            continue;
        }

        if let Some(cand_text) = extract_text_from_file(&candidate.path) {
            if cand_text.trim().is_empty() {
                continue;
            }

            let cand_words_vec = tokenize_words(&cand_text);
            let cand_word_set: HashSet<String> = cand_words_vec.iter().cloned().collect();
            let cand_bigrams = create_shingles(&cand_words_vec, 2);

            let (similarity, shared_words, shared_bigrams) = calculate_composite_score(
                &target_word_set,
                &cand_word_set,
                &target_bigrams,
                &cand_bigrams,
            );

            if similarity >= min_threshold_percentage {
                matches.push(SimilarityMatch {
                    target_file: target_file.clone(),
                    candidate_file: candidate.clone(),
                    similarity_percentage: similarity,
                    shared_keyword_count: shared_words,
                    shared_bigram_count: shared_bigrams,
                });
            }
        }
    }

    // Sort from highest to lowest similarity
    matches.sort_by(|a, b| {
        b.similarity_percentage
            .partial_cmp(&a.similarity_percentage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    matches
}
