use image::{DynamicImage, ImageBuffer, Rgb, RgbImage};
use ndarray::{Array2, Axis as NdAxis};

use crate::cli::{Axis, Colormap};
use crate::nifti_io::LoadedVolume;
use crate::windowing::Window;

const TRIPTYCH_GUTTER: u32 = 8;
const TRIPTYCH_AXES: [Axis; 3] = [Axis::Sagittal, Axis::Axial, Axis::Coronal];

pub fn extract_slice(volume: &LoadedVolume, axis: Axis, slice: usize, time: usize) -> Array2<f32> {
    let time = volume.clamp_volume(time);
    let slice = volume.clamp_slice(axis.index(), slice);

    match axis {
        Axis::Axial => volume
            .data
            .slice(ndarray::s![.., .., slice, time])
            .to_owned(),
        Axis::Coronal => volume
            .data
            .slice(ndarray::s![.., slice, .., time])
            .to_owned(),
        Axis::Sagittal => volume
            .data
            .slice(ndarray::s![slice, .., .., time])
            .to_owned(),
    }
}

pub fn render_slice_image(
    slice: &Array2<f32>,
    axis: Axis,
    pixdim: [f32; 4],
    colormap: Colormap,
    window: Window,
) -> DynamicImage {
    let oriented = orient_for_display(slice, axis);
    let width = oriented.shape()[0] as u32;
    let height = oriented.shape()[1] as u32;
    let mut img: RgbImage = ImageBuffer::new(width, height);

    for x in 0..width {
        for y in 0..height {
            let value = oriented[[x as usize, y as usize]];
            img.put_pixel(x, y, map_value(value, colormap, window));
        }
    }

    let (sx, sy) = in_plane_spacing(axis, pixdim);
    let corrected = resize_for_spacing(&img, sx, sy);
    DynamicImage::ImageRgb8(corrected)
}

pub fn render_triptych_image(
    volume: &LoadedVolume,
    cursor: [usize; 3],
    volume_index: usize,
    colormap: Colormap,
    window: Window,
) -> DynamicImage {
    let panels = TRIPTYCH_AXES
        .into_iter()
        .map(|axis| {
            let slice = extract_slice(volume, axis, cursor[axis.index()], volume_index);
            render_slice_image(&slice, axis, volume.pixdim, colormap, window).to_rgb8()
        })
        .collect::<Vec<_>>();

    let common_height = panels
        .iter()
        .map(RgbImage::height)
        .max()
        .unwrap_or(1)
        .max(1);
    let resized = panels
        .iter()
        .map(|panel| resize_to_height(panel, common_height))
        .collect::<Vec<_>>();

    let total_width = resized.iter().map(RgbImage::width).sum::<u32>()
        + TRIPTYCH_GUTTER.saturating_mul(resized.len().saturating_sub(1) as u32);
    let mut canvas = RgbImage::from_pixel(total_width.max(1), common_height, Rgb([0, 0, 0]));

    let mut x = 0u32;
    for panel in resized {
        image::imageops::replace(&mut canvas, &panel, i64::from(x), 0);
        x = x
            .saturating_add(panel.width())
            .saturating_add(TRIPTYCH_GUTTER);
    }

    DynamicImage::ImageRgb8(canvas)
}

pub fn triptych_order_label() -> &'static str {
    "sag|ax|cor"
}

fn orient_for_display(slice: &Array2<f32>, axis: Axis) -> Array2<f32> {
    let mut oriented = slice.clone();

    match axis {
        Axis::Axial | Axis::Coronal | Axis::Sagittal => {
            oriented.invert_axis(NdAxis(1));
            oriented
        }
    }
}

fn map_value(value: f32, colormap: Colormap, window: Window) -> Rgb<u8> {
    if !value.is_finite() {
        return Rgb([0, 0, 0]);
    }

    let norm = ((value - window.lo) / (window.hi - window.lo)).clamp(0.0, 1.0);
    match colormap {
        Colormap::Gray => {
            let gray = (norm * 255.0).round() as u8;
            Rgb([gray, gray, gray])
        }
        Colormap::Viridis => gradient_rgb(colorous::VIRIDIS, norm),
        Colormap::Magma => gradient_rgb(colorous::MAGMA, norm),
        Colormap::Turbo => gradient_rgb(colorous::TURBO, norm),
        Colormap::Hot => hot_rgb(norm),
    }
}

