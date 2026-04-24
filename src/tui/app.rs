use std::time::Duration;
use std::{env, ffi::OsString};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    Resize, StatefulImage,
};

use crate::cli::{Args, Axis, Colormap, LayoutMode, Protocol};
use crate::dwi::{self, DwiMetadata};
use crate::modality::Modality;
use crate::nifti_io::{load_nifti, LoadedVolume};
use crate::render::{extract_slice, render_slice_image, render_triptych_image};
use crate::stats::format_stats_line;
use crate::windowing::{WindowCache, WindowMode, WindowPreset};

pub struct AppState {
    pub volume: LoadedVolume,
    pub modality: Modality,
    pub dwi: Option<DwiMetadata>,
    pub layout: LayoutMode,
    pub active_axis: Axis,
    pub cursor: [usize; 3],
    pub volume_index: usize,
    pub playing: bool,
    pub fps: u16,
    pub colormap: Colormap,
    pub window_mode: WindowMode,
    pub window_preset_index: usize,
    pub size_mode: SizeMode,
    pub playback_render_mode: PlaybackRenderMode,
    pub image: StatefulProtocol,
    pub picker: Picker,
    pub preferred_protocol_type: ProtocolType,
    pub protocol_type: ProtocolType,
    pub should_quit: bool,
    pub show_help: bool,
    pub window_cache: WindowCache,
}

#[derive(Debug, Clone, Copy)]
struct RenderOptions {
    layout: LayoutMode,
    active_axis: Axis,
    cursor: [usize; 3],
    volume_index: usize,
    colormap: Colormap,
    window_mode: WindowMode,
    size_mode: SizeMode,
}

impl AppState {
    pub fn build_picker(protocol: Protocol) -> Picker {
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        apply_protocol_override(&mut picker, protocol);

        if matches!(protocol, Protocol::Auto) {
            if is_wezterm() {
                picker.set_protocol_type(ProtocolType::Iterm2);
            } else if is_apple_terminal() {
                picker.set_protocol_type(ProtocolType::Halfblocks);
            }
        }

        picker
    }

    pub fn new(args: Args, mut picker: Picker) -> Result<Self> {
        let volume = load_nifti(&args.file)?;
        let modality = Modality::detect(&args.file);
        let dwi = if modality == Modality::Dwi {
            dwi::load_with_warning(&args.file)
        } else {
            None
        };

        let layout = args.layout_mode();
        let active_axis = args.axis;
        let volume_index = volume.clamp_volume(args.volume);
        let cursor = initial_cursor(&args, &volume, active_axis);

        let colormap = args.colormap.unwrap_or_else(|| modality.default_colormap());
        let window_mode = args
            .window
            .as_deref()
            .map(str::parse::<WindowMode>)
            .transpose()?
            .unwrap_or_else(|| modality.default_window());
        let size_mode = SizeMode::default_for_modality(modality);
        let playback_render_mode = PlaybackRenderMode::Auto;

        apply_protocol_override(&mut picker, args.protocol);
        let preferred_protocol_type = picker.protocol_type();

        let mut window_cache = WindowCache::default();
        let initial = build_image(
            &volume,
            RenderOptions {
                layout,
                active_axis,
                cursor,
                volume_index,
                colormap,
                window_mode,
                size_mode,
            },
            &mut window_cache,
        );
        picker.set_protocol_type(effective_protocol_type(
            preferred_protocol_type,
            playback_render_mode,
            args.play,
            volume.nvols(),
            size_mode,
        ));
        let image = picker.new_resize_protocol(initial);
        let protocol_type = picker.protocol_type();

        Ok(Self {
            volume,
            modality,
            dwi,
            layout,
            active_axis,
            cursor,
            volume_index,
            playing: args.play,
            fps: args.fps.max(1),
            colormap,
            window_mode,
            window_preset_index: preset_index(window_mode),
            size_mode,
            playback_render_mode,
            image,
            picker,
            preferred_protocol_type,
            protocol_type,
            should_quit: false,
            show_help: false,
            window_cache,
        })
    }

