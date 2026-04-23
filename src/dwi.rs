use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;
use tracing::warn;

use crate::modality::stem_without_nii;

#[derive(Debug, Clone)]
pub struct DwiMetadata {
    pub bvals: Vec<f32>,
    pub bvecs: Vec<[f32; 3]>,
}

impl DwiMetadata {
    pub fn entry(&self, index: usize) -> Option<(f32, [f32; 3])> {
        let bval = *self.bvals.get(index)?;
        let bvec = *self.bvecs.get(index)?;
        Some((bval, bvec))
    }
}

#[derive(Debug, Error)]
pub enum DwiError {
    #[error("bval file contained no values")]
    EmptyBvals,
    #[error("bvec file did not have 3 rows or 3 columns")]
    BadBvecShape,
    #[error("bval and bvec lengths did not match")]
    LengthMismatch,
    #[error("failed to parse DWI sidecar {path}: {message}")]
    Parse { path: PathBuf, message: String },
}

pub fn load_for_nifti(path: &Path) -> Result<Option<DwiMetadata>, DwiError> {
    let stem = stem_without_nii(path);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let bval_path = dir.join(format!("{stem}.bval"));
    let bvec_path = dir.join(format!("{stem}.bvec"));

    if !bval_path.exists() || !bvec_path.exists() {
        return Ok(None);
    }

    let bvals = parse_bvals(&bval_path)?;
    let bvecs = parse_bvecs(&bvec_path)?;

    if bvals.len() != bvecs.len() {
        return Err(DwiError::LengthMismatch);
    }

    Ok(Some(DwiMetadata { bvals, bvecs }))
}

pub fn load_with_warning(path: &Path) -> Option<DwiMetadata> {
    match load_for_nifti(path) {
        Ok(found) => found,
        Err(error) => {
            warn!("{error}");
            None
        }
    }
}

fn parse_bvals(path: &Path) -> Result<Vec<f32>, DwiError> {
    let text = fs::read_to_string(path).map_err(|error| DwiError::Parse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let values = text
        .split_whitespace()
        .map(|token| {
            token.parse::<f32>().map_err(|error| DwiError::Parse {
                path: path.to_path_buf(),
                message: error.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        Err(DwiError::EmptyBvals)
    } else {
        Ok(values)
    }
}

fn parse_bvecs(path: &Path) -> Result<Vec<[f32; 3]>, DwiError> {
    let text = fs::read_to_string(path).map_err(|error| DwiError::Parse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let rows = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_whitespace()
                .map(|token| {
                    token.parse::<f32>().map_err(|error| DwiError::Parse {
                        path: path.to_path_buf(),
                        message: error.to_string(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;

    if rows.len() == 3 {
        let n = rows[0].len();
        if rows.iter().any(|row| row.len() != n) {
            return Err(DwiError::BadBvecShape);
        }
        return Ok((0..n)
            .map(|i| [rows[0][i], rows[1][i], rows[2][i]])
            .collect());
    }

    if rows.iter().all(|row| row.len() == 3) {
        return Ok(rows
            .into_iter()
            .map(|row| [row[0], row[1], row[2]])
            .collect());
    }

    Err(DwiError::BadBvecShape)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn loads_dwi_sidecars() {
        let dir = tempdir().unwrap();
        let nifti = dir.path().join("sub-01_dwi.nii.gz");
        let bval = dir.path().join("sub-01_dwi.bval");
        let bvec = dir.path().join("sub-01_dwi.bvec");

        fs::write(&bval, "0 1000 2000\n").unwrap();
        fs::write(&bvec, "1 0 0\n0 1 0\n0 0 1\n").unwrap();

        let meta = load_for_nifti(&nifti).unwrap().unwrap();
        assert_eq!(meta.bvals.len(), 3);
        assert_eq!(meta.bvecs[1], [0.0, 1.0, 0.0]);
    }
}
