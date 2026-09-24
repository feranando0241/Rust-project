//! # OxideClean
//! 
//! Herramienta de escaneo, organización y optimización de archivos construida en Rust.

mod models;
mod scanner;
mod formatter;
mod actions;
mod similarity;

use std::io;
use std::path::{Path, PathBuf};

use actions::{
    filter_by_extension, find_exact_duplicates, find_same_name_files, get_top_largest,
    send_files_to_trash,
};
use formatter::{
    print_accessible_directories, print_exact_duplicates, print_filtered_files,
    print_same_name_results, print_similarity_results, print_top_largest, print_tree,
};
use models::FileInfo;
use scanner::{list_accessible_subdirectories, scan_directory};
use similarity::find_similar_documents;

/// Punto de entrada principal de la aplicación.
fn main() {
    println!(" OxideClean — Organizador & Visor de Archivos");
    println!("=================================================");

    loop {
        println!("\nOpciones de inicio:");
        println!("   - Escribe una ruta a escanear (ej: ~/Downloads, . para actual)");
        println!("   - Escribe 'dirs' o 'ls' para explorar tus carpetas accesibles");
        println!("   - Escribe '0' o 'exit' para salir");
        print!("Ingresa tu opción o ruta: ");

        let user_input = read_line_from_user();

        if is_exit_command(&user_input) {
            println!("¡Hasta luego! Gracias por usar OxideClean.");
            break;
        }

        if user_input.eq_ignore_ascii_case("dirs") || user_input.eq_ignore_ascii_case("ls") {
            if let Some(selected_path) = run_directory_browser() {
                process_target_path(&selected_path);
            }
            continue;
        }

        let target_path = resolve_user_path(&user_input);

        if !target_path.exists() {
            eprintln!("La ruta '{}' no existe. Intenta con otra.", target_path.display());
            continue;
        }

        if !target_path.is_dir() {
            eprintln!("'{}' no es un directorio. Intenta con otra ruta.", target_path.display());
            continue;
        }

        process_target_path(&target_path);
    }
}

/// Ejecuta el explorador interactivo de directorios accesibles.
fn run_directory_browser() -> Option<PathBuf> {
    let mut current_browser_dir = if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
    } else {
        PathBuf::from(".")
    };

    loop {
        let subdirs = match list_accessible_subdirectories(&current_browser_dir) {
            Ok(dirs) => dirs,
            Err(err) => {
                eprintln!("No se pudo listar '{}': {}", current_browser_dir.display(), err);
                return None;
            }
        };

        print_accessible_directories(&current_browser_dir, &subdirs);

        println!("Navegación:");
        println!("   - Ingresa el número [#] de la carpeta para entrar o escanearla");
        println!("   - Ingresa '..' para subir un nivel");
        println!("   - Ingresa 'scan' para escanear esta carpeta actual ({})", current_browser_dir.display());
        println!("   - Ingresa '0' o 'cancelar' para volver");
        print!("Selecciona: ");

        let choice = read_line_from_user();

        if choice == "0" || choice.eq_ignore_ascii_case("cancelar") || choice.eq_ignore_ascii_case("volver") || choice.eq_ignore_ascii_case("exit") {
            return None;
        }

        if choice.eq_ignore_ascii_case("scan") || choice.eq_ignore_ascii_case("analizar") {
            return Some(current_browser_dir);
        }

        if choice == ".." {
            if let Some(parent) = current_browser_dir.parent() {
                current_browser_dir = parent.to_path_buf();
            } else {
                println!("Ya estás en la raíz del sistema de archivos.");
            }
            continue;
        }

        if let Ok(idx) = choice.parse::<usize>() {
            if idx >= 1 && idx <= subdirs.len() {
                let chosen = &subdirs[idx - 1];
                println!("\n¿Qué deseas hacer con '{}'?", chosen.display());
                println!("  [1] Escanear y analizar esta carpeta");
                println!("  [2] Entrar y ver sus subcarpetas");
                println!("  [0] Volver");
                print!("        Opción: ");

                let sub_choice = read_line_from_user();
                match sub_choice.as_str() {
                    "1" => return Some(chosen.clone()),
                    "2" => {
                        current_browser_dir = chosen.clone();
                    }
                    _ => {}
                }
            } else {
                println!("Número fuera de rango.");
            }
        } else {
            println!("Opción no reconocida.");
        }
    }
}

