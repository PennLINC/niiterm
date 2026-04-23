use image::{DynamicImage, ImageBuffer, Rgb, RgbImage};
use ndarray::{Array2, Axis as NdAxis};

use crate::cli::{Axis, Colormap};
use crate::nifti_io::LoadedVolume;
use crate::windowing::Window;

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

fn orient_for_display(slice: &Array2<f32>, axis: Axis) -> Array2<f32> {
    let mut oriented = slice.clone();

    match axis {
        Axis::Axial | Axis::Coronal => {
            oriented.invert_axis(NdAxis(1));
            oriented
        }
        Axis::Sagittal => {
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

fn in_plane_spacing(axis: Axis, pixdim: [f32; 4]) -> (f32, f32) {
    match axis {
        Axis::Axial => (pixdim[0].max(1e-6), pixdim[1].max(1e-6)),
        Axis::Coronal => (pixdim[0].max(1e-6), pixdim[2].max(1e-6)),
        Axis::Sagittal => (pixdim[1].max(1e-6), pixdim[2].max(1e-6)),
    }
}
