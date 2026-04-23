use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    StatefulImage,
};

use crate::cli::{Args, Axis, Colormap, Protocol};
use crate::dwi::{self, DwiMetadata};
use crate::modality::Modality;
use crate::nifti_io::{load_nifti, LoadedVolume};
use crate::render::{extract_slice, render_slice_image};
use crate::stats::format_stats_line;
use crate::windowing::{WindowCache, WindowMode, WindowPreset};

pub struct AppState {
    pub volume: LoadedVolume,
    pub modality: Modality,
    pub dwi: Option<DwiMetadata>,
    pub axis: Axis,
    pub slice: usize,
    pub volume_index: usize,
    pub playing: bool,
    pub fps: u16,
    pub colormap: Colormap,
    pub window_mode: WindowMode,
    pub window_preset_index: usize,
    pub image: StatefulProtocol,
    pub picker: Picker,
    pub should_quit: bool,
    pub show_help: bool,
    pub window_cache: WindowCache,
}

impl AppState {
    pub fn new(args: Args) -> Result<Self> {
        let volume = load_nifti(&args.file)?;
        let modality = Modality::detect(&args.file);
        let dwi = if modality == Modality::Dwi {
            dwi::load_with_warning(&args.file)
        } else {
            None
        };

        let axis = args.axis;
        let volume_index = volume.clamp_volume(args.volume);
        let mm_coord = args.mm.and_then(|coord| {
            volume.ras_index_from_mm([coord.x as f64, coord.y as f64, coord.z as f64])
        });
        let slice = args
            .slice
            .or_else(|| args.coord.map(|coord| coord.component_for_axis(axis)))
            .or_else(|| mm_coord.map(|coord| coord[axis.index()]))
            .unwrap_or_else(|| volume.middle_slice(axis.index()));

        let colormap = args.colormap.unwrap_or_else(|| modality.default_colormap());
        let window_mode = args
            .window
            .as_deref()
            .map(str::parse::<WindowMode>)
            .transpose()?
            .unwrap_or_else(|| modality.default_window());

        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        apply_protocol_override(&mut picker, args.protocol);

        let mut window_cache = WindowCache::default();
        let initial = build_image(
            &volume,
            axis,
            slice,
            volume_index,
            colormap,
            window_mode,
            &mut window_cache,
        );
        let image = picker.new_resize_protocol(initial);

        Ok(Self {
            volume,
            modality,
            dwi,
            axis,
            slice,
            volume_index,
            playing: args.play,
            fps: args.fps.max(1),
            colormap,
            window_mode,
            window_preset_index: preset_index(window_mode),
            image,
            picker,
            should_quit: false,
            show_help: false,
            window_cache,
        })
    }

    pub fn status_line(&self) -> String {
        let mut line = format_stats_line(
            &self.volume,
            self.modality,
            self.volume_index,
            self.dwi.as_ref(),
        );
        line.push_str(&format!(
            "  axis={} slice={} cmap={} window={}",
            self.axis.label(),
            self.slice,
            self.colormap.label(),
            self.window_mode
        ));
        line
    }

