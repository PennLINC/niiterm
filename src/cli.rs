use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use tracing_subscriber::EnvFilter;

const LONG_ABOUT: &str = "\
View NIfTI volumes directly in the terminal for fast neuroimaging QC.

niiterm supports a quick one-shot mode for printing a single slice, a static
mid-slice QC snapshot, and an interactive mode for scrubbing slices, stepping
through 4D data, or browsing linked tri-planar views over SSH, on HPC nodes,
or on local terminals with image protocol support.";

const AFTER_LONG_HELP: &str = "\
Examples:
  niiterm sub-01_T1w.nii.gz
  niiterm --axis sag --slice 25% sub-01_T1w.nii.gz
  niiterm --axis z --slice 0.25 sub-01_T1w.nii.gz
  niiterm --snapshot mid3 --width 50% sub-01_T1w.nii.gz
  niiterm --interactive --layout triptych sub-01_T1w.nii.gz
  niiterm --interactive --protocol iterm sub-01_task-rest_bold.nii.gz
  niiterm --interactive --play --fps 12 sub-01_task-rest_bold.nii.gz

Terminal notes:
  WezTerm works well, but remote/HPC sessions may need --protocol iterm when
  auto-detection picks kitty for the interactive viewer.
  Apple Terminal falls back to block rendering only, so it is usable for rough
  QC but will look lower resolution than WezTerm, iTerm2, Kitty, or sixel-capable
  terminals.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Axial,
    Coronal,
    Sagittal,
}

impl Axis {
    pub fn index(self) -> usize {
        match self {
            Self::Axial => 2,
            Self::Coronal => 1,
            Self::Sagittal => 0,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Axial => Self::Coronal,
            Self::Coronal => Self::Sagittal,
            Self::Sagittal => Self::Axial,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Axial => "axial",
            Self::Coronal => "coronal",
            Self::Sagittal => "sagittal",
        }
    }
}

impl FromStr for Axis {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "axial" | "ax" | "z" => Ok(Self::Axial),
            "coronal" | "cor" | "y" => Ok(Self::Coronal),
            "sagittal" | "sag" | "x" => Ok(Self::Sagittal),
            _ => Err("expected axial/ax/z, coronal/cor/y, or sagittal/sag/x".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Colormap {
    Gray,
    Viridis,
    Magma,
    Turbo,
    Hot,
}

impl Colormap {
    pub fn next(self) -> Self {
        match self {
            Self::Gray => Self::Viridis,
            Self::Viridis => Self::Magma,
            Self::Magma => Self::Turbo,
            Self::Turbo => Self::Hot,
            Self::Hot => Self::Gray,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gray => "gray",
            Self::Viridis => "viridis",
            Self::Magma => "magma",
            Self::Turbo => "turbo",
            Self::Hot => "hot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Protocol {
    Auto,
    Kitty,
    Iterm,
    Sixel,
    Blocks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SnapshotMode {
    Mid3,
}

impl SnapshotMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mid3 => "mid3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LayoutMode {
    Single,
    Triptych,
}

impl LayoutMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Triptych => "triptych",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliceSpec {
    Absolute(usize),
    Relative(f32),
}

impl SliceSpec {
    pub fn resolve(self, axis_len: usize) -> usize {
        if axis_len <= 1 {
            return 0;
        }

        match self {
            Self::Absolute(index) => index.min(axis_len.saturating_sub(1)),
            Self::Relative(fraction) => {
                let max_index = axis_len.saturating_sub(1) as f32;
                (fraction * max_index).round().clamp(0.0, max_index) as usize
            }
        }
    }
}

impl FromStr for SliceSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.trim();
        if value.is_empty() {
            return Err("slice spec cannot be empty".to_string());
        }

        if let Some(percent) = value.strip_suffix('%') {
            let parsed = percent
                .trim()
                .parse::<f32>()
                .map_err(|_| "slice percent must be numeric".to_string())?;
            if !parsed.is_finite() {
                return Err("slice percent must be finite".to_string());
            }
            return Ok(Self::Relative(parsed / 100.0));
        }

        if value.contains('.') {
            let parsed = value
                .parse::<f32>()
                .map_err(|_| "slice fraction must be numeric".to_string())?;
            if !parsed.is_finite() {
                return Err("slice fraction must be finite".to_string());
            }
            return Ok(Self::Relative(parsed));
        }

        let parsed = value.parse::<usize>().map_err(|_| {
            "slice index must be an integer, percent, or decimal fraction".to_string()
        })?;
        Ok(Self::Absolute(parsed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidthSpec {
    Absolute(u32),
    Relative(f32),
}

impl WidthSpec {
    pub fn resolve(self, terminal_width: u32) -> u32 {
        match self {
            Self::Absolute(width) => width.max(1),
            Self::Relative(fraction) => {
                ((terminal_width.max(1) as f32) * fraction).round().max(1.0) as u32
            }
        }
    }
}

impl FromStr for WidthSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.trim();
        if value.is_empty() {
            return Err("width spec cannot be empty".to_string());
        }

        if let Some(percent) = value.strip_suffix('%') {
            let parsed = percent
                .trim()
                .parse::<f32>()
                .map_err(|_| "width percent must be numeric".to_string())?;
            if !parsed.is_finite() {
                return Err("width percent must be finite".to_string());
            }
            if parsed < 0.0 {
                return Err("width percent must be non-negative".to_string());
            }
            return Ok(Self::Relative(parsed / 100.0));
        }

        let parsed = value
            .parse::<u32>()
            .map_err(|_| "width must be an integer column count or percent like 50%".to_string())?;
        Ok(Self::Absolute(parsed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coord3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Coord3 {
    pub fn component_for_axis(self, axis: Axis) -> usize {
        let value = match axis {
            Axis::Sagittal => self.x,
            Axis::Coronal => self.y,
            Axis::Axial => self.z,
        };
        value.round().max(0.0) as usize
    }

    pub fn ras_indices(self, dims: [usize; 3]) -> [usize; 3] {
        [
            clamp_coord(self.x, dims[0]),
            clamp_coord(self.y, dims[1]),
            clamp_coord(self.z, dims[2]),
        ]
    }
}

impl FromStr for Coord3 {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s
            .split(',')
            .map(str::trim)
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "expected three comma-separated numeric values".to_string())?;
        if parts.len() != 3 {
            return Err("expected exactly three comma-separated values".to_string());
        }
        Ok(Self {
            x: parts[0],
            y: parts[1],
            z: parts[2],
        })
    }
}

impl fmt::Display for Coord3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2},{:.2},{:.2}", self.x, self.y, self.z)
    }
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "niiterm",
    version,
    about = "PennLINC NIfTI terminal viewer",
    long_about = LONG_ABOUT,
    after_long_help = AFTER_LONG_HELP
)]
pub struct Args {
    #[arg(
        value_name = "FILE",
        help = "Path to the .nii/.nii.gz volume to inspect."
    )]
    pub file: PathBuf,

