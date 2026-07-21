"""Render the tested 80x22 Rust TUI snapshots as documentation SVGs."""

from __future__ import annotations

from html import escape
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SNAPSHOTS = ROOT / "zipmt-rust" / "src" / "snapshots"
OUTPUT = ROOT / "docs" / "assets"
FIXTURES = {
    "rust-tui-split.svg": (
        SNAPSHOTS / "zipmt_rust__tui__tests__tui_layout_split_mode_snapshot.snap",
        "Rust TUI — Split mode",
    ),
    "rust-tui-stream.svg": (
        SNAPSHOTS / "zipmt_rust__tui__tests__tui_layout_stream_mode_snapshot.snap",
        "Rust TUI — Stream mode",
    ),
}


def snapshot_lines(path: Path) -> list[str]:
    """Return only the terminal frame from an Insta snapshot."""
    content = path.read_text(encoding="utf-8")
    marker = "---\n"
    frame_start = content.find(marker, len(marker))
    if frame_start < 0:
        raise ValueError(f"snapshot frame marker missing: {path}")
    return content[frame_start + len(marker) :].rstrip("\n").splitlines()


def line_color(index: int, line: str) -> str:
    """Apply the dashboard palette without altering the tested frame text."""
    if index == 0:
        return "#f4d06f"
    if line.startswith(("╭", "╰")):
        return "#f28c45"
    if "TOTAL" in line or "IN " in line or "AVG" in line or "Q " in line:
        return "#75d5e8"
    return "#d8b4e2"


def render_svg(lines: list[str], title: str) -> str:
    """Render a terminal-like SVG with accessible title and description."""
    char_width = 9.6
    line_height = 19
    padding = 24
    title_height = 42
    columns = max(len(line) for line in lines)
    width = int(columns * char_width + padding * 2)
    height = title_height + len(lines) * line_height + padding
    text = []
    for index, line in enumerate(lines):
        y = title_height + 16 + index * line_height
        text.append(
            f'<text x="{padding}" y="{y}" fill="{line_color(index, line)}">'
            f"{escape(line)}</text>"
        )
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">
  <title id="title">{escape(title)}</title>
  <desc id="description">Tested 80 by 22 terminal dashboard rendered from the Rust snapshot fixture.</desc>
  <rect width="100%" height="100%" rx="12" fill="#090b10"/>
  <rect x="1" y="1" width="{width - 2}" height="{height - 2}" rx="11" fill="none" stroke="#3d405b" stroke-width="2"/>
  <rect x="1" y="1" width="{width - 2}" height="34" rx="11" fill="#171a24"/>
  <circle cx="18" cy="18" r="5" fill="#f28c45"/>
  <circle cx="34" cy="18" r="5" fill="#f4d06f"/>
  <circle cx="50" cy="18" r="5" fill="#75d5e8"/>
  <text x="{width / 2}" y="23" text-anchor="middle" fill="#b8b8c8" font-family="ui-monospace, SFMono-Regular, Consolas, monospace" font-size="13">{escape(title)}</text>
  <g font-family="ui-monospace, SFMono-Regular, Consolas, 'Liberation Mono', monospace" font-size="15" xml:space="preserve">
    {''.join(text)}
  </g>
</svg>
'''


def main() -> None:
    """Regenerate both documentation assets deterministically."""
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for filename, (snapshot, title) in FIXTURES.items():
        destination = OUTPUT / filename
        destination.write_text(render_svg(snapshot_lines(snapshot), title), encoding="utf-8")
        print(f"wrote {destination.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
