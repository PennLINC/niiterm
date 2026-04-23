use anyhow::{Context, Result};
use image::DynamicImage;
use viuer::Config as ViuerConfig;

use crate::cli::{Args, Protocol};
use crate::dwi;
use crate::modality::Modality;
use crate::nifti_io::load_nifti;
use crate::render::{extract_slice, render_slice_image};
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
    let ras_coord = args.mm.and_then(|coord| {
        volume.ras_index_from_mm([coord.x as f64, coord.y as f64, coord.z as f64])
    });
    let slice_index = args
        .slice
        .or_else(|| args.coord.map(|coord| coord.component_for_axis(args.axis)))
        .or_else(|| ras_coord.map(|coord| coord[args.axis.index()]))
        .unwrap_or_else(|| volume.middle_slice(args.axis.index()));

    let slice = extract_slice(&volume, args.axis, slice_index, volume_index);
    let mut cache = WindowCache::default();
    let window = cache.get_or_insert(
        volume_index,
        window_mode,
        volume.data.slice(ndarray::s![.., .., .., volume_index]),
    );
    let image = render_slice_image(&slice, args.axis, volume.pixdim, colormap, window);

    if args.show_stats() {
        println!(
            "{}",
            format_stats_line(&volume, modality, volume_index, dwi.as_ref())
        );
    }

    print_image(&image, args.width, args.protocol).context("failed to render image to terminal")
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
