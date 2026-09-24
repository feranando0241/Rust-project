use std::path::PathBuf;

/// Representa la información esencial de un archivo en disco.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// Ruta completa al archivo en el sistema de archivos.
    pub path: PathBuf,
    /// Tamaño exacto del archivo en bytes.
    pub size_bytes: u64,
    /// Extensión del archivo (ej: "pdf", "png") o `None` si no tiene.
    pub extension: Option<String>,
}

impl FileInfo {
    /// Obtiene el nombre del archivo (ej: "documento.pdf") como texto.
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("desconocido")
    }

    /// Retorna la extensión en minúsculas o un texto por defecto si no tiene.
    pub fn extension_or<'a>(&'a self, default: &'a str) -> &'a str {
        self.extension.as_deref().unwrap_or(default)
    }
}
