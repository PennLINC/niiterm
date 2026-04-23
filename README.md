# niiterm

`niiterm` is a PennLINC-oriented terminal viewer for NIfTI files. It is meant for the common "I just need to sanity-check this image right now" workflow on SSH sessions, HPC login nodes, or compute nodes where desktop viewers are unavailable.

It supports:

- Fast one-shot slice rendering in the terminal
- Interactive slice and volume scrubbing
- RAS reorientation so axial/coronal/sagittal behave consistently
- 4D playback for BOLD/DWI/ASL-style series
- Modality-aware defaults for colormap and windowing
- DWI `.bval` / `.bvec` context in the status line
- Header and data stats suitable for quick QC

## Install

I recommend installing `niiterm` with a normal Rust toolchain managed by `rustup`, not inside a micromamba environment, unless you specifically want an isolated user-space install on HPC.

Why:

- `niiterm` is a Rust CLI, not a Python package
- `cargo install` is the native install path
- a plain Rust install keeps startup and shell integration simple
- micromamba is still a good option on shared systems where you do not want to touch your base shell setup

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

That installs `niiterm` into `~/.cargo/bin`, which should be on your `PATH`.

### Micromamba / HPC-friendly option

If you want `niiterm` fully contained inside a micromamba environment, that works too:

```bash
micromamba create -n niiterm -c conda-forge rust
micromamba activate niiterm
cargo install --path . --locked --root "$CONDA_PREFIX"
```

That puts the `niiterm` binary under the active environment instead of your global cargo bin directory.

If `micromamba activate` is not set up in your shell yet, initialize it first:

```bash
micromamba shell init -s zsh -r ~/micromamba
exec zsh
```

### Which one should I use?

- Use `rustup` if this is your laptop/workstation and you are happy to have Rust tools available generally.
- Use micromamba if you want a self-contained install, you are on an HPC system, or you already manage CLI tools that way.

### Verify

```bash
niiterm --help
```

## Usage

```bash
niiterm sub-01_T1w.nii.gz
niiterm --axis sagittal --slice 72 sub-01_T1w.nii.gz
niiterm --coord 90,110,76 sub-01_T1w.nii.gz
niiterm --interactive --play sub-01_task-rest_bold.nii.gz
niiterm --interactive --volume 12 sub-01_dwi.nii.gz
niiterm --protocol blocks sub-01_T1w.nii.gz
```

## Controls

- `Left` / `Right` or `h` / `l`: previous / next slice
- `Up` / `Down` or `k` / `j`: move slice by 10
- `H` / `L`: previous / next 4D volume
- `a`: cycle axis
- `space`: play / pause 4D series
- `+` / `-`: increase / decrease FPS
- `c`: cycle colormap
- `w`: cycle window preset
- `g`: jump to the middle slice
- `?`: toggle help
- `q` or `esc`: quit

## Notes

- `niiterm` reorients loaded data to RAS without resampling, so axis semantics stay stable across files.
- `--coord` is interpreted in reoriented voxel coordinates.
- `--mm` is interpreted in world-space millimeters using the file affine and then mapped into the reoriented array.
- DWI gradient metadata is loaded from sibling `<stem>.bval` and `<stem>.bvec` files when present.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
