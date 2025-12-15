# ☽ C O S M O S ✦

A **terminal-first codebase steward** for solo developers.

Cosmos is not an editor. It lives *outside* your editing loop and helps you keep a repo healthy: it reads git context, surfaces high-leverage improvements, and can turn fixes into shippable branches/PRs.

Uses AST-based indexing and AI (via OpenRouter presets) to proactively suggest improvements, bug fixes, optimizations, and new features while minimizing LLM spend through smart caching and tiered model usage.

**Monochromatic. Minimal. Meaningful.**

### Where Cosmos fits (vs Cursor)
- **Cursor**: your editor and interactive coding environment.
- **Cosmos**: your calm, git-native steward that helps you *choose the right next changes* and *ship them safely*.

```
╔══════════════════════════════════════════════════════════════╗
║                    ☽ C O S M O S ✦                           ║
║          a contemplative companion for your codebase         ║
╠═══════════════════════════╦══════════════════════════════════╣
║  PROJECT                  ║  SUGGESTIONS                     ║
║  · src/                   ║  ● Improve: ai.rs has 715        ║
║    · main.rs          ●   ║    lines - split into modules    ║
║    · ui/                  ║                                  ║
║    · index/               ║  ◐ Quality: Missing tests for    ║
║  · tests/                 ║    public functions              ║
╠═══════════════════════════╩══════════════════════════════════╣
║  main ● 5 changed │ ? inquiry  ↵ view  a apply  q quit       ║
╚══════════════════════════════════════════════════════════════╝
```

## Features

- **AST-Powered Indexing** - Deep understanding of your codebase using tree-sitter
- **Tiered AI Suggestions** - Opus 4.5 for depth, Grok Fast for speed, static rules for free
- **Git-Aware Context** - Knows what you're working on from uncommitted changes
- **Dual-Panel UI** - Project tree + suggestions side by side
- **Cosmic Aesthetic** - Monochromatic with celestial motifs
- **Hyper-Optimized** - <$0.10 per session through smart caching

## Installation

```bash
# From source
cargo install --path .

# Or run directly
cargo run --release
```

## Quick Start

```bash
# Launch the TUI
cosmos

# Point at a specific project
cosmos /path/to/project

# Show stats without TUI
cosmos --stats

# Set up AI features
cosmos --setup
```

## Keyboard Controls

| Key | Action |
|-----|--------|
| `↑/k` `↓/j` | Navigate |
| `Tab` | Switch panels |
| `Enter` | View suggestion detail |
| `?` | Toggle help |
| `a` | Apply selected suggestion |
| `d` | Dismiss suggestion |
| `i` | Inquiry - ask AI for suggestions |
| `r` | Refresh context |
| `q` | Quit |

## How It Works

### 1. AST Indexing

On startup, Cosmos parses your codebase with tree-sitter to understand:
- Functions, classes, structs, traits
- Dependencies and imports
- Code patterns and complexity

### 2. Tiered Suggestions

| Layer | Cost | When Used |
|-------|------|-----------|
| **Static Rules** | $0 | Always - pattern matching for file size, complexity, TODOs |
| **Cached** | $0 | Previously generated suggestions, stored in `.cosmos/` |
| **Grok Fast** | ~$0.0001 | Quick categorization on browse |
| **Opus 4.5** | ~$0.02 | Deep analysis on explicit inquiry |

### 3. Git-Aware Context

Cosmos infers what you're working on from:
- Uncommitted changes
- Staged files
- Recent commits

It prioritizes suggestions relevant to your current focus.

## Suggestion Types

| Icon | Type | Description |
|------|------|-------------|
| ● | High | Significant improvement opportunity |
| ◐ | Medium | Worth considering |
| ○ | Low | Minor enhancement |

**Categories:**
- **Improvement** - Refactoring opportunities
- **BugFix** - Potential bugs or error handling
- **Optimization** - Performance improvements
- **Quality** - Code cleanliness, missing tests
- **Feature** - New feature suggestions

## AI Setup

```bash
cosmos --setup
```

This guides you through getting an OpenRouter API key (https://openrouter.ai/keys) and saves it securely.

## Caching

Cosmos stores suggestions in `.cosmos/` to:
- Avoid redundant LLM calls
- Speed up subsequent sessions
- Remember dismissed suggestions

Add `.cosmos/` to your `.gitignore` (Cosmos does this automatically).

## Design Philosophy

**Monochrome with meaning:**
- White = attention required
- Grey gradients = information hierarchy
- Celestial symbols = cosmic branding

**Contemplative pace:**
- No flashing or jarring transitions
- Zen-like layout with ample whitespace
- Suggestions, not demands

**Hyper-optimization:**
- Free static analysis first
- AI only when needed
- Cache everything cacheable

## Options

```
Usage: cosmos [OPTIONS] [PATH]

Arguments:
  [PATH]  Path to the repository [default: .]

Options:
      --setup   Set up OpenRouter API key
      --stats   Show stats and exit (no TUI)
  -h, --help    Print help
  -V, --version Print version
```

## License

MIT

---

*"Where code meets the cosmos"*
