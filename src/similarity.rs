//! # similarity.rs
//!
//! Módulo para comparar el contenido de documentos y detectar similitudes.
//!
//! ## ¿Cómo funciona?
//!
//! El análisis usa un **Score Compuesto** que combina tres métricas:
//!
//! 1. **Vocabulario compartido (50%)**: Qué tan parecido es el vocabulario entre documentos.
//!    Si dos tesis hablan de los mismos temas, sus palabras clave se van a repetir.
//!    → Usa Coeficiente de Jaccard sobre el conjunto de palabras únicas.
//!
//! 2. **Frases en común (40%)**: Si además de las mismas palabras, aparecen seguidas
//!    en el mismo orden (bigramas = pares de palabras).
//!    → Usa Coeficiente de Jaccard sobre bigramas (shingles de 2 palabras).
//!
//! 3. **Proporción de longitud (10%)**: Penaliza documentos de tamaños muy distintos,
//!    ya que un texto de 5 palabras siempre tendrá "similitud" con uno de 5000.
//!
//! El resultado final es un porcentaje del 0% al 100% que refleja qué tan parecidos
//! son dos documentos en **contenido real**, no solo palabras sueltas.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use crate::models::FileInfo;

// ─────────────────────────────────────────────────────────────
// ESTRUCTURAS DE DATOS
// ─────────────────────────────────────────────────────────────

/// Contiene el resultado de comparar dos documentos por su contenido.
#[derive(Debug, Clone)]
pub struct SimilarityMatch {
    /// Archivo que fue usado como referencia base en la comparación.
    /// Se guarda para permitir futuras extensiones (reportes, exportación, etc.)
    #[allow(dead_code)]
    pub target_file: FileInfo,
    pub candidate_file: FileInfo,
    /// Porcentaje de similitud compuesto (0.0 a 100.0).
    pub similarity_percentage: f64,
    /// Número de palabras clave únicas que ambos documentos comparten.
    pub shared_keyword_count: usize,
    /// Número de bigramas (pares de palabras) que ambos comparten.
    pub shared_bigram_count: usize,
}

// ─────────────────────────────────────────────────────────────
// EXTRACCIÓN DE TEXTO
// ─────────────────────────────────────────────────────────────

/// Extrae el contenido de texto de un archivo según su tipo.
///
/// - **PDF**: usa la librería `pdf-extract` para parsear el contenido interno.
/// - **Cualquier otro formato de texto** (txt, md, rs, json, etc.): lectura directa.
///
/// Retorna `None` si el archivo no se puede leer o está vacío.
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
// PROCESAMIENTO DE TEXTO
// ─────────────────────────────────────────────────────────────

/// Lista de palabras comunes (stopwords) en español e inglés que no aportan
/// significado temático y se deben excluir del análisis.
///
/// Esto mejora la precisión: si dos documentos comparten "de", "el", "la", "the",
/// eso no los hace similares en contenido.
const STOPWORDS: &[&str] = &[
    // Español
    "de", "el", "la", "los", "las", "un", "una", "unos", "unas",
    "en", "con", "por", "para", "del", "al", "se", "lo", "le",
    "que", "es", "su", "son", "nos", "ha", "han", "si", "ya",
    "pero", "como", "mas", "yo", "tu", "el", "no", "a", "e",
    "o", "y", "ni", "se", "te", "me", "mi", "ti", "so",
    // Inglés
    "the", "a", "an", "is", "are", "was", "were", "be", "been",
    "being", "have", "has", "had", "do", "does", "did", "will",
    "would", "should", "could", "may", "might", "shall", "can",
    "to", "of", "in", "for", "on", "with", "at", "by", "from",
    "this", "that", "these", "those", "it", "its", "he", "she",
    "they", "we", "you", "i", "and", "or", "but", "not", "so",
    "if", "as", "up", "all", "any", "each", "both", "few", "more",
];

/// Convierte un texto a una lista de palabras limpias y relevantes:
///
/// 1. Divide por espacios y signos de puntuación.
/// 2. Convierte todo a minúsculas.
/// 3. Elimina caracteres que no sean letras o números.
/// 4. Descarta palabras de una sola letra y stopwords comunes.
///
/// Nota de rendimiento: el HashSet de stopwords se crea en cada llamada.
/// Si en el futuro se comparan miles de archivos, considera moverlo a un
/// `OnceLock<HashSet>` estático para construirlo solo una vez.
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

