#!/usr/bin/env python3
"""Generate a large ordered plaintext log fixture and optionally convert it to tinylog."""

from __future__ import annotations

import argparse
import subprocess
from dataclasses import dataclass
from datetime import datetime, timedelta
from pathlib import Path
from typing import Iterable, List


TIMESTAMP_FORMAT = "%Y-%m-%d %H:%M:%S,%f"
TIMESTAMP_TEXT_LENGTH = 23
TIMESTAMP_SEPARATOR_INDEX = TIMESTAMP_TEXT_LENGTH
MESSAGE_START_INDEX = TIMESTAMP_TEXT_LENGTH + 1
BYTES_PER_MEBIBYTE = 1024 * 1024


@dataclass(frozen=True)
class TemplateLine:
    """Store one template line as a timestamp delta plus the original message text."""

    offset_millis: int
    message: str


def parse_arguments() -> argparse.Namespace:
    """Parse the command-line arguments for fixture generation."""
    parser = argparse.ArgumentParser(
        description="Generate a large ordered plaintext log fixture and optionally convert it to .tog."
    )
    parser.add_argument("--source", required=True, help="Path to the source plaintext log template.")
    parser.add_argument("--log-output", required=True, help="Path to the generated plaintext log output.")
    parser.add_argument(
        "--size-mib",
        required=True,
        type=int,
        help="Target plaintext log size in MiB.",
    )
    parser.add_argument("--tog-output", help="Optional path to the generated .tog output.")
    parser.add_argument(
        "--converter-jar",
        help="Path to the tinylog fat jar used to convert the generated plaintext log.",
    )
    return parser.parse_args()


def load_template_lines(source_path: Path) -> tuple[datetime, List[TemplateLine], int]:
    """Load the template log and keep relative line timing so generated output stays ordered."""
    lines = source_path.read_text(encoding="utf-8").splitlines()
    if not lines:
        raise ValueError(f"source log is empty: {source_path}")

    first_timestamp: datetime | None = None
    template_lines: List[TemplateLine] = []
    last_offset_millis = 0

    for line in lines:
        if len(line) <= TIMESTAMP_SEPARATOR_INDEX or line[TIMESTAMP_SEPARATOR_INDEX] != " ":
            raise ValueError(
                f"invalid source log line, expected '<yyyy-MM-dd HH:mm:ss,SSS> <message>': {line}"
            )
        timestamp = datetime.strptime(line[:TIMESTAMP_TEXT_LENGTH], TIMESTAMP_FORMAT)
        if first_timestamp is None:
            first_timestamp = timestamp
        offset_millis = int((timestamp - first_timestamp).total_seconds() * 1000)
        template_lines.append(TemplateLine(offset_millis=offset_millis, message=line[MESSAGE_START_INDEX:]))
        last_offset_millis = offset_millis

    if first_timestamp is None:
        raise ValueError(f"source log is empty: {source_path}")

    # Leave a one-millisecond gap between repeated chunks so the generated file stays strictly ordered.
    chunk_span_millis = max(last_offset_millis + 1, 1)
    return first_timestamp, template_lines, chunk_span_millis


def generate_plaintext_log(
    base_timestamp: datetime,
    template_lines: Iterable[TemplateLine],
    chunk_span_millis: int,
    output_path: Path,
    target_size_bytes: int,
) -> int:
    """Generate a plaintext log fixture that reaches the requested byte size."""
    template_lines = list(template_lines)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    bytes_written = 0
    chunk_index = 0

    with output_path.open("w", encoding="utf-8", newline="\n") as handle:
        while bytes_written < target_size_bytes:
            chunk_base_timestamp = base_timestamp + timedelta(milliseconds=chunk_index * chunk_span_millis)
            for template_line in template_lines:
                current_timestamp = chunk_base_timestamp + timedelta(milliseconds=template_line.offset_millis)
                rendered_line = (
                    f"{current_timestamp.strftime(TIMESTAMP_FORMAT)[:-3]} {template_line.message}\n"
                )
                handle.write(rendered_line)
                bytes_written += len(rendered_line.encode("utf-8"))
                if bytes_written >= target_size_bytes:
                    break
            chunk_index += 1

    return bytes_written


def convert_to_tog(converter_jar: Path, log_output: Path, tog_output: Path) -> None:
    """Convert the generated plaintext log to tinylog format using the existing fat jar."""
    tog_output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "java",
            "-jar",
            str(converter_jar),
            str(log_output),
            str(tog_output),
        ],
        check=True,
    )


def main() -> None:
    """Run the fixture generation workflow."""
    arguments = parse_arguments()
    source_path = Path(arguments.source)
    log_output = Path(arguments.log_output)
    target_size_bytes = arguments.size_mib * BYTES_PER_MEBIBYTE

    base_timestamp, template_lines, chunk_span_millis = load_template_lines(source_path)
    bytes_written = generate_plaintext_log(
        base_timestamp=base_timestamp,
        template_lines=template_lines,
        chunk_span_millis=chunk_span_millis,
        output_path=log_output,
        target_size_bytes=target_size_bytes,
    )

    print(f"generated {log_output} ({bytes_written} bytes)")

    if arguments.tog_output:
        if not arguments.converter_jar:
            raise ValueError("--converter-jar is required when --tog-output is provided")
        convert_to_tog(Path(arguments.converter_jar), log_output, Path(arguments.tog_output))
        print(f"converted {log_output} to {arguments.tog_output}")


if __name__ == "__main__":
    main()
