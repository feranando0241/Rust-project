use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use crate::models::FileInfo;

/// Lista de carpetas técnicas, de dependencias y de sistema que se ignoran por defecto.
///
/// Estas carpetas generan mucho "ruido" porque contienen cientos de archivos
/// de librerías, compilados o del sistema que no son relevantes para el usuario.
pub const IGNORED_DIRECTORIES: &[&str] = &[
    // Control de versiones
    ".git",
    // Compilación de Rust
    "target",
    // Dependencias de JavaScript/Node
    "node_modules",
    // Caché y configuración de IDEs / editores
    ".cache",
    ".vscode",
    ".idea",
    // Papelera del sistema
    ".Trash",
    // Entornos virtuales de Python (nombres exactos más comunes)
    "venv",
    ".venv",
    "env",
    ".env",
    "__pycache__",
    // Librería del sistema en macOS
    "Library",
    // Carpetas de build de proyectos web / Java / Python
    "dist",
    "build",
    ".next",
    ".nuxt",
    "out",
    // Gestores de paquetes de Python y Ruby
    "site-packages",
    ".gem",
    // Carpetas de caché de herramientas
    ".npm",
    ".yarn",
    ".pnpm-store",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
];

/// Lista de archivos ocultos, de sistema o binarios que se ignoran durante el escaneo.
pub const IGNORED_FILES: &[&str] = &[
    // Archivos de metadatos de macOS
    ".DS_Store",
    ".localized",
    // Archivos de metadatos de Windows
    "Thumbs.db",
    "desktop.ini",
    // Archivos de bloqueo de dependencias (generados automáticamente)
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Pipfile.lock",
    "poetry.lock",
];

/// Extensiones de archivos compilados o binarios que se ignoran (sin el punto).
const IGNORED_EXTENSIONS: &[&str] = &[
    "pyc",  // Bytecode compilado de Python
    "pyo",  // Bytecode optimizado de Python
    "class", // Bytecode compilado de Java
    "o",    // Objeto compilado de C/C++
];

/// Determina si un archivo individual debe ser ignorado.
///
/// Ignora archivos de la lista exacta y también los que tienen extensiones de bytecode/binario.
pub fn should_ignore_file(file_name: &str) -> bool {
    if IGNORED_FILES.contains(&file_name) {
        return true;
    }
    // Ignorar por extensión (bytecode y compilados)
    if let Some(ext) = file_name.rsplit('.').next() {
        if IGNORED_EXTENSIONS.contains(&ext) {
            return true;
        }
    }
    false
}

/// Determina si un nombre de directorio debe ser ignorado durante el escaneo.
///
/// Ignora las carpetas de la lista explícita, todas las carpetas ocultas (`.algo`),
/// las que terminan en `_venv` o `-env` (variantes de entornos Python),
/// y los paquetes de aplicaciones de macOS (`.app`).
pub fn should_ignore_directory(dir_name: &str) -> bool {
    if IGNORED_DIRECTORIES.contains(&dir_name) {
        return true;
    }
    // Carpetas ocultas del sistema (empiezan con punto), excepto "." que es el directorio actual
    if dir_name.starts_with('.') && dir_name != "." {
        return true;
    }
    // Variantes de entornos virtuales de Python: proyecto_venv, mi-env, report_venv, etc.
    if dir_name.ends_with("_venv") || dir_name.ends_with("-venv") || dir_name.ends_with("-env") {
        return true;
    }
    // Paquetes de aplicaciones de macOS (.app, .framework, .bundle)
    if dir_name.ends_with(".app") || dir_name.ends_with(".framework") || dir_name.ends_with(".bundle") {
        return true;
    }
    false
}

/// Escanea un directorio y lista únicamente las subcarpetas inmediatas a las que se tiene acceso.
pub fn list_accessible_subdirectories(dir_path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    let entries = fs::read_dir(dir_path)?;

    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let file_name = entry.file_name();
            let name_str = file_name.to_str().unwrap_or("");
            if !should_ignore_directory(name_str) {
                dirs.push(entry.path());
            }
        }
    }

    dirs.sort();
    Ok(dirs)
}

/// Escanea un directorio y todas sus subcarpetas de manera recursiva.
/// Retorna una lista con todos los archivos encontrados.
pub fn scan_directory(dir_path: &Path) -> io::Result<Vec<FileInfo>> {
    let mut files = Vec::new();
    scan_recursive(dir_path, &mut files)?;
    Ok(files)
}

/// Función auxiliar recursiva para explorar la jerarquía de carpetas.
fn scan_recursive(current_path: &Path, files: &mut Vec<FileInfo>) -> io::Result<()> {
    let entries = match fs::read_dir(current_path) {
        Ok(read_dir) => read_dir,
        Err(_) => return Ok(()), // Si no hay permisos de lectura, saltamos en silencio
    };

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_str().unwrap_or("");

        if file_type.is_dir() {
            if should_ignore_directory(name_str) {
                continue;
            }
            scan_recursive(&path, files)?;
        } else if file_type.is_file() {
            if should_ignore_file(name_str) {
                continue;
            }

            let size_bytes = entry.metadata()?.len();
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|s| s.to_lowercase());

            files.push(FileInfo {
                path,
                size_bytes,
                extension,
            });
        }
    }

    Ok(())
}
