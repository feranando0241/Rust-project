use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::actions::{DuplicateGroup, SameNameGroup};
use crate::models::FileInfo;
use crate::similarity::SimilarityMatch;

/// Convierte una cantidad de bytes a una cadena legible (B, KB, MB, GB).
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    match bytes {
        b if b >= GB => format!("{:.2} GB", b as f64 / GB as f64),
        b if b >= MB => format!("{:.2} MB", b as f64 / MB as f64),
        b if b >= KB => format!("{:.2} KB", b as f64 / KB as f64),
        b => format!("{} B", b),
    }
}

/// Agrupa una lista de archivos según su carpeta contenedora.
pub fn group_by_folder(files: &[FileInfo]) -> BTreeMap<PathBuf, Vec<FileInfo>> {
    let mut grouped: BTreeMap<PathBuf, Vec<FileInfo>> = BTreeMap::new();

    for file in files {
        let parent = file
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        grouped.entry(parent).or_default().push(file.clone());
    }

    for file_list in grouped.values_mut() {
        file_list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    }

    grouped
}

/// Muestra los resultados del escaneo organizados en un árbol visual.
pub fn print_tree(root_dir: &Path, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    let grouped = group_by_folder(files);

    println!("\nEscaneo completado para: {}", root_dir.display());
    println!("══════════════════════════════════════════════════════════════════");

    for (folder, folder_files) in &grouped {
        let folder_size: u64 = folder_files.iter().map(|f| f.size_bytes).sum();

        let display_folder = if folder == root_dir {
            "[Raiz]".to_string()
        } else if let Ok(relative) = folder.strip_prefix(root_dir) {
            format!("./{}", relative.display())
        } else {
            format!("{}", folder.display())
        };

        println!(
            "\n{} ({} archivos - {})",
            display_folder,
            folder_files.len(),
            format_size(folder_size)
        );
        println!("  ├─────────────────────────────────────────────────────────────");

        for (index, file) in folder_files.iter().enumerate() {
            let is_last = index == folder_files.len() - 1;
            let branch = if is_last { "  └─" } else { "  ├─" };
            let ext = file.extension_or("sin ext");
            let size = format_size(file.size_bytes);

            println!("{} {:<38} {:>10}  [{}]", branch, file.name(), size, ext);
        }
    }

    println!("\n══════════════════════════════════════════════════════════════════");
    println!(
        "Total general: {} archivo(s) en {} carpeta(s) | Espacio total: {}",
        files.len(),
        grouped.len(),
        format_size(total_size)
    );
    println!("══════════════════════════════════════════════════════════════════\n");
}

/// Muestra la tabla del Top de archivos más pesados.
pub fn print_top_largest(files: &[FileInfo]) {
    println!("\n=== Top Archivos Mas Pesados ===");
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("{:<4} {:<35} {:>10}   {}", "TOP", "NOMBRE", "TAMAÑO", "RUTA");
    println!("──────────────────────────────────────────────────────────────────────────");

    for (i, file) in files.iter().enumerate() {
        println!(
            "#{:<3} {:<35} {:>10}   {}",
            i + 1,
            file.name(),
            format_size(file.size_bytes),
            file.path.display()
        );
    }
    println!("──────────────────────────────────────────────────────────────────────────\n");
}

/// Muestra la lista de archivos filtrados por extensión.
pub fn print_filtered_files(ext: &str, files: &[FileInfo]) {
    let total_size: u64 = files.iter().map(|f| f.size_bytes).sum();
    println!("\n=== Archivos con extension '.{}' ({} encontrados) ===", ext, files.len());
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("{:<38} {:>10}   {}", "NOMBRE", "TAMAÑO", "RUTA");
    println!("──────────────────────────────────────────────────────────────────────────");

    for file in files {
        println!(
            "{:<38} {:>10}   {}",
            file.name(),
            format_size(file.size_bytes),
            file.path.display()
        );
    }
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("Espacio total ocupado por '.{}': {}\n", ext, format_size(total_size));
}

/// Muestra los duplicados exactos verificados por Hash y el espacio a liberar.
pub fn print_exact_duplicates(groups: &[DuplicateGroup]) {
    if groups.is_empty() {
        println!("\nNo se encontraron archivos duplicados exactos.");
        return;
    }

    let mut total_wasted: u64 = 0;
    let mut total_dupe_files: usize = 0;

    println!("\n=== Archivos Duplicados Verificados (Hash SHA-256 Identico) ===");
    println!("══════════════════════════════════════════════════════════════════");

    for (i, group) in groups.iter().enumerate() {
        let wasted_in_group = group.file_size * group.duplicates.len() as u64;
        total_wasted += wasted_in_group;
        total_dupe_files += group.duplicates.len();

        println!(
            "\n[Grupo #{}] Tamaño: {} c/u | Desperdicio: {}",
            i + 1,
            format_size(group.file_size),
            format_size(wasted_in_group)
        );
        println!("  [CONSERVAR] {} ({})", group.original.name(), group.original.path.display());
        for dupe in &group.duplicates {
            println!("  [DUPLICADO] {} ({})", dupe.name(), dupe.path.display());
        }
    }

    println!("\n══════════════════════════════════════════════════════════════════");
    println!(
        "Resumen: {} archivo(s) duplicado(s) encontrados en {} grupo(s)",
        total_dupe_files,
        groups.len()
    );
    println!("Espacio total recuperable enviando a la Papelera: {}", format_size(total_wasted));
    println!("══════════════════════════════════════════════════════════════════\n");
}

