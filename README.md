# Highlighter.nvim

A Neovim plugin for visually highlighting text with customizable colors, written in Rust using `nvim-oxi`.

## Features

- 🎨 **Color Highlighting**: Highlight text selections with 6 predefined colors (Red, Green, Blue, Purple, Yellow, Black)
- 🔄 **Priority Management**: Automatically layers overlapping highlights with smart priority handling
- ⌨️ **Intuitive Keybindings**: Quick access via Visual mode keybinding
- 🧹 **Easy Cleanup**: Clear all highlights with a single command

## Requirements

### Build Dependencies
- Rust toolchain (1.60+)
- LuaJIT development files
  - **Ubuntu/Debian**: `sudo apt-get install libluajit-5.1-dev`
  - **Arch Linux**: `sudo pacman -S luajit`
  - **macOS**: `brew install luajit`
  - **Fedora**: `sudo dnf install luajit-devel`
- pkg-config

### Runtime Dependencies
- Neovim 0.8+

## Installation

### Building from Source

1. Clone the repository:
```bash
git clone https://github.com/yourusername/highlighter.nvim
cd highlighter
```

2. Build the plugin:
```bash
./make.sh
```

This will compile the Rust code and place the library in the `lua/` directory where Neovim can load it.

### Plugin Manager Installation

#### lazy.nvim
```lua
{
  'yourusername/highlighter.nvim',
  build = './make.sh',
  config = function()
    -- Plugin auto-configures on load
  end
}
```

#### packer.nvim
```lua
use {
  'yourusername/highlighter.nvim',
  run = './make.sh'
}
```

#### vim-plug
```vim
Plug 'yourusername/highlighter.nvim', { 'do': './make.sh' }
```

## Usage

### Commands

- `:Highlighter` - Opens color picker UI to highlight the current visual selection
- `:HighlighterClear` - Clears all highlights in the current buffer

### Default Keybindings

- `<C-h>` (Visual mode) - Trigger highlighter on selected text
- `<leader>ch` (Normal mode) - Clear all highlights

### Workflow Example

1. Enter Visual mode (`v`, `V`, or `<C-v>`)
2. Select the text you want to highlight
3. Press `<C-h>`
4. Choose a color from the picker
5. Repeat for additional highlights
6. Press `<leader>ch` to clear when done

## Available Colors

| Color  | Hex Code  |
|--------|-----------|
| Red    | `#ff0000` |
| Green  | `#00ff00` |
| Blue   | `#0000ff` |
| Purple | `#A020F0` |
| Yellow | `#ffff00` |
| Black  | `#000000` |

## How It Works

Highlighter uses Neovim's extmarks API to create persistent, priority-managed highlights:

1. **Extmarks**: Text highlights are stored as extmarks with namespace isolation
2. **Priority System**: Each new highlight gets priority = max(existing_priorities) + 1
3. **Layering**: Higher priority highlights render on top of lower priority ones
4. **Entire Line Detection**: Selecting an entire line clears previous highlights for clean state

## Architecture

```
src/lib.rs
├── COLORS (lazy_static)          # Predefined color map
├── PLUGIN (lazy_static)          # Namespace ID
├── entire_line()                 # Detect full-line selections
├── clear_line()                  # Clear highlights for a line
├── highest_line_priority()       # Calculate next priority value
├── end_of_line()                 # Get line length
├── zero_based_row()              # Convert 1-based to 0-based indexing
├── perform_highlight()           # Main highlighting logic
├── prompt_for_color_option()     # UI for color selection
├── highlight()                   # :Highlighter command handler
├── clear()                       # :HighlighterClear command handler
└── highlighter()                 # Module initialization
```

## Development

### Running Tests

```bash
cargo test
```

Tests are split into:
- **Unit tests** (`#[cfg(test)]`): Pure Rust logic without Neovim
- **Integration tests** (`#[oxi::test]`): Tests requiring Neovim runtime

### Code Quality

This project follows **Verification-Driven Development (VDD)** methodology. See [VDD_HIERARCHY.md](VDD_HIERARCHY.md) for the complete Epic/Issue breakdown.

## Known Issues

See [VDD_HIERARCHY.md](VDD_HIERARCHY.md) for tracked issues and technical debt.

### ✅ Recently Fixed (VDD Round 1)
- ✅ All `.unwrap()` calls replaced with proper error handling
- ✅ Priority overflow protection with MAX_SAFE_PRIORITY
- ✅ Color validation before applying highlights
- ✅ Namespace bug in clear command fixed
- ✅ Off-by-one error in clear_line range fixed

### Remaining Known Issues
- ⚠️ UTF-8 multi-byte character positions may not align with visual columns (documented)
- ⚠️ No configuration API for custom colors (planned in Epic 6)
- ⚠️ No persistence of highlights across sessions (planned in Epic 6)

## Contributing

1. Review the VDD hierarchy document
2. Pick an Epic/Issue/Sub-issue to tackle
3. Write tests first (TDD)
4. Submit PR with test coverage

## Roadmap

See [VDD_HIERARCHY.md](VDD_HIERARCHY.md) for the complete roadmap including:

- [ ] Epic 1: Core Functionality Robustness
- [ ] Epic 2: Testing Infrastructure (In Progress)
- [ ] Epic 3: Error Handling & Recovery
- [ ] Epic 4: Performance Optimization
- [ ] Epic 5: Documentation & Usability
- [ ] Epic 6: Configuration & Extensibility
- [ ] Epic 7: Build & Distribution

## License

[Add your license here]

## Acknowledgments

Built with:
- [nvim-oxi](https://github.com/noib3/nvim-oxi) - Rust bindings for Neovim
- [mlua](https://github.com/khvzak/mlua) - High-level Lua bindings
