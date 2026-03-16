# GhostType

GhostType is a local autocomplete assistant for Windows that suggests words and phrases while you type.

It learns from your own message history (for example Telegram exports) and predicts what you are likely to write next.

All processing happens locally on your machine.
No cloud, no telemetry, no external APIs.

## Features

- System-wide autocomplete suggestions
- Works in Telegram, browsers, editors, and most apps
- Word and phrase prediction using n-gram language model
- Training from Telegram JSON exports
- Transparent ghost-text overlay near caret
- egui settings UI for configuration
- Customizable appearance, hotkeys, and ignored apps
- Fully local SQLite dataset with in-memory cache
- Process-based app exclusion list

## Example

Typing:

```
она с
```

GhostType suggests: `может`

Typing:

```
я не
```

Suggestion: `знаю`

Press **Tab** to accept the suggestion.

## How It Works

1. Global keyboard hook captures typing system-wide
2. Characters are resolved using the active keyboard layout
3. Context is analyzed using an n-gram language model (trigram > bigram > unigram)
4. The most probable continuation is suggested
5. A transparent overlay shows the prediction near the caret
6. Press Tab to insert the suggestion

## Architecture

```
Keyboard Hook Thread  ──►  Suggestion Engine Thread  ──►  Overlay Renderer Thread
                                                      ◄──  UI Thread (egui)
```

Communication between threads uses `crossbeam-channel`.

### Module Responsibilities

| Module | Role |
|---|---|
| `main.rs` | Entry point, thread orchestration, event handling |
| `hook.rs` | Win32 global keyboard hook |
| `input.rs` | Keyboard layout resolution (ToUnicodeEx) |
| `db.rs` | SQLite n-gram storage (unigrams, bigrams, trigrams) |
| `ngram_cache.rs` | In-memory n-gram cache for zero-latency lookups |
| `suggest.rs` | N-gram prediction engine (trigram > bigram > unigram) |
| `overlay.rs` | Win32 transparent overlay window (ghost text) |
| `state.rs` | Application state (typed buffer, suggestions) |
| `config.rs` | TOML configuration loading/saving |
| `ui.rs` | egui settings UI |
| `telegram_import.rs` | Telegram JSON archive importer |

## Build Instructions

### Prerequisites

- Rust toolchain (stable, MSVC target for Windows)
- Windows 10 or later

### Build

```bash
# Debug build
cargo build

# Release build (optimized, LTO, stripped)
cargo build --release
```

### Run

```bash
# Start GhostType (opens settings UI + starts engine)
cargo run --release

# Import Telegram JSON files from db/ directory
cargo run --release -- import
```

### Import Data

1. Export your Telegram chat history as JSON
2. Place files as `db/db1.json`, `db/db2.json`, etc.
3. Run `cargo run -- import`
4. Or use the "Import" button in the settings UI

## Configuration

Settings are stored in `config.toml` (auto-created on first run):

```toml
mode = "hybrid"
prefix_length = 2
color = "#A0A0A0"
opacity = 0.7
font = "Segoe UI"
font_size = 16
accept_key = "Tab"
ignored_apps = ["obsidian.exe"]
overlay_enabled = true
engine_enabled = true
```

All settings can also be changed through the GUI.

## Settings UI

The settings window provides panels for:

- **General**: Start/Stop engine, enable/disable overlay
- **Suggestions**: Mode (word/phrase/hybrid), minimum prefix length
- **Appearance**: Color, opacity, font, font size
- **Dataset**: Stats display, Import Telegram JSON, Rebuild database
- **Hotkeys**: Accept suggestion key
- **Ignored Apps**: List of process names to skip

## Privacy

GhostType is completely local:

- No internet connection required
- No analytics or tracking
- No data leaves your machine
- All data stored in local SQLite database

## Performance

Target usage:

- CPU idle: < 1%
- RAM: 20-60 MB

Optimizations:

- All n-grams loaded into memory at startup
- Prefix cache for instant unigram lookups
- Debounced suggestion refresh (~30ms)
- SQLite WAL mode for concurrent access
- Batch transaction inserts for imports

## Known Limitations

- Overlay positioning uses `GetGUIThreadInfo` for caret detection; falls back to mouse position in apps that don't expose caret coordinates (e.g., Chrome, Electron apps)
- The keyboard hook captures physical key events; some edge cases with IME or complex keyboard layouts may produce incorrect characters
- Learning from typing is disabled — only Telegram imports populate the model
- The accept key is always Tab (configurable in config but not yet remappable at runtime)
- Ghost text overlay uses GDI rendering; no DirectWrite/GPU acceleration
- Import only processes messages from a hardcoded Telegram user ID

## License

MIT License.
