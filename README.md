# codecosmos

A sophisticated terminal health dashboard for your codebase. Get an instant health score (0-100) for any git repo—see complexity hotspots, danger zones, bus factor risks, test coverage gaps, and track improvements over time.

```
┌────────────────────────┬──────────────────────────────────┬────────────────────────────┐
│  score                 │  breakdown                       │  repo                      │
│                        │                                  │                            │
│   78 (B) ↑             │  churn       ████████████████░░░░  85 │  my-project @ main    │
│   "Good shape"         │  complexity  █████████████░░░░░░░  72 │                        │
│                        │  debt        ██████████████████░░  90 │  trend ▁▂▃▃▄▅▆▆▇▇ +3  │
│                        │  freshness   █████████████░░░░░░░  65 │                        │
├────────────────────────┴──────────────────────────────────┴────────────────────────────┤
│  1·danger 3 │ 2·hotspots 23 │ 3·dusty 12 │ 4·todos 8 │ 5·bus 4 │ 6·tests 7            │
├────────────────────────────────────────────────────────────────────────────────────────┤
│  ◆ danger zones (3)                                                                   │
│                                                                                        │
│  ▓▓ src/parser.rs                                                                     │
│      12× │ c:8.2 │ high churn + complex -> split into smaller modules                 │
│                                                                                        │
│  ▓░ src/analyzer.rs                                                                   │
│      8× │ c:6.1 │ moderate churn + complex -> add test coverage                       │
│                                                                                        │
│  ░░ lib/utils.ts                                                                      │
│      5× │ c:4.3 │ review and simplify                                                 │
├────────────────────────────────────────────────────────────────────────────────────────┤
│ q quit · 1-6 panel · / search · ↵ detail · ? help                                     │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

## Features

- **Health Score (0-100)** with letter grades and trend tracking
- **6 Analysis Panels**: Danger Zones, Hotspots, Dusty Files, TODOs, Bus Factor, Test Coverage
- **Greyscale TUI** with sophisticated typography
- **Score Breakdown** visual bars for each component
- **Trend Sparkline** showing score history over time
- **Search/Filter** across all panels
- **File Detail View** with comprehensive info
- **Bus Factor Analysis** identifying single-author risks
- **Test Coverage Detection** correlating source and test files
- **CI/CD Ready** with JSON output and threshold checks

## Installation

### From source (requires Rust)

```bash
cargo install --path .
```

Or run directly:

```bash
cargo run --release
```

## Usage

```bash
# Run in current directory (launches TUI dashboard)
codecosmos

# Run in a specific directory
codecosmos /path/to/repo

# Quick check (no TUI, just print summary)
codecosmos --check

# Save score to history for trend tracking
codecosmos --check --save

# CI mode: fail if score below threshold
codecosmos --check --threshold 70

# JSON output for pipelines
codecosmos --json

# Skip bus factor analysis (faster for large repos)
codecosmos --skip-authors

