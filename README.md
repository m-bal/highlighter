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

## Configuration

### Basic Setup (Optional)

If you want to use the default colors, no configuration is needed! Just install and use.

For custom colors, call `setup()` in your Neovim config:

```lua
require('highlighter').setup({
  colors = {
    -- Add custom colors
    Orange = "#FF8800",
    Pink = "#FF69B4",
    Cyan = "#00FFFF",

    -- Override default colors
    Red = "#FF0000",  -- brighter red
  }
})
```

### Default Colors

If you don't call `setup()`, these colors are available by default:

| Color  | Hex Code  |
|--------|-----------|
| Red    | `#ff0000` |
| Green  | `#00ff00` |
| Blue   | `#0000ff` |
| Purple | `#A020F0` |
| Yellow | `#ffff00` |
| Black  | `#000000` |

### Custom Keybindings

Don't like the default keybindings? Disable them and set your own:

```lua
-- Disable default keybindings
vim.keymap.del('v', '<C-h>')
vim.keymap.del('n', '<leader>ch')

-- Set custom keybindings
vim.keymap.set('v', '<leader>h', '<Esc>:Highlighter<CR>', { desc = "Highlight selection" })
vim.keymap.set('n', '<leader>H', ':HighlighterClear<CR>', { desc = "Clear highlights" })
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
4. Choose a color from the picker (including your custom colors!)
5. Repeat for additional highlights
6. Press `<leader>ch` to clear when done

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

### ✅ Recently Fixed

**VDD Round 4** (v0.3.0 - Configuration API):
- ✅ User-configurable colors via setup() function
- ✅ Hex color validation (#RRGGBB format)
- ✅ Thread-safe color storage with RwLock
- ✅ Custom color support (add new colors)
- ✅ Default color override support
- ✅ 4 new configuration tests (31 total tests, ~80% coverage)

**VDD Round 3** (v0.2.0 - E2E Testing):
- ✅ Comprehensive E2E test suite (28 total tests, ~75% coverage)
- ✅ Multi-line selection workflow tested
- ✅ Overlapping highlight priority verification
- ✅ UTF-8 multi-byte character handling tested
- ✅ Block selection edge cases covered
- ✅ Multi-buffer isolation verified
- ✅ Visual marks simulation tested
- ✅ Priority increment sequences validated

**VDD Round 2** (v0.2.0):
- ✅ Safe initialization with OnceLock (no startup panics)
- ✅ Error propagation instead of silent failures
- ✅ User-friendly error messages
- ✅ Unique namespace identifier ("highlighter.nvim")
- ✅ Proper buffer range handling (no usize::MAX)
- ✅ Zero-allocation color definitions
- ✅ Removed lazy_static dependency
- ✅ Keymap conflict documented with workaround
- ✅ Empty range handling for block selections

**VDD Round 1** (v0.1.0):
- ✅ All `.unwrap()` calls replaced with proper error handling
- ✅ Priority overflow protection with MAX_SAFE_PRIORITY
- ✅ Color validation before applying highlights
- ✅ Namespace bug in clear command fixed
- ✅ Off-by-one error in clear_line range fixed

### Remaining Known Issues
- ⚠️ `<C-h>` keybinding conflicts with terminal backspace (disable with `vim.keymap.del('v', '<C-h>')`)
- ⚠️ No persistence of highlights across sessions (planned in Epic 6.1.2)

### Notes
- UTF-8 multi-byte characters: This plugin uses byte-based indexing, which is the correct behavior for Neovim's extmark API. Character positions are handled automatically by Neovim.

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
