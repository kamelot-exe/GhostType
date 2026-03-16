GhostType

GhostType is a local autocomplete assistant for Windows that suggests words and phrases while you type.

It learns from your own message history (for example Telegram exports) and predicts what you are likely to write next.

All processing happens locally on your machine.
No cloud, no telemetry, no external APIs.

Features

• System-wide autocomplete suggestions
• Works in Telegram, browsers, editors, and most apps
• Word and phrase prediction
• Training from Telegram JSON exports
• Lightweight ghost-text overlay
• Customizable appearance and hotkeys
• Fully local SQLite dataset

Example

Typing:

она с

GhostType suggests:

может

Typing:

я не

Suggestion:

знаю

Press Tab to accept the suggestion.

How It Works

GhostType uses a small local prediction engine:

Global keyboard hook captures typing.

Characters are resolved using the active keyboard layout.

Context is analyzed using an n-gram language model.

The most probable continuation is suggested.

A ghost text overlay shows the prediction.

The system prioritizes:

trigram predictions

bigram predictions

word prefix matches

Training the Model

GhostType can learn from exported message archives.

Example supported source:

• Telegram JSON export

The importer extracts:

words

phrase frequencies

bigram/trigram relationships

Only your own messages are used.

Privacy

GhostType is completely local.

• No internet connection required
• No analytics or tracking
• No data leaves your machine
• All data stored in SQLite

Installation

Clone the repository:

git clone https://github.com/yourname/ghosttype
cd ghosttype

Build:

cargo build --release

Run:

cargo run

Import dataset:

cargo run -- import
Configuration

Settings are stored in:

config.toml

Example:

mode = "hybrid"

prefix_length = 2

color = "#A0A0A0"
opacity = 0.7

font = "Segoe UI"
font_size = 16

accept_key = "Tab"
Project Structure
src/
  main.rs
  hook.rs
  input.rs
  db.rs
  suggest.rs
  overlay.rs
  state.rs
  telegram_import.rs
Performance

Typical usage:

CPU: <1%
RAM: 20–50 MB

GhostType is designed to run quietly in the background.

Roadmap

Planned improvements:

• full ghost-text overlay near caret
• GUI settings panel
• better language model
• faster prefix indexing
• dataset management tools

License

MIT License.