    #[arg(
        short = 'i',
        long = "interactive",
        help = "Launch the interactive viewer instead of printing a single slice."
    )]
    pub interactive: bool,

    #[arg(
        short = 'a',
        long = "axis",
        default_value = "axial",
        help = "Initial viewing plane or active pane. Accepts axial/ax/z, coronal/cor/y, sagittal/sag/x."
    )]
    pub axis: Axis,

    #[arg(
        short = 's',
        long = "slice",
        value_name = "SPEC",
        help = "Initial slice as absolute `N`, percent like `25%`, or decimal fraction like `0.25`."
    )]
    pub slice: Option<SliceSpec>,

    #[arg(
        long = "coord",
        conflicts_with = "mm",
        value_name = "X,Y,Z",
        help = "Initial RAS voxel coordinate as comma-separated X,Y,Z."
    )]
    pub coord: Option<Coord3>,

    #[arg(
        long = "mm",
        conflicts_with = "coord",
        value_name = "X,Y,Z",
        help = "Initial world-space coordinate in millimeters, mapped through the affine."
    )]
    pub mm: Option<Coord3>,

    #[arg(
        short = 't',
        long = "volume",
        default_value_t = 0,
        help = "Initial 4D volume index. Ignored for 3D images."
    )]
    pub volume: usize,

    #[arg(
        long = "play",
        help = "Start 4D playback immediately in interactive mode."
    )]
    pub play: bool,

    #[arg(
        long = "fps",
        default_value_t = 10,
        help = "Playback frame rate for interactive 4D playback."
    )]
    pub fps: u16,

    #[arg(
        short = 'm',
        long = "colormap",
        help = "Override the modality-aware default colormap."
    )]
    pub colormap: Option<Colormap>,

    #[arg(
        short = 'w',
        long = "window",
        value_name = "SPEC",
        help = "Window preset as `pLO,pHI`, raw `LO,HI`, or `full`."
    )]
    pub window: Option<String>,

    #[arg(
        long = "width",
        value_name = "SPEC",
        help = "One-shot width as absolute columns or a percent like `50%`."
    )]
    pub width: Option<WidthSpec>,

    #[arg(
        long = "protocol",
        default_value = "auto",
        help = "Rendering protocol. Use `iterm` for WezTerm over SSH/HPC if auto picks kitty."
    )]
    pub protocol: Protocol,

    #[arg(
        long = "snapshot",
        conflicts_with_all = ["interactive", "axis", "slice", "coord", "mm"],
        help = "One-shot QC snapshot mode. `mid3` renders mid sagittal, axial, and coronal panels."
    )]
    pub snapshot: Option<SnapshotMode>,

    #[arg(
        long = "layout",
        requires = "interactive",
        help = "Interactive layout. `triptych` shows linked sagittal/axial/coronal panes; default is single."
    )]
    pub layout: Option<LayoutMode>,

    #[arg(
        long = "stats",
        default_value_t = true,
        action = clap::ArgAction::SetTrue,
        help = "Print the metadata header above one-shot output."
    )]
    pub stats: bool,

    #[arg(
        long = "no-stats",
        overrides_with = "stats",
        help = "Suppress the metadata header above one-shot output."
    )]
    pub no_stats: bool,

    #[arg(
        short = 'v',
        long = "verbose",
        action = clap::ArgAction::Count,
        help = "Increase logging verbosity (`-v` = info, `-vv` = debug)."
    )]
    pub verbose: u8,
}