/// Procesa y escanea la ruta seleccionada.
fn process_target_path(target_path: &Path) {
    println!("\n Escaneando recursivamente '{}'...", target_path.display());

    match scan_directory(target_path) {
        Ok(files) if files.is_empty() => {
            println!(" No se encontraron archivos en '{}'.", target_path.display());
        }
        Ok(files) => {
            print_tree(target_path, &files);
            run_action_menu(target_path, &files);
        }
        Err(error) => {
            eprintln!("Error al escanear: {}", error);
        }
    }
}

// ─────────────────────────────────────────────────────────────
// MENÚ DE ACCIONES POST-ESCANEO
// ─────────────────────────────────────────────────────────────

/// Muestra y ejecuta el submenú de acciones para la carpeta actualmente analizada.
fn run_action_menu(target_path: &Path, files: &[FileInfo]) {
    loop {
        println!("┌────────────────────────────────────────────────────────────┐");
        println!("│ ¿Qué deseas hacer con esta carpeta?                        │");
        println!("│ [1] Ver árbol completo de archivos                         │");
        println!("│ [2] Ver Top 10 archivos más pesados                        │");
        println!("│ [3] Filtrar por tipo / extensión (ej: pdf, png)            │");
        println!("│ [4] Buscar duplicados y limpiar a la Papelera              │");
        println!("│ [5] Ver subdirectorios accesibles en esta ruta             │");
        println!("│ [6] Analizar similitud de contenido entre documentos       │");
        println!("│ [7] Buscar archivos con el mismo nombre                    │");
        println!("│ [0] Volver al menú principal / Escanear otra carpeta       │");
        println!("└────────────────────────────────────────────────────────────┘");
        print!("Selecciona una opción [0-7]: ");

        let option = read_line_from_user();

        match option.as_str() {
            "1" => {
                print_tree(target_path, files);
            }
            "2" => {
                let top_10 = get_top_largest(files, 10);
                print_top_largest(&top_10);
            }
            "3" => {
                println!("\nIngresa la extensión a buscar (ej: pdf, mov, png, jpg):");
                let ext = read_line_from_user();
                if ext.is_empty() {
                    println!("No ingresaste ninguna extensión.");
                } else {
                    let filtered = filter_by_extension(files, &ext);
                    if filtered.is_empty() {
                        println!("No se encontraron archivos con la extensión '.{}'.", ext);
                    } else {
                        print_filtered_files(&ext, &filtered);
                    }
                }
            }
            "4" => {
                handle_duplicates_and_trash(files);
            }
            "5" => {
                if let Ok(subdirs) = list_accessible_subdirectories(target_path) {
                    print_accessible_directories(target_path, &subdirs);
                } else {
                    eprintln!(" No se pudieron listar los subdirectorios.");
                }
            }
            "6" => {
                handle_similarity_analysis(files);
            }
            "7" => {
                handle_same_name_files(files);
            }
            "0" | "exit" | "volver" => {
                println!("Regresando al menú principal...");
                break;
            }
            _ => {
                println!("Opción no válida. Por favor ingresa un número del 0 al 7.");
            }
        }
    }
}