    pub fn header_lines(&self) -> Vec<String> {
        let mut lines = vec![format_stats_line(
            &self.volume,
            self.modality,
            self.volume_index,
            self.dwi.as_ref(),
        )];

        let mut view_line = match self.layout {
            LayoutMode::Single => format!(
                "view layout=single axis={} slice={} cmap={} window={} size={} proto={}",
                self.active_axis.label(),
                self.slice_for_axis(self.active_axis),
                self.colormap.label(),
                self.window_mode,
                self.size_mode.label(),
                protocol_label(self.protocol_type)
            ),
            LayoutMode::Triptych => format!(
                "view layout=triptych active={} sag={} ax={} cor={} cmap={} window={} size={} proto={}",
                self.active_axis.label(),
                self.slice_for_axis(Axis::Sagittal),
                self.slice_for_axis(Axis::Axial),
                self.slice_for_axis(Axis::Coronal),
                self.colormap.label(),
                self.window_mode,
                self.size_mode.label(),
                protocol_label(self.protocol_type)
            ),
        };

        if self.volume.nvols() > 1 {
            view_line.push_str(&format!(
                " fps={} play={} playmode={}",
                self.fps,
                if self.playing { "on" } else { "paused" },
                self.playback_render_mode.label()
            ));
        }

        lines.push(view_line);
        lines
    }

    pub fn controls_hint(&self) -> &'static str {
        match self.layout {
            LayoutMode::Single => {
                "h/l slices  j/k +/-10  H/L volumes  a axis  c colormap  w window  z size  b playmode  space play  ? help  q quit"
            }
            LayoutMode::Triptych => {
                "h/l slices  j/k +/-10  tab/a pane  H/L volumes  c colormap  w window  z size  b playmode  space play  g center  ? help  q quit"
            }
        }
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
            KeyCode::Left | KeyCode::Char('h') => self.step_active_slice(-1)?,
            KeyCode::Right | KeyCode::Char('l') => self.step_active_slice(1)?,
            KeyCode::Up | KeyCode::Char('k') => self.step_active_slice(-10)?,
            KeyCode::Down | KeyCode::Char('j') => self.step_active_slice(10)?,
            KeyCode::Tab => self.cycle_active_pane()?,
            KeyCode::Char('H') => self.step_volume(-1)?,
            KeyCode::Char('L') => self.step_volume(1)?,
            KeyCode::Char('a') => {
                if self.layout == LayoutMode::Triptych {
                    self.cycle_active_pane()?;
                } else {
                    self.active_axis = self.active_axis.next();
                    self.cursor[self.active_axis.index()] =
                        self.volume.middle_slice(self.active_axis.index());
                    self.refresh_image()?;
                }
            }
            KeyCode::Char(' ') => {
                self.playing = !self.playing;
                self.refresh_image()?;
            }
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
            KeyCode::Char('z') => {
                self.size_mode = self.size_mode.next();
                self.refresh_image()?;
            }
            KeyCode::Char('b') => {
                self.playback_render_mode = self.playback_render_mode.next();
                self.refresh_image()?;
            }
            KeyCode::Char('g') => {
                if self.layout == LayoutMode::Triptych {
                    self.cursor = middle_cursor(&self.volume);
                } else {
                    self.cursor[self.active_axis.index()] =
                        self.volume.middle_slice(self.active_axis.index());
                }
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
        StatefulImage::new().resize(Resize::Fit(None))
    }

    fn slice_for_axis(&self, axis: Axis) -> usize {
        self.cursor[axis.index()]
    }

    fn cycle_active_pane(&mut self) -> Result<()> {
        self.active_axis = self.active_axis.next();
        if self.layout == LayoutMode::Triptych {
            return Ok(());
        }
        self.refresh_image()
    }

    fn step_active_slice(&mut self, delta: isize) -> Result<()> {
        let axis = self.active_axis;
        let limit = self.volume.axis_len(axis.index()).saturating_sub(1) as isize;
        let next = (self.slice_for_axis(axis) as isize + delta).clamp(0, limit);
        self.cursor[axis.index()] = next as usize;
        self.refresh_image()
    }

    fn step_volume(&mut self, delta: isize) -> Result<()> {
        let limit = self.volume.nvols().saturating_sub(1) as isize;
        let next = (self.volume_index as isize + delta).clamp(0, limit);
        self.volume_index = next as usize;
        self.refresh_image()
    }

    fn refresh_image(&mut self) -> Result<()> {
        self.sync_picker_protocol();
        let next = build_image(
            &self.volume,
            RenderOptions {
                layout: self.layout,
                active_axis: self.active_axis,
                cursor: self.cursor,
                volume_index: self.volume_index,
                colormap: self.colormap,
                window_mode: self.window_mode,
                size_mode: self.size_mode,
            },
            &mut self.window_cache,
        );
        self.image = self.picker.new_resize_protocol(next);
        self.protocol_type = self.picker.protocol_type();
        Ok(())
    }

