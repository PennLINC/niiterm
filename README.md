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

```bash
cargo install --path .
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
