# Changelog

smashbin is MIT licensed. The terms are in [LICENSE](LICENSE). Copyright (c) 2026 Stuart.

## 0.1.2

- `--width` / `-w` and `--height` / `-h` resize the picture. The file stays a PNG, JPEG, or GIF.
- A missing side keeps the current aspect ratio. Both sides together use those exact pixels.
- The resize is written first. `--quality` or `--bytes` then replaces that result with a new JPEG.
- The generated file name stays the original stem plus `_` and the inputs, in this fixed order: `strip`, `width`, `height`, `quality`, `bytes`.

## 0.1.0

- First version. Strip metadata, shrink a PNG without changing pixels, and check the GitHub avatar rule.
- `--quality` and `--bytes` re-encode a JPEG. They cannot be combined.