/// Ejecuta el análisis de similitud de contenido interactivo.
///
/// El análisis compara un archivo base contra todos los demás documentos
/// de la carpeta escaneada usando un score compuesto (vocabulario + frases + longitud).
fn handle_similarity_analysis(files: &[FileInfo]) {
    // Filtrar solo archivos de texto y PDFs
    let text_or_pdf_files: Vec<&FileInfo> = files
        .iter()
        .filter(|f| {
            let ext = f.extension.as_deref().unwrap_or("").to_lowercase();
            matches!(
                ext.as_str(),
                "pdf" | "txt" | "md" | "rs" | "json" | "csv" | "py" | "js" | "c" | "cpp" | "docx"
            )
        })
        .collect();

    if text_or_pdf_files.len() < 2 {
        println!("\nSe necesitan al menos 2 archivos de texto/PDF en esta carpeta para comparar.");
        println!("Tipos soportados: PDF, TXT, MD, código fuente (rs, py, js, c, cpp), JSON, CSV, DOCX\n");
        return;
    }

    println!("\nSelecciona el archivo BASE que deseas comparar contra los demás:");
    println!("──────────────────────────────────────────────────────────────────────────");

    let selected = match select_file_paginated(&text_or_pdf_files) {
        Some(f) => f,
        None => return,
    };

    // Guía de umbrales para el usuario
    println!("\nConfigura el umbral mínimo de similitud:");
    println!("──────────────────────────────────────────────────────────────────────────");
    println!("   El score combina vocabulario compartido, frases en común y longitud.");
    println!("   Guía de referencia para elegir tu umbral:");
    println!("   ─────────────────────────────────────────────────────────────────────");
    println!("   │  >= 70%  │ Muy similares: mismo tema, estructura y frases parecidas │");
    println!("   │  40–69%  │ Relacionados: vocabulario común, diferente enfoque       │");
    println!("   │  15–39%  │ Algo en común: comparten algunos términos del área       │");
    println!("   │  < 15%   │ Distantes: muy poca coincidencia de contenido            │");
    println!("   ─────────────────────────────────────────────────────────────────────");
    println!("  [1] Automático — Muestra documentos con >= 30% de similitud");
    println!("  [2] Personalizado — Tú eliges el porcentaje mínimo");
    print!("Opción [1 o 2]: ");

    let mode_choice = read_line_from_user();
    let threshold: f64 = match mode_choice.as_str() {
        "2" => {
            print!("Ingresa el porcentaje mínimo (ej: 10, 30, 50, 80): ");
            let custom_input = read_line_from_user();
            custom_input.parse::<f64>().unwrap_or(30.0).clamp(0.0, 100.0)
        }
        _ => 30.0, // Modo automático: 30% es un umbral más útil que 50%
    };

    println!(
        "\n Analizando similitudes contra '{}' (umbral: >= {:.1}%)...",
        selected.name(),
        threshold
    );

    let matches = find_similar_documents(selected, files, threshold);
    print_similarity_results(selected.name(), &matches, threshold);
}

