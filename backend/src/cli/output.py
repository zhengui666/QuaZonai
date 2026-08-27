"""Human and JSON CLI rendering."""

from __future__ import annotations

import json
from typing import Any


def render_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True)


def render_table(value: Any) -> str:
    if isinstance(value, dict):
        width = max((len(str(key)) for key in value), default=0)
        return "\n".join(f"{str(key):<{width}}  {format_cell(item)}" for key, item in value.items())
    if isinstance(value, list):
        if not value:
            return "No results."
        if all(isinstance(item, dict) for item in value):
            rows = [item for item in value if isinstance(item, dict)]
            columns: list[str] = []
            for row in rows:
                for key in row:
                    if key not in columns:
                        columns.append(key)
            widths = {
                column: max(
                    len(column),
                    *(len(format_cell(row.get(column))) for row in rows),
                )
                for column in columns
            }
            header = "  ".join(f"{column:<{widths[column]}}" for column in columns)
            divider = "  ".join("-" * widths[column] for column in columns)
            body = [
                "  ".join(f"{format_cell(row.get(column)):<{widths[column]}}" for column in columns)
                for row in rows
            ]
            return "\n".join([header, divider, *body])
    return format_cell(value)


def format_cell(value: Any) -> str:
    if value is None:
        return "-"
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, (dict, list)):
        return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    return str(value)