    fn sync_picker_protocol(&mut self) {
        self.picker.set_protocol_type(effective_protocol_type(
            self.preferred_protocol_type,
            self.playback_render_mode,
            self.playing,
            self.volume.nvols(),
            self.size_mode,
        ));
    }
}

fn initial_cursor(args: &Args, volume: &LoadedVolume, active_axis: Axis) -> [usize; 3] {
    if let Some(coord) = args.coord {
        return coord.ras_indices([volume.dims[0], volume.dims[1], volume.dims[2]]);
    }

    if let Some(coord) = args.mm.and_then(|coord| {
        volume.ras_index_from_mm([coord.x as f64, coord.y as f64, coord.z as f64])
    }) {
        return coord;
    }

    let mut cursor = middle_cursor(volume);
    if let Some(slice) = args.slice {
        cursor[active_axis.index()] = slice.resolve(volume.axis_len(active_axis.index()));
    }
    cursor
}

fn middle_cursor(volume: &LoadedVolume) -> [usize; 3] {
    [
        volume.middle_slice(0),
        volume.middle_slice(1),
        volume.middle_slice(2),
    ]
}

fn build_image(
    volume: &LoadedVolume,
    options: RenderOptions,
    cache: &mut WindowCache,
) -> image::DynamicImage {
    let current_window = cache.get_or_insert(
        options.volume_index,
        options.window_mode,
        volume
            .data
            .slice(ndarray::s![.., .., .., options.volume_index]),
    );

    let image = match options.layout {
        LayoutMode::Single => {
            let slice_data = extract_slice(
                volume,
                options.active_axis,
                options.cursor[options.active_axis.index()],
                options.volume_index,
            );
            render_slice_image(
                &slice_data,
                options.active_axis,
                volume.pixdim,
                options.colormap,
                current_window,
            )
        }
        LayoutMode::Triptych => render_triptych_image(
            volume,
            options.cursor,
            options.volume_index,
            options.colormap,
            current_window,
        ),
    };

    upscale_for_tui(image, options.size_mode)
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

fn protocol_label(protocol: ProtocolType) -> &'static str {
    match protocol {
        ProtocolType::Halfblocks => "blocks",
        ProtocolType::Sixel => "sixel",
        ProtocolType::Kitty => "kitty",
        ProtocolType::Iterm2 => "iterm2",
    }
}

