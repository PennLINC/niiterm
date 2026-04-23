use std::path::Path;

use crate::dwi::DwiMetadata;
use crate::modality::Modality;
use crate::nifti_io::LoadedVolume;

pub fn format_stats_line(
    volume: &LoadedVolume,
    modality: Modality,
    volume_index: usize,
    dwi: Option<&DwiMetadata>,
) -> String {
    let filename = volume
        .path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_else(|| path_fallback(&volume.path));

    let mut line = format!(
        "{filename}  {}x{}x{}  {:.2}x{:.2}x{:.2}mm  {}  {}  range=[{:.1}, {:.1}]  nan={}",
        volume.dims[0],
        volume.dims[1],
        volume.dims[2],
        volume.pixdim[0],
        volume.pixdim[1],
        volume.pixdim[2],
        volume.display_orientation,
        volume.dtype,
        volume.range.0,
        volume.range.1,
        volume.nan_count
    );

    if volume.nvols() > 1 {
        line.push_str(&format!("  vol={}/{}", volume_index + 1, volume.nvols()));
        if matches!(modality, Modality::Bold) {
            line.push_str("  play=space");
        }
    }

    if let Some(dwi) = dwi {
        if let Some((bval, bvec)) = dwi.entry(volume_index) {
            line.push_str(&format!(
                "  b={:.0} vec=({:.3}, {:.3}, {:.3})",
                bval, bvec[0], bvec[1], bvec[2]
            ));
        }
    }

    line
}

fn path_fallback(path: &Path) -> &str {
    path.to_str().unwrap_or("<unknown>")
}
