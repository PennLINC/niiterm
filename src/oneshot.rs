use anyhow::{Context, Result};
use image::DynamicImage;
use viuer::Config as ViuerConfig;
use viuer::KittySupport;

use crate::cli::{Args, Protocol, SnapshotMode};
use crate::dwi;
use crate::modality::Modality;
use crate::nifti_io::load_nifti;
use crate::render::{
    extract_slice, render_slice_image, render_triptych_image, triptych_order_label,
};
use crate::stats::format_stats_line;
use crate::windowing::{WindowCache, WindowMode};

pub fn run(args: Args) -> Result<()> {
    let volume = load_nifti(&args.file)?;
    let modality = Modality::detect(&args.file);
    let dwi = if modality == Modality::Dwi {
        dwi::load_with_warning(&args.file)
    } else {
        None
    };

    let colormap = args.colormap.unwrap_or_else(|| modality.default_colormap());
    let window_mode = args
        .window
        .as_deref()
        .map(str::parse::<WindowMode>)
        .transpose()?
        .unwrap_or_else(|| modality.default_window());

    let volume_index = volume.clamp_volume(args.volume);
    let resolved_width = args.width.map(resolve_width_spec);

    let mut cache = WindowCache::default();
    let window = cache.get_or_insert(
        volume_index,
        window_mode,
        volume.data.slice(ndarray::s![.., .., .., volume_index]),
    );

    let image = match args.snapshot {
        Some(SnapshotMode::Mid3) => render_triptych_image(
            &volume,
            [
                volume.middle_slice(0),
                volume.middle_slice(1),
                volume.middle_slice(2),
            ],
            volume_index,
            colormap,
            window,
        ),
        None => {
            let slice_index = resolve_single_slice(&args, &volume);
            let slice = extract_slice(&volume, args.axis, slice_index, volume_index);
            render_slice_image(&slice, args.axis, volume.pixdim, colormap, window)
        }
    };
    let image = prepare_for_terminal(&image, resolved_width, args.protocol);

    if args.show_stats() {
        println!(
            "{}",
            header_line(&args, &volume, modality, volume_index, dwi.as_ref())
        );
    }

    print_image(&image, resolved_width, args.protocol).context("failed to render image to terminal")
}

fn resolve_single_slice(args: &Args, volume: &crate::nifti_io::LoadedVolume) -> usize {
    let mm_coord = args.mm.and_then(|coord| {
        volume.ras_index_from_mm([coord.x as f64, coord.y as f64, coord.z as f64])
    });

    args.slice
        .map(|spec| spec.resolve(volume.axis_len(args.axis.index())))
        .or_else(|| args.coord.map(|coord| coord.component_for_axis(args.axis)))
        .or_else(|| mm_coord.map(|coord| coord[args.axis.index()]))
        .unwrap_or_else(|| volume.middle_slice(args.axis.index()))
}

fn header_line(
    args: &Args,
    volume: &crate::nifti_io::LoadedVolume,
    modality: Modality,
    volume_index: usize,
    dwi: Option<&crate::dwi::DwiMetadata>,
) -> String {
    let mut line = format_stats_line(volume, modality, volume_index, dwi);
    if let Some(snapshot) = args.snapshot {
        line.push_str(&format!(
            "  snapshot={} order={}",
            snapshot.label(),
            triptych_order_label()
        ));
    }
    line
}

fn resolve_width_spec(spec: crate::cli::WidthSpec) -> u32 {
    let (terminal_width, _) = crossterm::terminal::size().unwrap_or((80, 24));
    spec.resolve(terminal_width as u32)
}

fn prepare_for_terminal(
    image: &DynamicImage,
    width: Option<u32>,
    protocol: Protocol,
) -> DynamicImage {
    match resolve_protocol(protocol) {
        ResolvedProtocol::Blocks => image.clone(),
        ResolvedProtocol::Iterm | ResolvedProtocol::Kitty | ResolvedProtocol::Sixel => {
            supersample_for_graphics_protocol(image, width)
        }
    }
}