# Customize analysis window
codecosmos --days 7 --stale-days 60
```

## Keyboard Controls

| Key | Action |
|-----|--------|
| `1-6` | Switch panels |
| `Tab` / `Shift+Tab` | Next/previous panel |
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `d` / `PgDn` | Page down |
| `u` / `PgUp` | Page up |
| `g` / `G` | Go to top/bottom |
| `/` | Search/filter |
| `Enter` | View file details |
| `?` | Toggle help |
| `Esc` | Close overlay/cancel |
| `q` | Quit |

## Health Score

The health score (0-100) is calculated from four weighted components:

| Component | Weight | What it measures |
|-----------|--------|------------------|
| **Churn** | 30% | Ratio of files changed recently (high churn = lower score) |
| **Complexity** | 30% | Code complexity based on LOC, function length |
| **Debt** | 20% | TODO/FIXME/HACK comments per 1000 lines |
| **Freshness** | 20% | Ratio of dusty (untouched) files |

### Grades

| Grade | Score | Description |
|-------|-------|-------------|
| **A** | 90-100 | Excellent health |
| **B** | 75-89 | Good shape |
| **C** | 60-74 | Needs attention |
| **D** | 40-59 | Significant issues |
| **F** | 0-39 | Critical state |

### Trend Indicators

When history is available (use `--save`), the dashboard shows:
- **↑** Improving (score increased by 3+)
- **↓** Declining (score decreased by 3+)
- **→** Stable (within ±2 points)
- **Sparkline** showing recent score history

## Panels

### 1. Danger Zones (◆)
Files that are **both** high-churn AND high-complexity. These are your riskiest files—frequently changed and hard to maintain. Risk levels:
- `▓▓` Critical (danger score ≥70)
- `▓░` High (danger score ≥50)
- `░░` Medium

### 2. Hotspots (●)
Files with the most changes in the analysis window. High churn often indicates active development or instability. Shows relative churn bars.

### 3. Dusty Files (○)
Old files nobody has touched in months. May indicate tech debt, dead code, or stable foundations. Staleness indicators:
- `·` 90-120 days
- `··` 120-240 days
- `···` 240-365 days
- `····` 365+ days

### 4. TODOs (▸)
All TODO, FIXME, HACK, and XXX comments found in the codebase, sorted by priority. Color intensity indicates severity (FIXME > HACK > TODO > XXX).

### 5. Bus Factor (◐)
Files with concentrated ownership—single author or dominant contributor. Identifies knowledge silos and single-point-of-failure risks:
- Shows primary author and their code ownership percentage
- Highlights files where one person wrote >80% of the code
- Aggregates total authors and single-author file count

### 6. Test Coverage (◇)
Correlates source files with their test files using naming conventions:
- `●` Has test file
- `◐` Has inline tests
- `○` **No tests** (highlighted for attention)

Shows overall coverage percentage and warns about untested danger zones.

## File Detail View

Press `Enter` on any file to see comprehensive details:
- Change count and last modified date
- Complexity score and danger zone status
- Primary author and ownership percentage
- Test coverage status and associated test files
- Actionable recommendations

## CI Integration

### GitHub Actions

```yaml
- name: Check code health
  run: |
    cargo install --path .
    codecosmos --check --threshold 70 --json > health.json
```

### Exit Codes

- `0` - Score meets or exceeds threshold (or no threshold set)
- `1` - Score below threshold

### JSON Output

Use `--json` for machine-readable output:

```json
{
  "score": 78,
  "grade": "B",
  "components": {
    "churn": 85,
    "complexity": 72,
    "debt": 90,
    "freshness": 65
  },
  "metrics": {
    "total_files": 142,
    "total_loc": 15420,
    "files_changed_recently": 23,
    "todo_count": 5,
    "fixme_count": 2,
    "hack_count": 1,
    "dusty_file_count": 12,
    "danger_zone_count": 3
  },
  "danger_zones": [...],
  "test_coverage": {
    "coverage_pct": 64.5,
    "files_with_tests": 92,
    "files_without_tests": 50,
    "untested_danger_zones": ["src/parser.rs"]
  },
  "bus_factor": {
    "total_authors": 5,
    "single_author_files": 23,
    "avg_bus_factor": 1.8,
    "high_risk_files": [...]
  }
}
```

## Options

```
Usage: codecosmos [OPTIONS] [PATH]

Arguments:
  [PATH]  Path to the repository [default: .]

Options:
  -d, --days <DAYS>        Days to analyze for churn [default: 14]
  -s, --stale-days <DAYS>  Days until a file is "dusty" [default: 90]
  -c, --check              Print summary and exit (no TUI)
  -t, --threshold <SCORE>  Minimum score threshold (exit 1 if below)
      --json               Output results as JSON
      --save               Save score to history for trend tracking
      --skip-authors       Skip bus factor analysis (faster)
  -h, --help               Print help
  -V, --version            Print version
```

## History

Scores are stored in `.codecosmos/history.json` in your repo (automatically gitignored). Use `--save` to record snapshots and track trends over time.

## Design Philosophy

codecosmos uses a **greyscale aesthetic** with intensity-based visual hierarchy:
- Brighter = more important/critical
- Dimmer = less urgent/historical
- Pure white for maximum emphasis
- Unicode box-drawing and symbols for texture

The goal is a tool that's both **functional** and **beautiful**—something developers actually want to look at.

## License

MIT