fn gradient_rgb(gradient: colorous::Gradient, norm: f32) -> Rgb<u8> {
    let color = gradient.eval_continuous(norm as f64);
    Rgb([color.r, color.g, color.b])
}

fn hot_rgb(norm: f32) -> Rgb<u8> {
    let n = norm.clamp(0.0, 1.0);
    let r = (3.0 * n).clamp(0.0, 1.0);
    let g = (3.0 * n - 1.0).clamp(0.0, 1.0);
    let b = (3.0 * n - 2.0).clamp(0.0, 1.0);
    Rgb([
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ])
}

fn resize_for_spacing(image: &RgbImage, sx: f32, sy: f32) -> RgbImage {
    let width = image.width().max(1);
    let height = image.height().max(1);
    let aspect = if sy > 0.0 { sx / sy } else { 1.0 };
    let target_width = ((width as f32 * aspect).round() as u32).max(1);

    if target_width == width {
        return image.clone();
    }

    image::imageops::resize(
        image,
        target_width,
        height,
        image::imageops::FilterType::Lanczos3,
    )
}

fn resize_to_height(image: &RgbImage, target_height: u32) -> RgbImage {
    if image.height() == target_height.max(1) {
        return image.clone();
    }

    let target_width = ((image.width().max(1) as f32) * (target_height.max(1) as f32)
        / image.height().max(1) as f32)
        .round()
        .max(1.0) as u32;

    image::imageops::resize(
        image,
        target_width,
        target_height.max(1),
        image::imageops::FilterType::Lanczos3,
    )
}

fn in_plane_spacing(axis: Axis, pixdim: [f32; 4]) -> (f32, f32) {
    match axis {
        Axis::Axial => (pixdim[0].max(1e-6), pixdim[1].max(1e-6)),
        Axis::Coronal => (pixdim[0].max(1e-6), pixdim[2].max(1e-6)),
        Axis::Sagittal => (pixdim[1].max(1e-6), pixdim[2].max(1e-6)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array4;

    use crate::windowing::Window;

    fn test_volume() -> LoadedVolume {
        LoadedVolume {
            path: "test.nii.gz".into(),
            header: nifti::NiftiHeader::default(),
            data: Array4::from_shape_fn((8, 10, 12, 1), |(x, y, z, _)| (x + y + z) as f32),
            dims: [8, 10, 12, 1],
            pixdim: [1.0, 2.0, 1.5, 1.0],
            dtype: "float32".to_string(),
            affine: nalgebra::Matrix4::identity(),
            inverse_affine: Some(nalgebra::Matrix4::identity()),
            reorientation: crate::nifti_io::Reorientation {
                perm: [0, 1, 2],
                flip: [false, false, false],
                original_dims: [8, 10, 12],
            },
            source_orientation: "RAS".to_string(),
            display_orientation: "RAS".to_string(),
            range: (0.0, 30.0),
            nan_count: 0,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn triptych_renderer_uses_expected_panel_order_and_output_shape() {
        let image = render_triptych_image(
            &test_volume(),
            [3, 5, 7],
            0,
            Colormap::Gray,
            Window { lo: 0.0, hi: 30.0 },
        )
        .to_rgb8();

        assert_eq!(triptych_order_label(), "sag|ax|cor");
        assert!(image.width() > image.height());
        assert!(image.width() > TRIPTYCH_GUTTER * 2);
    }

    #[test]
    fn slice_rendering_applies_spacing_correction() {
        let volume = test_volume();
        let slice = extract_slice(&volume, Axis::Axial, 6, 0);
        let image = render_slice_image(
            &slice,
            Axis::Axial,
            volume.pixdim,
            Colormap::Gray,
            Window { lo: 0.0, hi: 30.0 },
        )
        .to_rgb8();

        assert!(image.width() > 0);
        assert!(image.height() > 0);
    }
}
