use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use sha2::{Digest, Sha256};
use crate::models::FileInfo;

/// Representa un grupo de archivos duplicados verificados.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// Hash SHA-256 compartido por todos los archivos del grupo.
    /// Aunque no se muestra directamente en la UI, documenta el origen
    /// del agrupamiento y es útil para futuras extensiones (exportar reporte, etc.).
    #[allow(dead_code)]
    pub hash: String,
    pub file_size: u64,
    /// El archivo que se conservará (generalmente el más antiguo o el primero de la lista).
    pub original: FileInfo,
    /// Los archivos redundantes que son candidatos a enviarse a la papelera.
    pub duplicates: Vec<FileInfo>,
}

/// Retorna los `n` archivos más pesados de la lista, ordenados de mayor a menor tamaño.
pub fn get_top_largest(files: &[FileInfo], n: usize) -> Vec<FileInfo> {
    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    sorted.into_iter().take(n).collect()
}

/// Filtra los archivos que coincidan con una extensión específica (ej: "pdf", "png").
pub fn filter_by_extension(files: &[FileInfo], ext: &str) -> Vec<FileInfo> {
    let clean_ext = ext.trim().trim_start_matches('.').to_lowercase();
    files
        .iter()
        .filter(|f| {
            f.extension
                .as_ref()
                .map(|e| e == &clean_ext)
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Calcula el hash criptográfico SHA-256 del contenido de un archivo.
pub fn calculate_sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192]; // Buffer de 8 KB para lectura eficiente en chunks

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Encuentra duplicados reales:
/// 1. Primero agrupa por tamaño en bytes (filtro rápido de costo cero).
/// 2. Para los grupos con más de 1 archivo, calcula su SHA-256 para verificar coincidencia exacta al 100%.
pub fn find_exact_duplicates(files: &[FileInfo]) -> Vec<DuplicateGroup> {
    // Paso 1: Agrupar por tamaño
    let mut by_size: HashMap<u64, Vec<FileInfo>> = HashMap::new();
    for file in files {
        if file.size_bytes > 0 {
            by_size.entry(file.size_bytes).or_default().push(file.clone());
        }
    }

    let candidate_groups: Vec<Vec<FileInfo>> = by_size
        .into_values()
        .filter(|list| list.len() > 1)
        .collect();

    // Paso 2: Agrupar por hash SHA-256 dentro de cada grupo de mismo tamaño
    let mut duplicate_groups = Vec::new();

    for candidates in candidate_groups {
        let mut by_hash: HashMap<String, Vec<FileInfo>> = HashMap::new();

        for file in candidates {
            if let Ok(hash) = calculate_sha256(&file.path) {
                by_hash.entry(hash).or_default().push(file);
            }
        }

        for (hash, matching_files) in by_hash {
            if matching_files.len() > 1 {
                let file_size = matching_files[0].size_bytes;
                let original = matching_files[0].clone();
                let duplicates = matching_files[1..].to_vec();

                duplicate_groups.push(DuplicateGroup {
                    hash,
                    file_size,
                    original,
                    duplicates,
                });
            }
        }
    }

    // Ordenamos de mayor a menor desperdicio de espacio
    duplicate_groups.sort_by(|a, b| {
        let wasted_a = a.file_size * a.duplicates.len() as u64;
        let wasted_b = b.file_size * b.duplicates.len() as u64;
        wasted_b.cmp(&wasted_a)
    });

    duplicate_groups
}

/// Mueve una lista de archivos a la Papelera de reciclaje del sistema operativo de forma segura.
pub fn send_files_to_trash(files: &[FileInfo]) -> Result<usize, String> {
    let mut deleted_count = 0;

    for file in files {
        match trash::delete(&file.path) {
            Ok(_) => {
                deleted_count += 1;
            }
            Err(err) => {
                return Err(format!(
                    "Error al mover '{}' a la papelera: {}",
                    file.name(),
                    err
                ));
            }
        }
    }

    Ok(deleted_count)
}

/// Representa un grupo de archivos que comparten el mismo nombre de archivo.
#[derive(Debug, Clone)]
pub struct SameNameGroup {
    /// El nombre base compartido por todos los archivos del grupo (sin extensión, normalizado).
    pub shared_name: String,
    /// Todos los archivos que tienen ese nombre, en distintas ubicaciones.
    pub files: Vec<FileInfo>,
}

/// Busca archivos que tengan el mismo nombre (sin importar su ubicación ni contenido).
///
/// ### ¿Cómo funciona?
/// 1. Agrupa todos los archivos por su nombre completo (con extensión), normalizado a minúsculas.
/// 2. Retorna solo los grupos con 2 o más archivos, ordenados de mayor a menor cantidad de coincidencias.
///
/// ### Diferencia con duplicados exactos:
/// - **Duplicados exactos** (opción 4): mismo contenido verificado por hash SHA-256.
/// - **Mismo nombre** (esta función): mismo nombre de archivo, pueden tener contenido diferente.
///
/// Es útil para detectar versiones distintas de un mismo archivo dispersas en múltiples carpetas.
pub fn find_same_name_files(files: &[FileInfo]) -> Vec<SameNameGroup> {
    let mut by_name: HashMap<String, Vec<FileInfo>> = HashMap::new();

    for file in files {
        // Normalizamos a minúsculas para que "Tesis.pdf" y "tesis.pdf" sean considerados iguales
        let key = file.name().to_lowercase();
        by_name.entry(key).or_default().push(file.clone());
    }

    let mut groups: Vec<SameNameGroup> = by_name
        .into_iter()
        .filter(|(_, group)| group.len() > 1)
        .map(|(shared_name, files)| SameNameGroup { shared_name, files })
        .collect();

    // Ordenar de mayor a menor cantidad de coincidencias
    groups.sort_by(|a, b| b.files.len().cmp(&a.files.len()));

    groups
}
