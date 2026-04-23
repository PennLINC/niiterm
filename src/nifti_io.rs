use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use nalgebra::{Matrix4, Vector4};
use ndarray::{Array4, Axis as NdAxis, Ix3, Ix4};
use nifti::{IntoNdArray, NiftiHeader, NiftiObject, ReaderOptions};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct Reorientation {
    pub perm: [usize; 3],
    pub flip: [bool; 3],
    pub original_dims: [usize; 3],
}

#[derive(Debug, Clone)]
pub struct LoadedVolume {
    pub path: PathBuf,
    pub header: NiftiHeader,
    pub data: Array4<f32>,
    pub dims: [usize; 4],
    pub pixdim: [f32; 4],
    pub dtype: String,
    pub affine: Matrix4<f64>,
    pub inverse_affine: Option<Matrix4<f64>>,
    pub reorientation: Reorientation,
    pub source_orientation: String,
    pub display_orientation: String,
    pub range: (f32, f32),
    pub nan_count: usize,
    pub warnings: Vec<String>,
}

impl LoadedVolume {
    pub fn axis_len(&self, axis: usize) -> usize {
        self.dims[axis]
    }

    pub fn nvols(&self) -> usize {
        self.dims[3]
    }

    pub fn clamp_volume(&self, volume: usize) -> usize {
        volume.min(self.nvols().saturating_sub(1))
    }

    pub fn clamp_slice(&self, axis: usize, slice: usize) -> usize {
        slice.min(self.axis_len(axis).saturating_sub(1))
    }

    pub fn middle_slice(&self, axis: usize) -> usize {
        self.axis_len(axis) / 2
    }

    pub fn ras_index_from_mm(&self, mm: [f64; 3]) -> Option<[usize; 3]> {
        let inv = self.inverse_affine.as_ref()?;
        let voxel = inv * Vector4::new(mm[0], mm[1], mm[2], 1.0);

        let mut ras = [0usize; 3];
        for (ras_axis, ras_value) in ras.iter_mut().enumerate() {
            let original_axis = self.reorientation.perm[ras_axis];
            let original_dim = self.reorientation.original_dims[original_axis];
            let raw = voxel[original_axis].round();
            let clamped = raw.clamp(0.0, original_dim.saturating_sub(1) as f64) as usize;
            *ras_value = if self.reorientation.flip[ras_axis] {
                original_dim.saturating_sub(1).saturating_sub(clamped)
            } else {
                clamped
            };
        }
        Some(ras)
    }
}

pub fn load_nifti(path: &Path) -> Result<LoadedVolume> {
    let obj = ReaderOptions::new()
        .read_file(path)
        .with_context(|| format!("failed to read NIfTI file {}", path.display()))?;

    let header = obj.header().clone();
    let affine = header.affine::<f64>();
    let inverse_affine = affine.try_inverse();

    let mut warnings = Vec::new();
    if header.sform_code <= 0 && header.qform_code <= 0 {
        warnings.push(
            "missing sform/qform; falling back to base affine and assuming RAS-like indexing"
                .to_string(),
        );
        warn!("{}", warnings.last().unwrap());
    }

    let dyn_arr = obj.into_volume().into_ndarray::<f32>()?;
    let data = match dyn_arr.ndim() {
        3 => dyn_arr
            .into_dimensionality::<Ix3>()
            .map(|arr| arr.insert_axis(NdAxis(3)))
            .map_err(|_| anyhow!("failed to interpret 3D NIfTI array"))?,
        4 => dyn_arr
            .into_dimensionality::<Ix4>()
            .map_err(|_| anyhow!("failed to interpret 4D NIfTI array"))?,
        ndim => {
            return Err(anyhow!(
                "expected a 3D or 4D NIfTI, found {ndim} dimensions"
            ))
        }
    };

    let original_dims = [data.shape()[0], data.shape()[1], data.shape()[2]];
    let (perm, flip, source_orientation) = derive_ras_reorientation(&affine);

    let mut reoriented = data
        .permuted_axes([perm[0], perm[1], perm[2], 3])
        .to_owned();
    for (axis, should_flip) in flip.into_iter().enumerate() {
        if should_flip {
            reoriented.invert_axis(NdAxis(axis));
        }
    }

    let dims = [
        reoriented.shape()[0],
        reoriented.shape()[1],
        reoriented.shape()[2],
        reoriented.shape()[3],
    ];
    let pixdim = [
        header.pixdim[perm[0] + 1].abs(),
        header.pixdim[perm[1] + 1].abs(),
        header.pixdim[perm[2] + 1].abs(),
        header.pixdim[4].abs().max(1.0),
    ];

    let (range, nan_count) = finite_range_and_nans(&reoriented);
    let dtype = header
        .data_type()
        .map(|dtype| format!("{dtype:?}").to_lowercase())
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(LoadedVolume {
        path: path.to_path_buf(),
        header,
        data: reoriented,
        dims,
        pixdim,
        dtype,
        affine,
        inverse_affine,
        reorientation: Reorientation {
            perm,
            flip,
            original_dims,
        },
        source_orientation,
        display_orientation: "RAS".to_string(),
        range,
        nan_count,
        warnings,
    })
}

fn derive_ras_reorientation(affine: &Matrix4<f64>) -> ([usize; 3], [bool; 3], String) {
    let mut candidates = Vec::new();
    for voxel_axis in 0..3 {
        for world_axis in 0..3 {
            let value = affine[(world_axis, voxel_axis)];
            candidates.push((value.abs(), voxel_axis, world_axis, value));
        }
    }
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut perm = [0usize; 3];
    let mut flip = [false; 3];
    let mut used_voxel = [false; 3];
    let mut used_world = [false; 3];

    for (_, voxel_axis, world_axis, value) in candidates {
        if !used_voxel[voxel_axis] && !used_world[world_axis] {
            perm[world_axis] = voxel_axis;
            flip[world_axis] = value < 0.0;
            used_voxel[voxel_axis] = true;
            used_world[world_axis] = true;
        }
    }

    let pos = ['R', 'A', 'S'];
    let neg = ['L', 'P', 'I'];
    let mut orientation = ['?'; 3];
    for (voxel_axis, orientation_value) in orientation.iter_mut().enumerate() {
        if let Some(world_axis) = perm.iter().position(|&axis| axis == voxel_axis) {
            *orientation_value = if flip[world_axis] {
                neg[world_axis]
            } else {
                pos[world_axis]
            };
        }
    }

    (perm, flip, orientation.iter().collect())
}

fn finite_range_and_nans(data: &Array4<f32>) -> ((f32, f32), usize) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut nan_count = 0usize;

    for value in data.iter().copied() {
        if value.is_nan() {
            nan_count += 1;
            continue;
        }
        if value.is_finite() {
            min = min.min(value);
            max = max.max(value);
        }
    }

    if !min.is_finite() || !max.is_finite() {
        ((0.0, 0.0), nan_count)
    } else {
        ((min, max), nan_count)
    }
}