/// Muestra una lista de archivos de forma paginada (20 por página) y permite
/// al usuario navegar entre páginas o buscar por nombre antes de seleccionar.
///
/// Retorna el archivo elegido, o `None` si el usuario cancela.
fn select_file_paginated<'a>(files: &[&'a FileInfo]) -> Option<&'a FileInfo> {
    const PAGE_SIZE: usize = 20;
    let total = files.len();
    // total_pages es fijo: se usa en el encabezado cuando no hay búsqueda activa.
    // page_total se recalcula en cada iteración para reflejar la lista activa (puede ser filtrada).
    let total_pages = total.div_ceil(PAGE_SIZE);
    let mut current_page: usize = 0;
    // Lista activa: puede ser el listado completo o un resultado de búsqueda.
    let mut active_list: Vec<&FileInfo> = files.to_vec();
    let mut search_active = false;

    loop {
        let page_total = active_list.len().div_ceil(PAGE_SIZE).max(1);
        let page = current_page.min(page_total - 1);
        let start = page * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(active_list.len());
        let page_slice = &active_list[start..end];

        // Encabezado de página
        if search_active {
            println!(
                "\n Resultados de búsqueda ({} encontrados) — Página {}/{}",
                active_list.len(), page + 1, page_total
            );
        } else {
            println!(
                "\n Archivos disponibles ({} total) — Página {}/{}",
                total, page + 1, total_pages
            );
        }
        println!("──────────────────────────────────────────────────────────────────────────");
        println!("  {:<4}  {:<30}  {:<6}  {}", "#", "NOMBRE", "TIPO", "CARPETA");
        println!("  ────  ──────────────────────────────  ──────  ──────────────────────────");

        for (i, f) in page_slice.iter().enumerate() {
            let global_num = start + i + 1;
            let ext = f.extension.as_deref().unwrap_or("?").to_uppercase();
            // Mostrar solo el nombre de la carpeta contenedora para no llenar la pantalla
            let folder = f.path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("raíz");
            println!(
                "  {:<4}  {:<30}  {:<6}  {}",
                format!("[{}]", global_num),
                truncate_name(f.name(), 30),
                ext,
                truncate_name(folder, 26),
            );
        }

        println!("──────────────────────────────────────────────────────────────────────────");
        println!("Navegación:");

        // Solo mostrar opciones de paginación cuando hay más de una página
        if page_total > 1 {
            if page + 1 < page_total {
                println!("  [n] Página siguiente →");
            }
            if page > 0 {
                println!("  [p] Página anterior ←");
            }
        }
        if search_active {
            println!("  [r] Resetear búsqueda / ver todos");
        } else {
            println!("  [b] Buscar por nombre de archivo");
        }
        println!("  [0] Cancelar");
        print!("Ingresa un numero o comando: ");

        let input = read_line_from_user();

        match input.to_lowercase().as_str() {
            "0" | "cancelar" => return None,

            "n" if page + 1 < page_total => {
                current_page = page + 1;
            }
            "p" if page > 0 => {
                current_page = page - 1;
            }

            "b" => {
                print!("Escribe parte del nombre a buscar: ");
                let query = read_line_from_user().to_lowercase();
                if query.is_empty() {
                    println!("Búsqueda vacía. Mostrando todos los archivos.");
                    active_list = files.to_vec();
                    search_active = false;
                } else {
                    active_list = files
                        .iter()
                        .filter(|f| f.name().to_lowercase().contains(&query))
                        .copied()
                        .collect();
                    search_active = true;
                    current_page = 0;

                    if active_list.is_empty() {
                        println!("No se encontraron archivos con '{}'. Volviendo a la lista completa.", query);
                        active_list = files.to_vec();
                        search_active = false;
                    } else {
                        println!(" {} archivo(s) encontrado(s) con '{}'.", active_list.len(), query);
                    }
                }
            }

            "r" => {
                active_list = files.to_vec();
                search_active = false;
                current_page = 0;
            }

            raw => {
                // Intentar interpretar como número de selección
                if let Ok(num) = raw.parse::<usize>() {
                    if num >= 1 && num <= active_list.len() {
                        let chosen = active_list[num - 1];
                        println!("\n Seleccionado: {} ({})", chosen.name(), chosen.path.display());
                        return Some(chosen);
                    } else {
                        println!("Número fuera de rango (1–{}). Intenta de nuevo.", active_list.len());
                    }
                } else {
                    println!("Opción no reconocida. Usa un número, 'n', 'p', 'b', 'r' o '0'.");
                }
            }
        }
    }
}

/// Acorta un texto al máximo de caracteres indicado, añadiendo '…' si fue cortado.
fn truncate_name(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{}…", cut)
    }
}

/// Busca y muestra archivos que comparten el mismo nombre dentro de la carpeta escaneada.
///
/// A diferencia de los duplicados exactos (opción 4), aquí no se analiza el contenido:
/// dos archivos con el mismo nombre pero diferente contenido (ej: versiones distintas
/// de un documento) también aparecerán listados.
fn handle_same_name_files(files: &[FileInfo]) {
    println!("\nBuscando archivos con el mismo nombre...");
    let groups = find_same_name_files(files);
    print_same_name_results(&groups);
}