    pub fn controls_hint(&self) -> &'static str {
        "h/l slices  j/k +/-10  H/L volumes  a axis  c colormap  w window  space play  ? help  q quit"
    }

    pub fn poll_timeout(&self, elapsed: Duration) -> Duration {
        if !self.playing || self.volume.nvols() <= 1 {
            return Duration::from_millis(250);
        }

        let frame = Duration::from_secs_f32(1.0 / self.fps.max(1) as f32);
        frame.saturating_sub(elapsed)
    }

    pub fn should_advance(&self, elapsed: Duration) -> bool {
        self.playing
            && self.volume.nvols() > 1
            && elapsed >= Duration::from_secs_f32(1.0 / self.fps.max(1) as f32)
    }

    pub fn advance_playback(&mut self) -> Result<()> {
        if self.volume.nvols() <= 1 {
            return Ok(());
        }
        self.volume_index = (self.volume_index + 1) % self.volume.nvols();
        self.refresh_image()
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Left | KeyCode::Char('h') => self.step_slice(-1)?,
            KeyCode::Right | KeyCode::Char('l') => self.step_slice(1)?,
            KeyCode::Up | KeyCode::Char('k') => self.step_slice(-10)?,
            KeyCode::Down | KeyCode::Char('j') => self.step_slice(10)?,
            KeyCode::Char('H') => self.step_volume(-1)?,
            KeyCode::Char('L') => self.step_volume(1)?,
            KeyCode::Char('a') => {
                self.axis = self.axis.next();
                self.slice = self.volume.middle_slice(self.axis.index());
                self.refresh_image()?;
            }
            KeyCode::Char(' ') => self.playing = !self.playing,
            KeyCode::Char('+') | KeyCode::Char('=') => self.fps = (self.fps + 1).min(60),
            KeyCode::Char('-') => self.fps = self.fps.saturating_sub(1).max(1),
            KeyCode::Char('c') => {
                self.colormap = self.colormap.next();
                self.refresh_image()?;
            }
            KeyCode::Char('w') => {
                self.window_preset_index = (self.window_preset_index + 1) % WindowPreset::ALL.len();
                self.window_mode = WindowPreset::ALL[self.window_preset_index].to_mode();
                self.refresh_image()?;
            }
            KeyCode::Char('g') => {
                self.slice = self.volume.middle_slice(self.axis.index());
                self.refresh_image()?;
            }
            KeyCode::Char('?') => self.show_help = !self.show_help,
            _ => {}
        }

        Ok(())
    }

    pub fn check_encoding_result(&mut self) -> Result<()> {
        if let Some(result) = self.image.last_encoding_result() {
            result?;
        }
        Ok(())
    }

    pub fn image_widget(&self) -> StatefulImage<StatefulProtocol> {
        StatefulImage::default()
    }

    fn step_slice(&mut self, delta: isize) -> Result<()> {
        let limit = self.volume.axis_len(self.axis.index()).saturating_sub(1) as isize;
        let next = (self.slice as isize + delta).clamp(0, limit);
        self.slice = next as usize;
        self.refresh_image()
    }

    fn step_volume(&mut self, delta: isize) -> Result<()> {
        let limit = self.volume.nvols().saturating_sub(1) as isize;
        let next = (self.volume_index as isize + delta).clamp(0, limit);
        self.volume_index = next as usize;
        self.refresh_image()
    }

    fn refresh_image(&mut self) -> Result<()> {
        let next = build_image(
            &self.volume,
            self.axis,
            self.slice,
            self.volume_index,
            self.colormap,
            self.window_mode,
            &mut self.window_cache,
        );
        self.image = self.picker.new_resize_protocol(next);
        Ok(())
    }
}

fn build_image(
    volume: &LoadedVolume,
    axis: Axis,
    slice: usize,
    volume_index: usize,
    colormap: Colormap,
    window_mode: WindowMode,
    cache: &mut WindowCache,
) -> image::DynamicImage {
    let current_window = cache.get_or_insert(
        volume_index,
        window_mode,
        volume.data.slice(ndarray::s![.., .., .., volume_index]),
    );
    let slice_data = extract_slice(volume, axis, slice, volume_index);
    render_slice_image(&slice_data, axis, volume.pixdim, colormap, current_window)
}

fn apply_protocol_override(picker: &mut Picker, protocol: Protocol) {
    let protocol_type = match protocol {
        Protocol::Auto => return,
        Protocol::Kitty => ProtocolType::Kitty,
        Protocol::Iterm => ProtocolType::Iterm2,
        Protocol::Sixel => ProtocolType::Sixel,
        Protocol::Blocks => ProtocolType::Halfblocks,
    };
    picker.set_protocol_type(protocol_type);
}

fn preset_index(mode: WindowMode) -> usize {
    WindowPreset::ALL
        .iter()
        .position(|preset| preset.to_mode() == mode)
        .unwrap_or(0)
}