/// Genera un conjunto de n-gramas (frases de `n` palabras consecutivas)
/// a partir de una lista de tokens.
///
/// ### Ejemplo (n=2, bigramas):
/// `["inteligencia", "artificial", "aplicada"]`
/// → `{"inteligencia artificial", "artificial aplicada"}`
///
/// Los bigramas permiten detectar no solo palabras compartidas,
/// sino también si aparecen en el mismo contexto.
pub fn create_shingles(words: &[String], n: usize) -> HashSet<String> {
    if words.len() < n {
        // Si hay menos palabras que el tamaño del n-grama,
        // usamos las palabras individuales para no quedarnos sin datos.
        return words.iter().cloned().collect();
    }

    let mut shingles = HashSet::new();
    for window in words.windows(n) {
        shingles.insert(window.join(" "));
    }
    shingles
}

// ─────────────────────────────────────────────────────────────
// CÁLCULO DE SIMILITUD
// ─────────────────────────────────────────────────────────────

/// Calcula el **Coeficiente de Jaccard** entre dos conjuntos.
///
/// La fórmula es: `|A ∩ B| / |A ∪ B|`
/// Es decir: elementos en común / total de elementos únicos.
///
/// Resultado: un valor entre 0.0% (nada en común) y 100.0% (idénticos).
/// También retorna la cantidad de elementos compartidos.
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

/// Calcula el **Score de Similitud Compuesto** entre dos documentos
/// ya procesados (representados por sus conjuntos de palabras y bigramas).
///
/// ### Ponderación:
/// - 50% → Similitud de vocabulario (palabras únicas compartidas)
/// - 40% → Similitud de frases/bigramas (contexto compartido)
/// - 10% → Penalización por diferencia de longitud extrema
///
/// ### ¿Por qué esta combinación?
/// La similitud solo por vocabulario puede ser engañosa: dos documentos
/// pueden compartir palabras clave sin ser similares. Añadir bigramas
/// requiere que las palabras aparezcan juntas, lo que indica estructura
/// y contenido realmente compartidos.
fn calculate_composite_score(
    words_a: &HashSet<String>,
    words_b: &HashSet<String>,
    bigrams_a: &HashSet<String>,
    bigrams_b: &HashSet<String>,
) -> (f64, usize, usize) {
    let (vocab_sim, shared_words) = calculate_jaccard_similarity(words_a, words_b);
    let (bigram_sim, shared_bigrams) = calculate_jaccard_similarity(bigrams_a, bigrams_b);

    // Penalización por longitud: si un documento es mucho más largo que el otro,
    // reducimos levemente el score. Se calcula como la proporción del más corto
    // respecto al más largo (resultado entre 0.0 y 1.0).
    let len_a = words_a.len() as f64;
    let len_b = words_b.len() as f64;
    let length_ratio = if len_a == 0.0 || len_b == 0.0 {
        0.0
    } else {
        len_a.min(len_b) / len_a.max(len_b)
    };
    let length_penalty_score = length_ratio * 100.0;

    // Ponderación final
    let composite = (vocab_sim * 0.50) + (bigram_sim * 0.40) + (length_penalty_score * 0.10);

    (composite, shared_words, shared_bigrams)
}

// ─────────────────────────────────────────────────────────────
// FUNCIÓN PRINCIPAL DE COMPARACIÓN
// ─────────────────────────────────────────────────────────────

/// Compara un documento base contra una lista de candidatos y retorna
/// aquellos que superen el umbral mínimo de similitud.
///
/// ### Proceso:
/// 1. Extrae y tokeniza el texto del documento base.
/// 2. Para cada candidato, extrae su texto y calcula el score compuesto.
/// 3. Filtra los que no alcanzan el umbral y ordena los resultados.
///
/// ### Parámetros:
/// - `target_file`: El documento que se usa como referencia.
/// - `candidates`: Todos los archivos contra los cuales comparar.
/// - `min_threshold_percentage`: Porcentaje mínimo para incluir en resultados (0-100).
pub fn find_similar_documents(
    target_file: &FileInfo,
    candidates: &[FileInfo],
    min_threshold_percentage: f64,
) -> Vec<SimilarityMatch> {
    // Extraer y tokenizar el texto del archivo base
    let target_text = match extract_text_from_file(&target_file.path) {
        Some(txt) if !txt.trim().is_empty() => txt,
        _ => {
            println!("No se pudo extraer texto del archivo base. Puede estar vacio o ser binario.");
            return Vec::new();
        }
    };

    let target_words_vec = tokenize_words(&target_text);
    let target_word_set: HashSet<String> = target_words_vec.iter().cloned().collect();
    let target_bigrams = create_shingles(&target_words_vec, 2);

    if target_word_set.is_empty() {
        println!("El archivo base no contiene palabras analizables tras la limpieza.");
        return Vec::new();
    }

    let mut matches = Vec::new();

    for candidate in candidates {
        // No comparar el archivo consigo mismo
        if candidate.path == target_file.path {
            continue;
        }

        // Solo procesar archivos de texto o PDF
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

    // Ordenar de mayor a menor similitud
    matches.sort_by(|a, b| {
        b.similarity_percentage
            .partial_cmp(&a.similarity_percentage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    matches
}