/// Muestra la lista interactiva de directorios accesibles.
pub fn print_accessible_directories(parent_dir: &Path, dirs: &[PathBuf]) {
    println!("\n=== Carpetas y Directorios Accesibles en '{}' ===", parent_dir.display());
    println!("──────────────────────────────────────────────────────────────────────────");
    if dirs.is_empty() {
        println!("No hay subcarpetas adicionales en este directorio.");
    } else {
        for (i, dir) in dirs.iter().enumerate() {
            let folder_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("carpeta");
            println!(" [{:>2}] {:<30} ({})", i + 1, folder_name, dir.display());
        }
    }
    println!("──────────────────────────────────────────────────────────────────────────\n");
}

/// Muestra los resultados de la comparación de similitud de contenido.
///
/// Para cada resultado se imprime:
/// - Nombre del documento comparado
/// - Porcentaje de similitud compuesto con barra visual
/// - Cuántas palabras clave y bigramas tienen en común
/// - Ruta completa del archivo
pub fn print_similarity_results(target_name: &str, matches: &[SimilarityMatch], threshold: f64) {
    println!("\n=== Reporte de Similitud para: '{}' ===", target_name);
    println!("    (Umbral mínimo: >= {:.1}% | Score = 50% vocabulario + 40% frases + 10% longitud)", threshold);
    println!("══════════════════════════════════════════════════════════════════════════════════");

    if matches.is_empty() {
        println!("No se encontraron documentos con similitud >= {:.1}%.", threshold);
        println!("Sugerencias:");
        println!("   • Intenta bajar el umbral (ej: 10% o 20%) para capturar similitudes leves.");
        println!("   • Verifica que los archivos comparados contengan texto real (no imágenes escaneadas).");
        println!("   • Asegúrate de que la carpeta escaneada contenga otros documentos de texto/PDF.");
    } else {
        println!(
            "{:<35} {:>10}   {:>9}   {:>8}   {}",
            "DOCUMENTO COMPARADO", "SIMILITUD", "PALABRAS", "FRASES", "RUTA"
        );
        println!("──────────────────────────────────────────────────────────────────────────────────────");

        for m in matches {
            // Barra visual de progreso de 20 bloques
            let bar_filled = ((m.similarity_percentage / 100.0) * 20.0).round() as usize;
            let bar_filled = bar_filled.min(20);
            let bar = "█".repeat(bar_filled);
            let empty = "░".repeat(20 - bar_filled);

            println!(
                "{:<35} {:>5.1}% [{}{}]   +{} palabras   +{} frases   {}",
                m.candidate_file.name(),
                m.similarity_percentage,
                bar,
                empty,
                m.shared_keyword_count,
                m.shared_bigram_count,
                m.candidate_file.path.display()
            );
        }
    }
    println!("══════════════════════════════════════════════════════════════════════════════════\n");
}

/// Muestra los grupos de archivos que comparten el mismo nombre en distintas ubicaciones.
///
/// Por cada grupo imprime el nombre compartido, cuántos archivos lo tienen
/// y la ruta completa de cada uno para que el usuario decida qué hacer.
pub fn print_same_name_results(groups: &[SameNameGroup]) {
    if groups.is_empty() {
        println!("\nNo se encontraron archivos con el mismo nombre en esta carpeta.\n");
        return;
    }

    let total_files: usize = groups.iter().map(|g| g.files.len()).sum();

    println!("\n=== Archivos con el Mismo Nombre ===");
    println!("══════════════════════════════════════════════════════════════════");

    for (i, group) in groups.iter().enumerate() {
        println!(
            "\n[Grupo #{}] '{}' — {} archivos encontrados",
            i + 1,
            group.shared_name,
            group.files.len()
        );
        println!("  ─────────────────────────────────────────────────────────────────");
        for (j, file) in group.files.iter().enumerate() {
            println!(
                "  [{}.{}] {}   ({})",
                i + 1,
                j + 1,
                format_size(file.size_bytes),
                file.path.display()
            );
        }
    }

    println!("\n══════════════════════════════════════════════════════════════════");
    println!(
        "Resumen: {} grupo(s) con nombre repetido | {} archivos en total",
        groups.len(),
        total_files
    );
    println!("   Tip: usa la opcion [4] para verificar si ademas tienen el mismo contenido (duplicados exactos).");
    println!("══════════════════════════════════════════════════════════════════\n");
}
