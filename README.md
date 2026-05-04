# svg-strip

![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg?logo=rust)
![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macos%20%7C%20windows-lightgray)


Fast and aggressive **SVG minification CLI tool** built in Rust, tailored specifically for shrinking SVGs for **inline HTML use** (like Blade templates) and **standalone files**.

![demo](https://artifacts.grolmes.com/svg-strip/demo.gif)

## Features

- **Extreme Minification** (automatic): Removes `title`, `desc`, `metadata`, comments, unused `id`s, hidden elements (`display="none"`, `opacity="0"`), empty groups, and the `<?xml...?>` declaration, collapsing all whitespace and linebreaks into a single-line string.
- **Color Shrink** (automatic): Identifies 6-digit hex color codes with identical byte pairs (e.g., `#FF0000`, `#aabbcc`) across all styling attributes (`fill`, `stroke`, etc.) and losslessly converts them into their 3-digit shorthands (e.g., `#f00`, `#abc`). Leaves non-matching hex codes safely untouched.
- **Inline Optimization (`-i` / `--inline`)**: Strips the `xmlns` attributes which are unnecessary overhead for browsers when embedding SVGs directly into HTML5 code.
- **Decimal Precision (`-dp` / `--decimal-precision`)**: Aggressively rounds all path coordinates and attributes (like `viewBox`, `x`, `y`, `transform`) down to a user-specified number of decimal places (0-4), stripping trailing zeros.

## Usage

```bash
svg-strip [OPTIONS] <input.svg> [output.svg]
```

If no `output.svg` is specified, the tool will automatically save the minified file in the same directory as `[ORIGINAL_NAME]_stripped.svg`.

### Options

| Flag | Name | Description |
|---|---|---|
| `-i`, `--inline` | Inline Mode | Strips `xmlns` attributes for optimal inline HTML usage. |
| `-o`, `--output` | Stdout Mode | Prints the minified SVG directly to the terminal (stdout) instead of writing to a file. |
| `-dp`, `--decimal-precision <0-4>` | Decimal Precision | Rounds all floating point numbers inside paths and attributes to the specified number of decimal places to save bytes. |

### Example

Minify an icon for use in a web template, stripping all namespace overhead and rounding coordinates to 2 decimal places:

```bash
svg-strip -i -dp 2 raw_icon.svg
```

**Output:**
```text
Stripped SVG written to raw_icon_stripped.svg
• Inline SVG with zero overhead
• Decimal Precision for paths rounded down to 2 decimals
• Color Shrink to convert 6-digit hex codes to 3-digit shorthands
```

## Installation

You must have [Rust and Cargo](https://rustup.rs/) installed. Clone the repository and build the project:

```bash
git clone git@github.com:madLinux7/svg-strip.git
cd svg-strip
cargo build --release
```

The compiled binary will be available in `target/release/svg-strip`.
