#!/usr/bin/env python3
"""Generate a terminal-style GIF that demonstrates the TinyLog workflow."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, List, Sequence, Tuple

from PIL import Image, ImageDraw, ImageFont


OUTPUT_PATH = Path("assets/tinylog-demo.gif")
CANVAS_WIDTH = 1100
CANVAS_HEIGHT = 760
WINDOW_MARGIN = 36
TITLE_BAR_HEIGHT = 42
TERMINAL_PADDING_X = 28
TERMINAL_PADDING_Y = 24
LINE_GAP = 12
FONT_SIZE = 24
SMALL_FONT_SIZE = 18
BACKGROUND_COLOR = "#0b1020"
WINDOW_COLOR = "#111827"
WINDOW_BORDER_COLOR = "#1f2937"
TITLE_TEXT_COLOR = "#d1d5db"
TERMINAL_TEXT_COLOR = "#d1fae5"
MUTED_TEXT_COLOR = "#93c5fd"
COMMAND_TEXT_COLOR = "#f8fafc"
PROMPT_TEXT_COLOR = "#34d399"
STATUS_TEXT_COLOR = "#cbd5e1"
SEARCH_HIGHLIGHT_COLOR = "#fde047"
FILTER_HIGHLIGHT_COLOR = "#fca5a5"
CURRENT_LINE_BACKGROUND = "#1e293b"
TOP_BAR_TEXT = "TinyLog demo"


@dataclass(frozen=True)
class Highlight:
    """Describe one text highlight inside a rendered line."""

    start: int
    end: int
    fill: str
    foreground: str = "#111827"


@dataclass(frozen=True)
class FrameSpec:
    """Describe one logical terminal frame before GIF encoding."""

    lines: Sequence[str]
    duration_ms: int
    highlights: dict[int, Sequence[Highlight]] = field(default_factory=dict)
    current_line: int | None = None


def load_font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    """Load a monospace font with a stable fallback."""

    candidates = [
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/SFNSMono.ttf",
        "/System/Library/Fonts/Supplemental/Courier New.ttf",
        "/System/Library/Fonts/Monaco.ttf",
    ]
    for candidate in candidates:
        path = Path(candidate)
        if path.exists():
            return ImageFont.truetype(str(path), size=size)
    return ImageFont.load_default()


def terminal_lines(*lines: str) -> list[str]:
    """Return a mutable terminal line list."""

    return list(lines)


def typing_frames(
    prefix_lines: Sequence[str],
    prompt: str,
    command: str,
    *,
    step: int = 5,
    tail_lines: Sequence[str] | None = None,
    typing_duration_ms: int = 60,
    settle_duration_ms: int = 700,
) -> list[FrameSpec]:
    """Build a short typing animation for one terminal command."""

    tail_lines = list(tail_lines or [])
    frames: list[FrameSpec] = []
    for index in range(step, len(command) + step, step):
        snippet = command[:index]
        frames.append(
            FrameSpec(
                lines=[*prefix_lines, f"{prompt}{snippet}_", *tail_lines],
                duration_ms=typing_duration_ms,
            )
        )
    frames.append(
        FrameSpec(
            lines=[*prefix_lines, f"{prompt}{command}", *tail_lines],
            duration_ms=settle_duration_ms,
        )
    )
    return frames


def viewer_frames() -> list[FrameSpec]:
    """Build the viewer open-and-jump-to-bottom part of the animation."""

    base = terminal_lines(
        "$ scripts/tinylog-view.sh logs/normal-10g.tog",
        "",
        "TinyLog viewer | file=normal-10g.tog | records=134217728 | line=1",
        "1 ▪ 2026-05-01 22:01:00,253 [INFO] service started",
        "2   2026-05-01 22:01:00,278 [WARN] queue depth rising",
        "3   2026-05-01 22:01:00,353 [ERROR] order created",
        "",
        "status: opened indexed 10 GiB source without full-file decode",
    )
    jump_lines = terminal_lines(
        "$ scripts/tinylog-view.sh logs/normal-10g.tog",
        "",
        "TinyLog viewer | file=normal-10g.tog | records=134217728 | line=1",
        "1 ▪ 2026-05-01 22:01:00,253 [INFO] service started",
        "2   2026-05-01 22:01:00,278 [WARN] queue depth rising",
        "3   2026-05-01 22:01:00,353 [ERROR] order created",
        "",
        "input: G",
    )
    last_screen = terminal_lines(
        "$ scripts/tinylog-view.sh logs/normal-10g.tog",
        "",
        "TinyLog viewer | file=normal-10g.tog | records=134217728 | line=134217726",
        "134217726   2026-05-03 09:14:11,901 [INFO] flush completed",
        "134217727   2026-05-03 09:14:11,927 [WARN] queue depth rising",
        "134217728 ▪ 2026-05-03 09:14:11,998 [ERROR] order created",
        "",
        "status: G -> jumped to the final window immediately",
    )
    return [
        FrameSpec(lines=base, duration_ms=1400, current_line=3),
        FrameSpec(
            lines=jump_lines,
            duration_ms=1200,
            current_line=3,
            highlights={
                7: [
                    Highlight(
                        start=jump_lines[7].index("G"),
                        end=jump_lines[7].index("G") + 1,
                        fill=SEARCH_HIGHLIGHT_COLOR,
                    )
                ]
            },
        ),
        FrameSpec(lines=last_screen, duration_ms=2200, current_line=5),
    ]


def build_frames() -> list[FrameSpec]:
    """Compose the full TinyLog demo sequence."""

    conversion_lines = terminal_lines(
        "$ ./scripts/tinylog-convert.sh logs/normal-10g.log",
        "counting total lines in logs/normal-10g.log",
        "using parallel conversion mode for inputs larger than 100.00 MiB",
        "building trunk index and preparing worker assignments for logs/normal-10g.log",
        "indexing: 0/49026105 (0.00%)",
        "indexing: 49026105/49026105 (100.00%)",
        "compressing 18334 trunks with 16 workers",
        "workers 1: 0% 2: 0% 3: 0% 4: 0%",
        "workers 1: 10% 2: 20% 3: 24% 4: 10%",
        "converted logs/normal-10g.log to logs/normal-10g.tog using gzip",
        "source size: 10.00 GiB",
        "output size: 234.19 MiB",
        "compression ratio: 2.29%",
        "elapsed: 195.950s",
    )
    preview_lines = terminal_lines(
        "$ ./scripts/tinylog-convert.sh logs/normal-10g.log",
        "converted logs/normal-10g.log to logs/normal-10g.tog using gzip",
        "",
        "$ scripts/tinylog-view.sh logs/normal-10g.tog",
        "TinyLog viewer | file=normal-10g.tog | records=134217728 | line=134217726",
        "134217726   2026-05-03 09:14:11,901 [INFO] flush completed",
        "134217727   2026-05-03 09:14:11,927 [WARN] queue depth rising",
        "134217728 ▪ 2026-05-03 09:14:11,998 [ERROR] order created",
        "",
        "workflow: convert 10 GiB -> open .tog -> press G -> inspect last screen",
    )
    frames = [FrameSpec(lines=preview_lines, duration_ms=1300, current_line=7)]
    frames.extend(
        typing_frames(
            [],
            "$ ",
            "./scripts/tinylog-convert.sh logs/normal-10g.log",
            step=6,
        )
    )
    frames.extend(
        [
            FrameSpec(lines=conversion_lines[:2], duration_ms=450),
            FrameSpec(lines=conversion_lines[:4], duration_ms=600),
            FrameSpec(lines=conversion_lines[:6], duration_ms=650),
            FrameSpec(lines=conversion_lines[:8], duration_ms=700),
            FrameSpec(lines=conversion_lines, duration_ms=1400),
        ]
    )
    frames.extend(
        typing_frames(
            conversion_lines,
            "$ ",
            "scripts/tinylog-view.sh logs/normal-10g.tog",
            step=6,
            tail_lines=[],
            typing_duration_ms=55,
            settle_duration_ms=700,
        )
    )
    frames.extend(viewer_frames())
    return frames


def rounded_rectangle(
    draw: ImageDraw.ImageDraw,
    box: Tuple[int, int, int, int],
    radius: int,
    *,
    fill: str,
    outline: str | None = None,
) -> None:
    """Draw a rounded rectangle with Pillow compatibility."""

    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=2 if outline else 0)


def render_frame(
    frame: FrameSpec,
    font: ImageFont.FreeTypeFont | ImageFont.ImageFont,
    small_font: ImageFont.FreeTypeFont | ImageFont.ImageFont,
) -> Image.Image:
    """Render one logical frame into an RGB image."""

    image = Image.new("RGB", (CANVAS_WIDTH, CANVAS_HEIGHT), BACKGROUND_COLOR)
    draw = ImageDraw.Draw(image)

    left = WINDOW_MARGIN
    top = WINDOW_MARGIN
    right = CANVAS_WIDTH - WINDOW_MARGIN
    bottom = CANVAS_HEIGHT - WINDOW_MARGIN
    rounded_rectangle(
        draw,
        (left, top, right, bottom),
        radius=24,
        fill=WINDOW_COLOR,
        outline=WINDOW_BORDER_COLOR,
    )

    draw.rectangle((left, top, right, top + TITLE_BAR_HEIGHT), fill="#0f172a")
    for index, color in enumerate(("#fb7185", "#fbbf24", "#34d399")):
        circle_left = left + 18 + index * 18
        draw.ellipse((circle_left, top + 14, circle_left + 10, top + 24), fill=color)
    draw.text((left + 70, top + 10), TOP_BAR_TEXT, font=small_font, fill=TITLE_TEXT_COLOR)

    terminal_left = left + TERMINAL_PADDING_X
    terminal_top = top + TITLE_BAR_HEIGHT + TERMINAL_PADDING_Y
    line_height = font.size + LINE_GAP

    for line_index, line in enumerate(frame.lines):
        y = terminal_top + line_index * line_height
        if frame.current_line == line_index:
            rounded_rectangle(
                draw,
                (terminal_left - 10, y - 3, right - TERMINAL_PADDING_X, y + font.size + 8),
                radius=10,
                fill=CURRENT_LINE_BACKGROUND,
            )

        text_color = TERMINAL_TEXT_COLOR
        if line.startswith("$ "):
            draw.text((terminal_left, y), "$ ", font=font, fill=PROMPT_TEXT_COLOR)
            draw.text(
                (terminal_left + draw.textlength("$ ", font=font), y),
                line[2:],
                font=font,
                fill=COMMAND_TEXT_COLOR,
            )
        elif line.startswith("TinyLog viewer |"):
            draw.text((terminal_left, y), line, font=font, fill=MUTED_TEXT_COLOR)
        elif line.startswith("status:"):
            draw.text((terminal_left, y), line, font=font, fill=STATUS_TEXT_COLOR)
        elif line.startswith(": "):
            draw.text((terminal_left, y), line, font=font, fill="#f9a8d4")
        else:
            draw.text((terminal_left, y), line, font=font, fill=text_color)

        for highlight in frame.highlights.get(line_index, []):
            prefix = line[: highlight.start]
            target = line[highlight.start : highlight.end]
            x = terminal_left + draw.textlength(prefix, font=font)
            width = draw.textlength(target, font=font)
            rounded_rectangle(
                draw,
                (int(x - 2), y - 1, int(x + width + 2), y + font.size + 5),
                radius=6,
                fill=highlight.fill,
            )
            draw.text((x, y), target, font=font, fill=highlight.foreground)

    footer = "10 GiB log -> compressed .tog -> press G to jump to the final window"
    footer_y = bottom - 42
    draw.line((terminal_left, footer_y - 10, right - TERMINAL_PADDING_X, footer_y - 10), fill=WINDOW_BORDER_COLOR, width=1)
    draw.text((terminal_left, footer_y), footer, font=small_font, fill=STATUS_TEXT_COLOR)
    return image


def save_gif(frames: Iterable[FrameSpec]) -> None:
    """Render and save the TinyLog demo GIF."""

    font = load_font(FONT_SIZE)
    small_font = load_font(SMALL_FONT_SIZE)
    rendered: List[Image.Image] = []
    durations: List[int] = []

    for frame in frames:
        rendered.append(render_frame(frame, font, small_font).convert("P", palette=Image.ADAPTIVE, colors=128))
        durations.append(frame.duration_ms)

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    rendered[0].save(
        OUTPUT_PATH,
        save_all=True,
        append_images=rendered[1:],
        duration=durations,
        loop=0,
        optimize=True,
        disposal=2,
    )


def main() -> None:
    """Generate the demo asset."""

    save_gif(build_frames())
    print(f"generated {OUTPUT_PATH}")


if __name__ == "__main__":
    main()
