"""Generate paired Dark/Light Traffic-Light SVG charts for the 6-language Polyglot & RTK Benchmark.

Strict semantic color standard:
  - Green (#3fb950 dark / #16a34a light) = Optimal / Best efficiency (SymForge V12 Code MCP & CLI+RTK)
  - Orange (#f0883e dark / #d97706 light) = Partial / Intermediate tradeoff (RTK CLI without AST, or Full 39-tool MCP upfront schema)
  - Red (#f85149 dark / #dc2626 light) = High token burn (Native Raw whole-file reads)
"""

from __future__ import annotations

import html
import json
import math
import pathlib
import xml.etree.ElementTree as ET
from typing import Any


def fmt_k(n: int) -> str:
    if n >= 1000:
        return f"{n / 1000.0:.1f}k"
    return f"{n}"


def render_svg(report: dict[str, Any], theme: str) -> str:
    dark = theme == "dark"
    bg = "#0d1117" if dark else "#ffffff"
    panel = "#161b22" if dark else "#f8fafc"
    border = "#30363d" if dark else "#cbd5e1"
    fg = "#e6edf3" if dark else "#0f172a"
    muted = "#8b949e" if dark else "#475569"
    grid = "#21262d" if dark else "#e2e8f0"

    red = "#f85149" if dark else "#dc2626"
    orange = "#f0883e" if dark else "#d97706"
    amber = "#d29922" if dark else "#ca8a04"
    green = "#3fb950" if dark else "#16a34a"
    emerald = "#2ea043" if dark else "#15803d"

    repos = report["repos"]
    gt = report["grand_totals"]

    width = 1080
    height = 700

    # Use square-root scale on per-repo bars so 3k..140k remain clearly legible side-by-side
    max_tok = max(
        max(
            r["session_5task_tokens"]["native_raw"],
            r["session_5task_tokens"]["rtk_cli"],
            r["session_5task_tokens"]["symforge_full_mcp"],
        )
        for r in repos
    )
    bar_max_w = 500.0

    def scale_w(val: int) -> float:
        if val <= 0:
            return 4.0
        return max(6.0, round(math.sqrt(val / max_tok) * bar_max_w, 1))

    svg: list[str] = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" font-family="ui-sans-serif, -apple-system, BlinkMacSystemFont, Segoe UI, Helvetica, Arial, sans-serif">',
        f'  <rect width="{width}" height="{height}" rx="14" fill="{bg}" stroke="{border}" stroke-width="1.5"/>',
        # Header
        f'  <text x="32" y="40" fill="{fg}" font-size="20" font-weight="700">SymForge 12.0.0 vs. RTK 1.2.1 vs. Native — 6-Language Polyglot Token Benchmark</text>',
        f'  <text x="32" y="63" fill="{muted}" font-size="13">30 auto-graded tasks across {report["total_indexed_files"]:,} files (Rust · C# · TypeScript · Python · Java · Go) · Exact tiktoken o200k_base tokens (Upfront Schema + 5 Tasks/Repo)</text>',
        # Legend
        f'  <g transform="translate(32, 80)">',
        f'    <rect x="0" y="0" width="12" height="12" rx="3" fill="{red}"/><text x="18" y="10.5" fill="{fg}" font-size="12" font-weight="600">Native Raw (rg + read)</text>',
        f'    <rect x="185" y="0" width="12" height="12" rx="3" fill="{orange}"/><text x="203" y="10.5" fill="{fg}" font-size="12" font-weight="600">RTK 1.2.1 CLI (no AST slice)</text>',
        f'    <rect x="405" y="0" width="12" height="12" rx="3" fill="{amber}"/><text x="423" y="10.5" fill="{fg}" font-size="12" font-weight="600">SymForge Full MCP (40 tools)</text>',
        f'    <rect x="635" y="0" width="12" height="12" rx="3" fill="{green}"/><text x="653" y="10.5" fill="{fg}" font-size="12" font-weight="600">SymForge V12 Code MCP (4 tools)</text>',
        f'    <rect x="880" y="0" width="12" height="12" rx="3" fill="{emerald}"/><text x="898" y="10.5" fill="{fg}" font-size="12" font-weight="600">SymForge CLI + RTK</text>',
        f'  </g>',
    ]

    start_y = 112
    row_h = 74
    bar_x = 245

    arm_specs = [
        ("native_raw", red),
        ("rtk_cli", orange),
        ("symforge_full_mcp", amber),
        ("symforge_code_mcp", green),
        ("symforge_cli_rtk", emerald),
    ]

    for idx, r in enumerate(repos):
        y = start_y + idx * row_h
        svg.append(f'  <rect x="24" y="{y}" width="{width - 48}" height="{row_h - 8}" rx="8" fill="{panel}" stroke="{grid}" stroke-width="1"/>')
        svg.append(f'  <text x="38" y="{y + 24}" fill="{fg}" font-size="14" font-weight="700">{html.escape(r["language"])}</text>')
        svg.append(f'  <text x="38" y="{y + 42}" fill="{muted}" font-size="11.5">{html.escape(r["display_name"])}</text>')
        svg.append(f'  <text x="38" y="{y + 57}" fill="{muted}" font-size="10.5">{r["indexed_files"]:,} files · {r["commit_sha"][:8]}</text>')

        s = r["session_5task_tokens"]
        nat_v = s["native_raw"]
        code_v = s["symforge_code_mcp"]
        hyb_v = s["symforge_cli_rtk"]
        red_code = round((1.0 - code_v / nat_v) * 100.0, 1)
        red_hyb = round((1.0 - hyb_v / nat_v) * 100.0, 1)

        for b_i, (arm_key, color) in enumerate(arm_specs):
            by = y + 7 + b_i * 11
            val = s[arm_key]
            bw = scale_w(val)
            svg.append(f'  <rect x="{bar_x}" y="{by}" width="{bw}" height="8.5" rx="2.5" fill="{color}"/>')
            svg.append(f'  <text x="{bar_x + bw + 7}" y="{by + 7.8}" fill="{fg if arm_key in ("symforge_code_mcp", "symforge_cli_rtk") else muted}" font-size="10.5" font-weight="{"700" if arm_key in ("symforge_code_mcp", "symforge_cli_rtk") else "500"}">{fmt_k(val)}</text>')

        # Right-side Traffic-Light Callout Badge per Language
        badge_x = 835
        svg.append(f'  <rect x="{badge_x}" y="{y + 11}" width="205" height="44" rx="6" fill="{bg}" stroke="{green}" stroke-width="1.2"/>')
        svg.append(f'  <text x="{badge_x + 12}" y="{y + 29}" fill="{green}" font-size="12.5" font-weight="700">Code MCP: -{red_code}% ({fmt_k(code_v)})</text>')
        svg.append(f'  <text x="{badge_x + 12}" y="{y + 46}" fill="{emerald}" font-size="11.5" font-weight="600">CLI + RTK: -{red_hyb}% ({fmt_k(hyb_v)})</text>')

    # Bottom Summary Strip (6-Repo Grand Totals)
    sum_y = 564
    svg.append(f'  <rect x="24" y="{sum_y}" width="{width - 48}" height="114" rx="10" fill="{panel}" stroke="{border}" stroke-width="1.5"/>')
    svg.append(f'  <text x="40" y="{sum_y + 26}" fill="{fg}" font-size="14.5" font-weight="700">Aggregate 6-Repository Totals (30 Tasks · 6 Sessions · 100% Deterministic Oracle Pass)</text>')

    cards = [
        ("Native Raw", gt["native_raw"], red, "Baseline 100%"),
        ("RTK 1.2.1 CLI", gt["rtk_cli"], orange, f"-{gt['rtk_cli']['reduction_vs_native_raw_pct']:.1f}% vs Native"),
        ("SymForge Full MCP", gt["symforge_full_mcp"], amber, f"-{gt['symforge_full_mcp']['reduction_vs_native_raw_pct']:.1f}% vs Native"),
        ("SymForge V12 Code MCP", gt["symforge_code_mcp"], green, f"-{gt['symforge_code_mcp']['reduction_vs_native_raw_pct']:.1f}% (-{gt['symforge_code_mcp']['reduction_vs_rtk_pct']:.1f}% vs RTK)"),
        ("SymForge CLI + RTK", gt["symforge_cli_rtk"], emerald, f"-{gt['symforge_cli_rtk']['reduction_vs_native_raw_pct']:.1f}% (-{gt['symforge_cli_rtk']['reduction_vs_rtk_pct']:.1f}% vs RTK)"),
    ]
    card_w = 194
    for c_i, (title, g, col, sub) in enumerate(cards):
        cx = 38 + c_i * (card_w + 8)
        cy = sum_y + 38
        svg.append(f'  <rect x="{cx}" y="{cy}" width="{card_w}" height="64" rx="7" fill="{bg}" stroke="{col}" stroke-width="1.4"/>')
        svg.append(f'  <text x="{cx + 10}" y="{cy + 18}" fill="{muted}" font-size="11" font-weight="600">{html.escape(title)}</text>')
        svg.append(f'  <text x="{cx + 10}" y="{cy + 39}" fill="{col}" font-size="17" font-weight="800">{g["total_session_tokens_6repos"]:,} tok</text>')
        svg.append(f'  <text x="{cx + 10}" y="{cy + 55}" fill="{fg}" font-size="10.5">{html.escape(sub)} · {g["total_turns_30tasks"]} turns</text>')

    svg.append("</svg>\n")
    xml_text = "\n".join(svg)
    ET.fromstring(xml_text)  # Validate well-formed XML
    return xml_text


def main() -> None:
    here = pathlib.Path(__file__).resolve().parent
    repo_root = here.parent.parent
    report_path = here / "results" / "stage1-results.json"
    report = json.loads(report_path.read_text(encoding="utf-8"))

    diagrams_dir = repo_root / "docs" / "diagrams"
    diagrams_dir.mkdir(parents=True, exist_ok=True)
    for theme in ("dark", "light"):
        out_path = diagrams_dir / f"symforge-polyglot-rtk-{theme}.svg"
        out_path.write_text(render_svg(report, theme), encoding="utf-8")
        print(f"Wrote XML-verified {out_path} ({out_path.stat().st_size:,} B)")


if __name__ == "__main__":
    main()
