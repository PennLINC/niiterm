use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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
#[command(name = "niiterm", version, about = "PennLINC NIfTI terminal viewer")]
pub struct Args {
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    #[arg(short = 'i', long = "interactive")]
    pub interactive: bool,

    #[arg(short = 'a', long = "axis", default_value = "axial")]
    pub axis: Axis,

    #[arg(short = 's', long = "slice")]
    pub slice: Option<usize>,

    #[arg(long = "coord", conflicts_with = "mm")]
    pub coord: Option<Coord3>,

    #[arg(long = "mm", conflicts_with = "coord")]
    pub mm: Option<Coord3>,

    #[arg(short = 't', long = "volume", default_value_t = 0)]
    pub volume: usize,

    #[arg(long = "play")]
    pub play: bool,

    #[arg(long = "fps", default_value_t = 10)]
    pub fps: u16,

    #[arg(short = 'm', long = "colormap")]
    pub colormap: Option<Colormap>,

    #[arg(short = 'w', long = "window")]
    pub window: Option<String>,

    #[arg(long = "width")]
    pub width: Option<u32>,

    #[arg(long = "protocol", default_value = "auto")]
    pub protocol: Protocol,

    #[arg(long = "stats", default_value_t = true, action = clap::ArgAction::SetTrue)]
    pub stats: bool,

    #[arg(long = "no-stats", overrides_with = "stats")]
    pub no_stats: bool,

    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbose: u8,
}

impl Args {
    pub fn parse_args() -> Self {
        Self::parse()
    }

    pub fn show_stats(&self) -> bool {
        !self.no_stats
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
