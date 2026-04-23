use std::path::Path;

use crate::cli::Colormap;
use crate::windowing::WindowMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    T1,
    T2,
    Bold,
    Dwi,
    Asl,
    Unknown,
}

impl Modality {
    pub fn detect(path: &Path) -> Self {
        let lower = stem_without_nii(path).to_lowercase();

        if contains_any(&lower, &["_t1w", "t1.nii", "_mprage", "mprage"]) {
            Self::T1
        } else if contains_any(&lower, &["_t2w", "t2.nii", "_flair", "flair"]) {
            Self::T2
        } else if contains_any(&lower, &["_bold", "_sbref", "bold", "sbref"]) {
            Self::Bold
        } else if contains_any(&lower, &["_dwi", "_dti", "dwi", "dti"]) {
            Self::Dwi
        } else if contains_any(&lower, &["_asl", "_m0scan", "_cbf", "asl", "cbf"]) {
            Self::Asl
        } else {
            Self::Unknown
        }
    }

    pub fn default_colormap(self) -> Colormap {
        match self {
            Self::Asl => Colormap::Turbo,
            _ => Colormap::Gray,
        }
    }

    pub fn default_window(self) -> WindowMode {
        match self {
            Self::Asl => WindowMode::Percentile(5.0, 99.0),
            _ => WindowMode::Percentile(2.0, 98.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::T1 => "T1w",
            Self::T2 => "T2w",
            Self::Bold => "BOLD",
            Self::Dwi => "DWI",
            Self::Asl => "ASL",
            Self::Unknown => "unknown",
        }
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

pub fn stem_without_nii(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_string();

    name.strip_suffix(".nii.gz")
        .or_else(|| name.strip_suffix(".nii"))
        .or_else(|| name.strip_suffix(".hdr.gz"))
        .or_else(|| name.strip_suffix(".hdr"))
        .unwrap_or(&name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_modalities_from_common_names() {
        assert_eq!(
            Modality::detect(Path::new("sub-01_T1w.nii.gz")),
            Modality::T1
        );
        assert_eq!(
            Modality::detect(Path::new("sub-01_FLAIR.nii.gz")),
            Modality::T2
        );
        assert_eq!(
            Modality::detect(Path::new("sub-01_task-rest_bold.nii.gz")),
            Modality::Bold
        );
        assert_eq!(
            Modality::detect(Path::new("sub-01_dwi.nii.gz")),
            Modality::Dwi
        );
        assert_eq!(
            Modality::detect(Path::new("sub-01_cbf.nii.gz")),
            Modality::Asl
        );
    }
}
