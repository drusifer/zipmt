# I/O Graph Renderer Assessment

## Finding

The current I/O graph is not a Ratatui `Chart` or `Canvas`. It is a custom
character matrix rendered through `Paragraph`: raw samples become full-block
columns, the ten-second average becomes diamond characters, and the center is a
solid rule. This preserves the mirrored IN/OUT model but limits resolution to
one horizontal point and one vertical cell per terminal cell.

## Options

### Ratatui `Chart`

`Chart` in the installed Ratatui 0.26.3 supports Braille line/scatter datasets
and axes. It does not supply btop-like filled/stippled areas or dotted plot
grids, and its conventional Cartesian layout does not directly model the
mirrored IN/OUT baseline. Two stacked charts would also duplicate axes and make
the shared scale harder to express.

### Ratatui `Canvas`

`Canvas` supports a 2x4 Braille raster and arbitrary line shapes. It is useful
for a prototype, but grid, fill, labels, mirrored scaling, and multi-color
collision policy would still be custom. Braille cells have one foreground
color, so overlapping grid/fill/average layers require explicit precedence.

### Dedicated Ratatui widget

Render directly into Ratatui's `Buffer` using a small Braille raster. This keeps
the existing shared mirrored scale and label ownership while allowing:

- 2x horizontal and 4x vertical Braille resolution;
- btop-like stippled area fill for raw I/O;
- low-contrast dotted guides at 25%, 50%, and 75%;
- a distinct moving-average line drawn above the fill;
- deterministic layer precedence and snapshot tests;
- Braille, block, and TTY fallback marker modes.

## Recommendation

Keep the one-second `IoSample` model and MA10s calculations. Replace only the
`render_io_chart -> Vec<String> -> Paragraph` presentation path with a dedicated
`IoChartWidget` that writes styled cells into Ratatui's buffer.

Do not upgrade Ratatui solely for this work. The current dependency already has
the required Braille primitives; newer `Chart` bar support still would not
provide the mirrored stippled area and dotted grid as one native widget.

## Proposed visual stack

1. Empty background.
2. Muted dotted horizontal guides.
3. Mirrored cyan/yellow Braille stipple for raw IN/OUT.
4. Magenta Braille moving-average line.
5. Center baseline and fixed axis labels.

The graph should expose a marker fallback (`braille`, `block`, `tty`) because
btop documents the same font-support limitation for Braille graphs.
