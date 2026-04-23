# niiterm

`niiterm` is a terminal-first NIfTI viewer for the common "I just need to sanity-check this image right now" workflow on SSH sessions, HPC login nodes, compute nodes, and local terminals.

It supports:

- Fast one-shot slice rendering in the terminal
- Interactive slice and volume scrubbing
- RAS reorientation so axial/coronal/sagittal behave consistently
- 4D playback for BOLD, DWI, and ASL-style series
- Modality-aware defaults for windowing, colormap, and interactive sizing
- DWI `.bval` / `.bvec` context in the header
- Header stats suitable for quick QC

Full docs website: [docs/index.html](docs/index.html)

## Install

I recommend installing `niiterm` with a normal Rust toolchain managed by `rustup`, not inside a micromamba environment, unless you specifically want an isolated user-space install on HPC.

### Recommended: rustup + cargo

Install Rust once:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Then, from a local clone of this repo:

```bash
cargo install --path . --locked
```

That installs `niiterm` into `~/.cargo/bin`.

### Micromamba / HPC-friendly option

If you want `niiterm` fully contained inside a micromamba environment:

```bash
micromamba create -n niiterm -c conda-forge rust
micromamba activate niiterm
cargo install --path . --locked --root "$CONDA_PREFIX"
```

If `micromamba activate` is not set up in your shell yet:

```bash
micromamba shell init -s zsh -r ~/micromamba
exec zsh
```

### Updating an existing install

After pulling new changes, rerun the same install command you used before:

```bash
cargo install --path . --locked
```

For a micromamba-rooted install:

```bash
cargo install --path . --locked --root "$CONDA_PREFIX"
```

If you want to force replacement of an existing binary:

```bash
cargo install --path . --locked --force
```

Or inside micromamba:

```bash
cargo install --path . --locked --root "$CONDA_PREFIX" --force
```

## Terminal Compatibility

`niiterm` works best in terminals with a supported graphics protocol.

| Terminal | Status | Notes |
| --- | --- | --- |
| WezTerm | Explicitly tested | Works well. On remote/HPC SSH sessions, interactive mode may need `--protocol iterm` if auto-detection picks `kitty`. |
| iTerm2 | Expected to work | Intended to work with graphics rendering, but not explicitly regression-tested in the current manual pass. |
| Kitty | Expected to work | Intended to work through the kitty graphics protocol, but not explicitly regression-tested in the current manual pass. |
| Ghostty | Expected to work | Likely fine through kitty-compatible rendering, but not explicitly regression-tested in the current manual pass. |
| Sixel-capable terminals | Partially supported | One-shot sixel output is supported. Interactive behavior is less thoroughly tested. |
| Apple Terminal.app | Limited fallback | Falls back to block rendering only, so output is usable for rough QC but visibly lower resolution. |
| Generic SSH terminals with no graphics protocol | Limited fallback | Falls back to block rendering only. |

## Quick Start

### One-shot examples

```bash
niiterm sub-01_T1w.nii.gz
niiterm --axis sagittal --slice 72 sub-01_T1w.nii.gz
niiterm --coord 90,110,76 sub-01_T1w.nii.gz
niiterm --window p1,p99 --colormap turbo sub-01_cbf.nii.gz
niiterm --protocol blocks sub-01_T1w.nii.gz
```

### Interactive examples

```bash
niiterm --interactive sub-01_T1w.nii.gz
niiterm --interactive --play sub-01_task-rest_bold.nii.gz
niiterm --interactive --protocol iterm sub-01_task-rest_bold.nii.gz
niiterm --interactive --volume 12 sub-01_dwi.nii.gz
```

### WezTerm on HPC / remote SSH

If WezTerm interactive mode shows `proto=kitty` and renders placeholder blocks instead of an image, force the protocol:

```bash
niiterm --interactive --protocol iterm sub-01_task-rest_bold.nii.gz
```

## Interactive Controls

- `Left` / `Right` or `h` / `l`: previous / next slice
- `Up` / `Down` or `j` / `k`: move slice by 10
- `H` / `L`: previous / next 4D volume
- `a`: cycle axis
- `space`: play / pause 4D series
- `+` / `-`: increase / decrease playback FPS
- `c`: cycle colormap
- `w`: cycle window preset
- `z`: cycle interactive size (`native`, `comfortable`, `large`)
- `b`: cycle playback rendering tradeoff (`auto`, `smooth`, `detail`)
- `g`: jump to the middle slice
- `?`: toggle help
- `q` or `esc`: quit

## Important Notes

- `niiterm` reorients loaded data to RAS without resampling, so axis semantics stay stable across files.
- `--coord` is interpreted in reoriented voxel coordinates.
- `--mm` is interpreted in world-space millimeters using the file affine and then mapped into the reoriented array.
- DWI gradient metadata is loaded from sibling `<stem>.bval` and `<stem>.bvec` files when present.
- `--width` affects one-shot rendering only. In the interactive viewer, use `z` to change the display size.
- In interactive 4D playback, `b` lets you trade spatial detail for smoother animation.

## Media Placeholders

Save screenshots and movies under `docs/assets/` so they are available to both the README and the docs site.

- Screenshots: `docs/assets/screenshots/`
- Movies or short clips: `docs/assets/movies/`

Suggested filenames:

- `docs/assets/screenshots/wezterm-interactive-t1.png`
- `docs/assets/screenshots/wezterm-interactive-bold.png`
- `docs/assets/screenshots/apple-terminal-blocks-t1.png`
- `docs/assets/movies/wezterm-bold-playback.mp4`
- `docs/assets/movies/wezterm-dwi-playback.mp4`

There is a companion note with the same instructions in [docs/assets/README.md](docs/assets/README.md).

## CLI Help

The built-in help includes examples and terminal notes:

```bash
niiterm --help
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