impl Args {
    pub fn parse_args() -> Self {
        Self::parse()
    }

    pub fn show_stats(&self) -> bool {
        !self.no_stats
    }

    pub fn layout_mode(&self) -> LayoutMode {
        self.layout.unwrap_or(LayoutMode::Single)
    }

    pub fn init_tracing(&self) -> Result<()> {
        let directive = match self.verbose {
            0 => "warn",
            1 => "info",
            _ => "debug",
        };

        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(directive))
            .with_target(false)
            .with_writer(std::io::stderr)
            .try_init()
            .map_err(|error| anyhow!(error.to_string()))?;

        Ok(())
    }
}

fn clamp_coord(value: f32, dim: usize) -> usize {
    if dim <= 1 {
        return 0;
    }

    value.round().clamp(0.0, dim.saturating_sub(1) as f32) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_aliases_parse_as_expected() {
        assert_eq!("axial".parse::<Axis>().unwrap(), Axis::Axial);
        assert_eq!("ax".parse::<Axis>().unwrap(), Axis::Axial);
        assert_eq!("z".parse::<Axis>().unwrap(), Axis::Axial);
        assert_eq!("coronal".parse::<Axis>().unwrap(), Axis::Coronal);
        assert_eq!("cor".parse::<Axis>().unwrap(), Axis::Coronal);
        assert_eq!("y".parse::<Axis>().unwrap(), Axis::Coronal);
        assert_eq!("sagittal".parse::<Axis>().unwrap(), Axis::Sagittal);
        assert_eq!("sag".parse::<Axis>().unwrap(), Axis::Sagittal);
        assert_eq!("x".parse::<Axis>().unwrap(), Axis::Sagittal);
    }

    #[test]
    fn slice_specs_resolve_absolute_and_relative_positions() {
        assert_eq!("25".parse::<SliceSpec>().unwrap(), SliceSpec::Absolute(25));
        assert_eq!(
            "25%".parse::<SliceSpec>().unwrap(),
            SliceSpec::Relative(0.25)
        );
        assert_eq!(
            "0.25".parse::<SliceSpec>().unwrap(),
            SliceSpec::Relative(0.25)
        );
        assert_eq!(SliceSpec::Absolute(25).resolve(100), 25);
        assert_eq!(SliceSpec::Relative(0.25).resolve(101), 25);
        assert_eq!(SliceSpec::Relative(1.5).resolve(100), 99);
        assert_eq!(SliceSpec::Relative(-0.5).resolve(100), 0);
    }

    #[test]
    fn width_specs_resolve_absolute_and_percent_widths() {
        assert_eq!("80".parse::<WidthSpec>().unwrap(), WidthSpec::Absolute(80));
        assert_eq!(
            "50%".parse::<WidthSpec>().unwrap(),
            WidthSpec::Relative(0.5)
        );
        assert_eq!(WidthSpec::Absolute(80).resolve(120), 80);
        assert_eq!(WidthSpec::Relative(0.5).resolve(120), 60);
        assert_eq!(WidthSpec::Relative(1.5).resolve(120), 180);
    }

    #[test]
    fn coord_to_ras_indices_clamps_each_axis() {
        let coord = Coord3 {
            x: 12.2,
            y: -1.0,
            z: 99.0,
        };
        assert_eq!(coord.ras_indices([10, 20, 30]), [9, 0, 29]);
    }
}