fn effective_protocol_type(
    preferred: ProtocolType,
    playback_render_mode: PlaybackRenderMode,
    playing: bool,
    nvols: usize,
    size_mode: SizeMode,
) -> ProtocolType {
    if !playing || nvols <= 1 {
        return preferred;
    }

    match playback_render_mode {
        PlaybackRenderMode::Smooth => ProtocolType::Halfblocks,
        PlaybackRenderMode::Detail => preferred,
        PlaybackRenderMode::Auto => match size_mode {
            SizeMode::Native => preferred,
            SizeMode::Comfortable | SizeMode::Large => ProtocolType::Halfblocks,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeMode {
    Native,
    Comfortable,
    Large,
}

impl SizeMode {
    fn default_for_modality(modality: Modality) -> Self {
        match modality {
            Modality::Bold | Modality::Dwi | Modality::Asl => Self::Large,
            Modality::T1 | Modality::T2 => Self::Comfortable,
            Modality::Unknown => Self::Comfortable,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Native => Self::Comfortable,
            Self::Comfortable => Self::Large,
            Self::Large => Self::Native,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Comfortable => "comfortable",
            Self::Large => "large",
        }
    }

    fn target_min_dimension(self) -> Option<u32> {
        match self {
            Self::Native => None,
            Self::Comfortable => Some(256),
            Self::Large => Some(384),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackRenderMode {
    Auto,
    Smooth,
    Detail,
}

impl PlaybackRenderMode {
    fn next(self) -> Self {
        match self {
            Self::Auto => Self::Smooth,
            Self::Smooth => Self::Detail,
            Self::Detail => Self::Auto,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Smooth => "smooth",
            Self::Detail => "detail",
        }
    }
}

fn upscale_for_tui(image: image::DynamicImage, size_mode: SizeMode) -> image::DynamicImage {
    let Some(target_min) = size_mode.target_min_dimension() else {
        return image;
    };

    let width = image.width().max(1);
    let height = image.height().max(1);
    let min_dim = width.min(height);
    if min_dim >= target_min {
        return image;
    }

    const MAX_DIMENSION: u32 = 1024;

    let scale = target_min as f32 / min_dim as f32;
    let mut target_width = ((width as f32 * scale).round() as u32).max(1);
    let mut target_height = ((height as f32 * scale).round() as u32).max(1);

    if target_width > MAX_DIMENSION || target_height > MAX_DIMENSION {
        let downscale = f32::min(
            MAX_DIMENSION as f32 / target_width as f32,
            MAX_DIMENSION as f32 / target_height as f32,
        );
        target_width = ((target_width as f32 * downscale).round() as u32).max(1);
        target_height = ((target_height as f32 * downscale).round() as u32).max(1);
    }

    image.resize_exact(
        target_width,
        target_height,
        image::imageops::FilterType::Lanczos3,
    )
}

fn is_wezterm() -> bool {
    env_has_nonempty("WEZTERM_EXECUTABLE")
        || env_var_contains("TERM_PROGRAM", "WezTerm")
        || env_var_contains("LC_TERMINAL", "WezTerm")
}

fn is_apple_terminal() -> bool {
    env_var_contains("TERM_PROGRAM", "Apple_Terminal")
        || env_var_contains("LC_TERMINAL", "Apple_Terminal")
}

fn env_has_nonempty(key: &str) -> bool {
    env::var_os(key).is_some_and(|value| !value.is_empty())
}

fn env_var_contains(key: &str, needle: &str) -> bool {
    env::var_os(key)
        .and_then(os_string_to_string)
        .is_some_and(|value| value.contains(needle))
}

fn os_string_to_string(value: OsString) -> Option<String> {
    value.into_string().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_mode_cycles_through_all_variants() {
        assert_eq!(SizeMode::Native.next(), SizeMode::Comfortable);
        assert_eq!(SizeMode::Comfortable.next(), SizeMode::Large);
        assert_eq!(SizeMode::Large.next(), SizeMode::Native);
    }

    #[test]
    fn playback_prefers_halfblocks_for_4d_series() {
        assert_eq!(
            effective_protocol_type(
                ProtocolType::Iterm2,
                PlaybackRenderMode::Smooth,
                true,
                1200,
                SizeMode::Large,
            ),
            ProtocolType::Halfblocks
        );
        assert_eq!(
            effective_protocol_type(
                ProtocolType::Kitty,
                PlaybackRenderMode::Smooth,
                true,
                1200,
                SizeMode::Large,
            ),
            ProtocolType::Halfblocks
        );
        assert_eq!(
            effective_protocol_type(
                ProtocolType::Iterm2,
                PlaybackRenderMode::Smooth,
                false,
                1200,
                SizeMode::Large,
            ),
            ProtocolType::Iterm2
        );
        assert_eq!(
            effective_protocol_type(
                ProtocolType::Iterm2,
                PlaybackRenderMode::Smooth,
                true,
                1,
                SizeMode::Large,
            ),
            ProtocolType::Iterm2
        );
    }

    #[test]
    fn auto_and_detail_modes_preserve_more_detail_when_requested() {
        assert_eq!(
            effective_protocol_type(
                ProtocolType::Iterm2,
                PlaybackRenderMode::Auto,
                true,
                1200,
                SizeMode::Native,
            ),
            ProtocolType::Iterm2
        );
        assert_eq!(
            effective_protocol_type(
                ProtocolType::Iterm2,
                PlaybackRenderMode::Auto,
                true,
                1200,
                SizeMode::Large,
            ),
            ProtocolType::Halfblocks
        );
        assert_eq!(
            effective_protocol_type(
                ProtocolType::Iterm2,
                PlaybackRenderMode::Detail,
                true,
                1200,
                SizeMode::Large,
            ),
            ProtocolType::Iterm2
        );
    }

    #[test]
    fn middle_cursor_uses_each_axis_midpoint() {
        let volume = LoadedVolume {
            path: "test.nii.gz".into(),
            header: nifti::NiftiHeader::default(),
            data: ndarray::Array4::zeros((9, 11, 13, 1)),
            dims: [9, 11, 13, 1],
            pixdim: [1.0, 1.0, 1.0, 1.0],
            dtype: "float32".to_string(),
            affine: nalgebra::Matrix4::identity(),
            inverse_affine: Some(nalgebra::Matrix4::identity()),
            reorientation: crate::nifti_io::Reorientation {
                perm: [0, 1, 2],
                flip: [false, false, false],
                original_dims: [9, 11, 13],
            },
            source_orientation: "RAS".to_string(),
            display_orientation: "RAS".to_string(),
            range: (0.0, 0.0),
            nan_count: 0,
            warnings: Vec::new(),
        };

        assert_eq!(middle_cursor(&volume), [4, 5, 6]);
    }
}
