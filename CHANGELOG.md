# Changelog

smashbin is MIT licensed. The terms are in [LICENSE](LICENSE). Copyright (c) 2026 Stuart.

## 0.1.4

- `--dpi` sets the stored pixels per inch. The pixel grid stays the same. JPEG and PNG store it in the file. WebP stores it in an EXIF block. A container with no density field is still written, and stderr names the original image's density.
- `--ext` / `-e` writes `jpg`, `png`, `gif`, `webp`, `bmp`, `tif`, `tga`, `ico`, `qoi`, `ppm`, `hdr`, or `ff`. WebP is lossless.
- A written image exits 0. The report does not score the file.
- The generated file name adds `dpi` after `height`: `strip`, `width`, `height`, `dpi`, `quality`, `bytes`.

## 0.1.3

- A `smasher.json` or `smasher.yaml` next to the image runs each job from top to bottom.
- `-c` / `--config` names a smasher file that is not next to the image.
- The JSON and YAML readers are in the program. No parser crate was added.

## 0.1.2

- `--width` / `-w` and `--height` / `-h` resize the picture. The file stays a PNG, JPEG, or GIF.
- A missing side keeps the current aspect ratio. Both sides together use those exact pixels.
- The resize is written first. `--quality` or `--bytes` then replaces that result with a new JPEG.
- The generated file name stays the original stem plus `_` and the inputs, in this fixed order: `strip`, `width`, `height`, `quality`, `bytes`.

## 0.1.0

- First version. Strip metadata and shrink a PNG without changing pixels.
- `--quality` and `--bytes` re-encode a JPEG. They cannot be combined.