/// Ejecuta el análisis de duplicados por Hash SHA-256 y ofrece moverlos a la papelera.
fn handle_duplicates_and_trash(files: &[FileInfo]) {
    println!("\nCalculando hashes SHA-256 para verificar duplicados exactos...");
    let duplicate_groups = find_exact_duplicates(files);

    if duplicate_groups.is_empty() {
        println!("¡Excelente! No se encontraron archivos duplicados exactos en esta carpeta.\n");
        return;
    }

    print_exact_duplicates(&duplicate_groups);

    let files_to_trash: Vec<FileInfo> = duplicate_groups
        .iter()
        .flat_map(|g| g.duplicates.clone())
        .collect();

    println!("¿Deseas mover estos {} archivo(s) duplicados a la Papelera de reciclaje?", files_to_trash.len());
    println!("Ingresa 's' o 'si' para confirmar, cualquier otra tecla para cancelar:");

    let confirmation = read_line_from_user();

    if confirmation.eq_ignore_ascii_case("s") || confirmation.eq_ignore_ascii_case("si") {
        match send_files_to_trash(&files_to_trash) {
            Ok(count) => {
                println!("\n¡Éxito! Se movieron {} archivo(s) a la Papelera de tu Mac.", count);
                println!("Puedes restaurarlos desde la Papelera si los necesitas.\n");
            }
            Err(err) => {
                eprintln!("\n{}", err);
            }
        }
    } else {
        println!("\nOperación cancelada. No se modificó ningún archivo.\n");
    }
}

// ─────────────────────────────────────────────────────────────
// FUNCIONES AUXILIARES DE ENTRADA Y RUTAS
// ─────────────────────────────────────────────────────────────

/// Lee una línea desde la entrada estándar (teclado) y elimina espacios y saltos de línea.
fn read_line_from_user() -> String {
    let mut buffer = String::new();
    io::stdin()
        .read_line(&mut buffer)
        .expect("Error al leer la entrada del usuario");
    buffer.trim().to_string()
}

/// Comprueba si el texto ingresado corresponde a un comando de salida.
fn is_exit_command(input: &str) -> bool {
    input == "0" || input.eq_ignore_ascii_case("exit")
}

/// Convierte la entrada del usuario en un `PathBuf` válido, expandiendo `~` al directorio home.
fn resolve_user_path(input: &str) -> PathBuf {
    if input.starts_with("~/") || input == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            let mut path = PathBuf::from(home);
            if input.len() > 2 {
                path.push(&input[2..]);
            }
            path
        } else {
            PathBuf::from(input)
        }
    } else if input.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(input)
    }
}

// ─────────────────────────────────────────────────────────────
// PRUEBAS AUTOMATIZADAS
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use formatter::format_size;
    use scanner::should_ignore_directory;
    use similarity::calculate_jaccard_similarity;

    #[test]
    fn test_format_sizes() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_size(10 * 1024 * 1024 * 1024), "10.00 GB");
    }

    #[test]
    fn test_is_exit_command() {
        assert!(is_exit_command("0"));
        assert!(is_exit_command("exit"));
        assert!(is_exit_command("EXIT"));
        assert!(is_exit_command("Exit"));
        assert!(!is_exit_command("no"));
        assert!(!is_exit_command("~/Downloads"));
    }

    #[test]
    fn test_should_ignore_directory() {
        assert!(should_ignore_directory("node_modules"));
        assert!(should_ignore_directory("target"));
        assert!(should_ignore_directory(".git"));
        assert!(should_ignore_directory(".cache"));
        assert!(!should_ignore_directory("fotos"));
        assert!(!should_ignore_directory("documentos"));
    }

    #[test]
    fn test_similarity_algorithm() {
        use std::collections::HashSet;
        let mut set1 = HashSet::new();
        set1.insert("tesis sobre inteligencia".to_string());
        set1.insert("inteligencia artificial aplicada".to_string());
        set1.insert("resultados del experimento".to_string());

        let mut set2 = HashSet::new();
        set2.insert("tesis sobre inteligencia".to_string());
        set2.insert("inteligencia artificial aplicada".to_string());
        set2.insert("otra seccion diferente".to_string());

        let (sim, count) = calculate_jaccard_similarity(&set1, &set2);
        assert_eq!(count, 2);
        assert!(sim >= 50.0, "Debe tener al menos 50% de similitud");
    }
}