fn print_image(image: &DynamicImage, width: Option<u32>, protocol: Protocol) -> Result<()> {
    let mut config = ViuerConfig {
        x: 0,
        y: 0,
        width,
        absolute_offset: false,
        restore_cursor: false,
        transparent: false,
        ..Default::default()
    };

    config.truecolor = true;
    config.use_kitty = true;
    config.use_iterm = true;
    config.use_sixel = true;

    match protocol {
        Protocol::Auto => {}
        Protocol::Kitty => {
            config.use_kitty = true;
            config.use_iterm = false;
            config.use_sixel = false;
        }
        Protocol::Iterm => {
            config.use_kitty = false;
            config.use_iterm = true;
            config.use_sixel = false;
        }
        Protocol::Sixel => {
            config.use_kitty = false;
            config.use_iterm = false;
            config.use_sixel = true;
        }
        Protocol::Blocks => {
            config.use_kitty = false;
            config.use_iterm = false;
            config.use_sixel = false;
            config.truecolor = true;
        }
    }

    viuer::print(image, &config)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedProtocol {
    Kitty,
    Iterm,
    Sixel,
    Blocks,
}

fn resolve_protocol(protocol: Protocol) -> ResolvedProtocol {
    match protocol {
        Protocol::Kitty => ResolvedProtocol::Kitty,
        Protocol::Iterm => ResolvedProtocol::Iterm,
        Protocol::Sixel => ResolvedProtocol::Sixel,
        Protocol::Blocks => ResolvedProtocol::Blocks,
        Protocol::Auto => {
            if viuer::is_iterm_supported() {
                ResolvedProtocol::Iterm
            } else if viuer::get_kitty_support() != KittySupport::None {
                ResolvedProtocol::Kitty
            } else if viuer::is_sixel_supported() {
                ResolvedProtocol::Sixel
            } else {
                ResolvedProtocol::Blocks
            }
        }
    }
}

fn supersample_for_graphics_protocol(image: &DynamicImage, width: Option<u32>) -> DynamicImage {
    const PX_PER_CELL_X: u32 = 8;
    const PX_PER_CELL_Y: u32 = 16;
    const MAX_DIMENSION: u32 = 3072;
    const MAX_AREA: u64 = 6_000_000;

    let (target_cells_w, target_cells_h) = best_fit_in_cells(image, width, None);
    let target_w = target_cells_w.saturating_mul(PX_PER_CELL_X).max(1);
    let target_h = target_cells_h.saturating_mul(PX_PER_CELL_Y).max(1);

    let (target_w, target_h) = cap_dimensions(target_w, target_h, MAX_DIMENSION, MAX_AREA);
    let current_w = image.width();
    let current_h = image.height();

    if target_w <= current_w && target_h <= current_h {
        return image.clone();
    }

    image.resize(target_w, target_h, image::imageops::FilterType::Lanczos3)
}

fn best_fit_in_cells(image: &DynamicImage, width: Option<u32>, height: Option<u32>) -> (u32, u32) {
    let (img_width, img_height) = (image.width(), image.height());

    match (width, height) {
        (None, None) => {
            let (term_w, term_h) = crossterm::terminal::size().unwrap_or((80, 24));
            let (w, h) = fit_dimensions(img_width, img_height, term_w as u32, term_h as u32);
            let h = if h == term_h as u32 {
                h.saturating_sub(1)
            } else {
                h
            };
            (w, h)
        }
        (Some(w), None) => fit_dimensions(img_width, img_height, w, img_height),
        (None, Some(h)) => fit_dimensions(img_width, img_height, img_width, h),
        (Some(w), Some(h)) => (w, h),
    }
}

fn fit_dimensions(width: u32, height: u32, bound_width: u32, bound_height: u32) -> (u32, u32) {
    let bound_height = 2 * bound_height;

    if width <= bound_width && height <= bound_height {
        return (width, std::cmp::max(1, height / 2 + height % 2));
    }

    let ratio = width * bound_height;
    let inverse_ratio = bound_width * height;
    let use_width = inverse_ratio <= ratio;
    let intermediate = if use_width {
        height * bound_width / width
    } else {
        width * bound_height / height
    };

    if use_width {
        (bound_width, std::cmp::max(1, intermediate / 2))
    } else {
        (intermediate, std::cmp::max(1, bound_height / 2))
    }
}

fn cap_dimensions(width: u32, height: u32, max_dimension: u32, max_area: u64) -> (u32, u32) {
    let mut w = width.min(max_dimension).max(1);
    let mut h = height.min(max_dimension).max(1);

    let area = u64::from(w) * u64::from(h);
    if area <= max_area {
        return (w, h);
    }

    let scale = (max_area as f64 / area as f64).sqrt();
    w = ((w as f64 * scale).floor() as u32).max(1);
    h = ((h as f64 * scale).floor() as u32).max(1);
    (w, h)
}
