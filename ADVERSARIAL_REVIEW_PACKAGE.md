# Adversarial Review Package - Round 1
**Date**: 2026-01-05
**VDD Phase**: Initial Adversarial Review

---

## Instructions for Adversarial Reviewer

You are a hyper-critical code reviewer with zero patience for sloppy code, lazy patterns, or unhandled edge cases. Your job is to tear this code apart and find every flaw, weakness, and potential failure point. Do not be polite. Do not assume good intentions. Assume the worst and find it.

### Specific Focus Areas
- Unhandled edge cases and error conditions
- Performance inefficiencies and code smell
- Security vulnerabilities
- Incomplete or misleading documentation
- Logic gaps and hidden assumptions
- Technical debt and maintainability issues
- Incorrect use of APIs
- Race conditions and concurrency issues
- Memory leaks or resource management problems

---

## Project Context

**Name**: highlighter.nvim
**Purpose**: A Neovim plugin for visually highlighting text selections with colors
**Language**: Rust + Neovim Lua integration
**Framework**: nvim-oxi (Rust bindings for Neovim API)

### Core Functionality
1. User selects text in Visual mode
2. Presses `<C-h>` to trigger highlighter
3. Selects a color from a picker UI
4. Plugin creates extmarks with appropriate priority to render highlights
5. User can clear all highlights with `<leader>ch`

---

## Code Under Review

### File: src/lib.rs (Complete Source)

```rust
use lazy_static;

use std::collections::HashMap;

use mlua::prelude::{LuaError, LuaFunction, LuaTable};
use nvim_oxi::{
    self as oxi,
    api::{self, opts::*, types::*},
};

lazy_static::lazy_static! {
    static ref COLORS : HashMap<String, String> =  HashMap::from([
        ("Red".to_string(), "#ff0000".to_string()),
        ("Green".to_string(), "#00ff00".to_string()),
        ("Blue".to_string(), "#0000ff".to_string()),
        ("Purple".to_string(), "#A020F0".to_string()),
        ("Yellow".to_string(), "#ffff00".to_string()),
        ("Black".to_string(), "#000000".to_string()),
    ]);
    static ref PLUGIN : u32 = api::create_namespace("highlighter");
}

fn entire_line(row: usize, col_start: usize, col_end: usize) -> bool {
    col_start == 0 && col_end == end_of_line(row)
}

fn clear_line(row: usize) {
    api::get_current_buf()
        .clear_namespace(*PLUGIN, row..=row + 1)
        .unwrap();
}

fn highest_line_priority(row: usize, col_start: usize, col_end: usize) -> u32 {
    let get_ext_opt = GetExtmarksOpts::builder().details(true).build();
    let start_extmark = ExtmarkPosition::ByTuple((row, col_start));
    let end_extmark = ExtmarkPosition::ByTuple((row, col_end));

    let extmark = api::get_current_buf()
        .get_extmarks(*PLUGIN, start_extmark, end_extmark, &get_ext_opt)
        .expect("cannot get mark");
    let mut max_priority: u32 = 200;
    if entire_line(row, col_start, col_end) {
        oxi::dbg!("Clearing LIne");
        clear_line(row);
        return max_priority;
    }
    extmark.for_each(|(_, _, _, opts)| {
        if let Some(priority) = opts
            .expect("GetExtmarksOpts details should be set")
            .priority
        {
            max_priority = max_priority.max(priority);
        }
    });
    return max_priority;
}

fn end_of_line(row: usize) -> usize {
    let current_line = api::get_current_buf()
        .get_lines(row..=row + 1, true)
        .unwrap()
        .next()
        .unwrap();
    current_line.len()
}

fn zero_based_row(tup: (usize, usize)) -> (usize, usize) {
    (tup.0 - 1, tup.1)
}

fn perform_highlight<T>(_: T, choice: String) -> Result<(), LuaError> {
    let mode = api::get_mode().unwrap();
    if mode.mode == Mode::Normal {
        //Get visual marks
        let (visual_row_start, visual_col_start) =
            zero_based_row(api::get_current_buf().get_mark('<').unwrap());
        let (visual_row_end, visual_col_end) =
            zero_based_row(api::get_current_buf().get_mark('>').unwrap());
        oxi::dbg!(visual_row_start..=visual_row_end);

        for row in visual_row_start..=visual_row_end {
            let (mut start, mut end) = (0, end_of_line(row));
            if row == visual_row_start {
                start = visual_col_start;
            }
            if row == visual_row_end {
                end = end.min(visual_col_end)
            }
            //Set new color
            let new_priority = highest_line_priority(row, start, end) + 1;
            let ext_opt = SetExtmarkOpts::builder()
                .priority(new_priority)
                .hl_group(&choice)
                .end_col(end)
                .build();
            oxi::dbg!(row, start, end, new_priority);
            let _ = api::get_current_buf().set_extmark(*PLUGIN, row, start, &ext_opt);
        }
    }
    Ok(())
}
fn prompt_for_color_option() {
    let lua = oxi::mlua::lua();
    let items = lua
        .create_sequence_from(COLORS.keys().map(|s| s.as_str()).collect::<Vec<_>>())
        .unwrap();
    let opts = lua.create_table_from([("prompt", "Pick a color")]).unwrap();
    let perform_highlight_callback = lua.create_function(perform_highlight).unwrap();
    let select = lua
        .globals()
        .get::<_, LuaTable>("vim")
        .expect("cannot get vim")
        .get::<_, LuaTable>("ui")
        .expect("cannot get vim.ui")
        .get::<_, LuaFunction>("select")
        .expect("cannot get vim.ui.select");

    select
        .call::<_, ()>((items, opts, perform_highlight_callback))
        .unwrap();
}

pub fn highlight(_args: CommandArgs) -> Result<(), api::Error> {
    prompt_for_color_option();
    Ok(())
}

pub fn clear(_args: CommandArgs) -> Result<(), api::Error> {
    api::get_current_buf()
        .clear_namespace(0, 0..=usize::max_value())
        .unwrap();
    Ok(())
}

#[oxi::module]
fn highlighter() -> oxi::Result<()> {
    for (key, value) in COLORS.iter() {
        let highlight_opt = &SetHighlightOpts::builder().foreground(value).build();

        let high_id = api::set_hl(0, &key, highlight_opt);
        oxi::print!("highlight id: {:?}", high_id);
    }
    let opts = CreateCommandOpts::builder().build();
    api::create_user_command("Highlighter", highlight, &opts)?;
    api::create_user_command("HighlighterClear", clear, &opts)?;
    let key_opts = SetKeymapOpts::builder().build();
    api::set_keymap(Mode::Visual, "<C-h>", "<Esc>:Highlighter<CR>", &key_opts)?;
    api::set_keymap(
        Mode::Normal,
        "<leader>ch",
        ":HighlighterClear<CR>",
        &key_opts,
    )?;
    Ok(())
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_based_row_conversion() {
        // Test normal case
        assert_eq!(zero_based_row((1, 0)), (0, 0));
        assert_eq!(zero_based_row((5, 10)), (4, 10));
        assert_eq!(zero_based_row((100, 50)), (99, 50));

        // Test edge cases
        assert_eq!(zero_based_row((1, usize::MAX)), (0, usize::MAX));
    }

    #[test]
    fn test_entire_line_detection() {
        // This would need to be tested with oxi::test as it requires end_of_line
        // which makes API calls. For now, we'll test the logic directly.
        // Entire line: col_start == 0 && col_end == end_of_line(row)

        // Mock scenario: line with 50 characters
        let col_start = 0;
        let col_end = 50;
        let line_length = 50;
        assert_eq!(col_start == 0 && col_end == line_length, true);

        // Not entire line: doesn't start at 0
        let col_start = 5;
        assert_eq!(col_start == 0 && col_end == line_length, false);

        // Not entire line: doesn't end at line_length
        let col_start = 0;
        let col_end = 25;
        assert_eq!(col_start == 0 && col_end == line_length, false);
    }
}

// ============================================================================
// NEOVIM INTEGRATION TESTS
// ============================================================================

#[oxi::test]
fn test_highlight_namespace_creation() {
    // Verify the namespace is created and has a valid ID
    assert!(*PLUGIN > 0, "Plugin namespace should have a valid ID");
}

#[oxi::test]
fn test_colors_map_initialization() {
    // Verify all expected colors are present
    assert_eq!(COLORS.len(), 6, "Should have 6 predefined colors");
    assert!(COLORS.contains_key("Red"));
    assert!(COLORS.contains_key("Green"));
    assert!(COLORS.contains_key("Blue"));
    assert!(COLORS.contains_key("Purple"));
    assert!(COLORS.contains_key("Yellow"));
    assert!(COLORS.contains_key("Black"));

    // Verify color format
    assert_eq!(COLORS.get("Red"), Some(&"#ff0000".to_string()));
    assert_eq!(COLORS.get("Green"), Some(&"#00ff00".to_string()));
    assert_eq!(COLORS.get("Blue"), Some(&"#0000ff".to_string()));
}

#[oxi::test]
fn test_end_of_line_empty_buffer() {
    // Create a test buffer with known content
    let buf = api::create_buf(false, true).unwrap();
    api::set_current_buf(&buf).unwrap();

    // Set a line with known length
    buf.set_lines(0..1, true, vec!["Hello World".to_string()].into_iter())
        .unwrap();

    let eol = end_of_line(0);
    assert_eq!(eol, 11, "Line 'Hello World' should be 11 characters");
}

#[oxi::test]
fn test_clear_command_on_empty_buffer() -> Result<(), api::Error> {
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    // Should not panic on empty buffer
    let result = clear(CommandArgs::default());
    assert!(result.is_ok(), "Clear should succeed on empty buffer");
    Ok(())
}
```

---

## Pre-Identified Issues (Do NOT limit yourself to these)

From initial code review, the following issues are OBVIOUS:

1. **Panic City**: `.unwrap()` and `.expect()` everywhere - 18+ instances
2. **Priority Overflow**: `let new_priority = highest_line_priority(row, start, end) + 1;` - What happens at u32::MAX?
3. **Magic Number**: `let mut max_priority: u32 = 200;` - Why 200? What if existing extmarks have priority > 200?
4. **Namespace Confusion**: `clear()` uses namespace `0` instead of `*PLUGIN`
5. **Unused Return**: `let _ = api::get_current_buf().set_extmark(...)` - Silently ignoring potential errors
6. **Typo in Debug**: `oxi::dbg!("Clearing LIne");` - "LIne" typo
7. **No Validation**: `choice: String` parameter not validated against COLORS
8. **Range Issue**: `clear_namespace(*PLUGIN, row..=row + 1)` - Why `row + 1`? Potential off-by-one?
9. **Double Return**: `return max_priority;` after early return in `highest_line_priority`
10. **Visual Mark Assumptions**: Assumes `<` and `>` marks always exist and are valid

### Questions to Answer with Extreme Prejudice

1. What happens if the buffer is closed while highlighting?
2. What happens with UTF-8 multi-byte characters? Does `.len()` give byte or char count?
3. What if `visual_col_end < visual_col_start`?
4. What if the user cancels the color picker?
5. What if Neovim doesn't support the required API version?
6. Is there a memory leak with extmarks never being garbage collected?
7. What if another plugin uses the same keybindings?
8. What if the buffer is read-only?
9. What if `row + 1` in `clear_line` exceeds buffer line count?
10. What's the behavior with folded text?

---

## Build System

### make.sh
```bash
#!/bin/bash
set -e

build() {
  echo "Building silicon.nvim from source..."

  cargo build --release --target-dir ./target

  # Place the compiled library where Neovim can find it.
  mkdir -p lua

  if [ "$(uname)" == "Darwin" ]; then
    mv target/release/libhighlighter.dylib lua/highlighter.so
  elif [ "$(expr substr $(uname -s) 1 5)" == "Linux" ]; then
    mv target/release/libhighlighter.so lua/highlighter.so
  elif [ "$(expr substr $(uname -s) 1 10)" == "MINGW64_NT" ]; then
    mv target/release/highlighter.dll lua/highlighter.dll
  fi
}

build
```

**Issues?**
- Echo says "silicon.nvim" but project is "highlighter" - copy-paste error?
- No error handling if build fails
- `expr substr` is archaic - why not use proper bash patterns?
- No verification that the file was actually created

---

## Cargo.toml
```toml
[package]
name = "highlighter"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
lazy_static = "1.4.0"
mlua = { version = "0.8.8", features = ["luajit52"] }
nvim-oxi = { version = "0.2.2", features = ["neovim-0-8", "test", "mlua"] }
```

**Issues?**
- No repository, authors, or license metadata
- nvim-oxi 0.2.2 is ancient (2023) - are there critical bugs/security issues?
- No dev-dependencies for testing
- No description

---

## Expected Deliverables

Provide a brutally honest critique in the following format:

### 1. Critical Flaws (Must Fix)
List issues that will cause data loss, panics, or security vulnerabilities.

### 2. Major Issues (Should Fix)
List issues that cause poor UX, performance problems, or maintainability nightmares.

### 3. Code Smell (Consider Fixing)
List minor issues, stylistic problems, or potential future tech debt.

### 4. Architecture Concerns
Question fundamental design decisions. Is this the right approach?

### 5. Missing Functionality
What SHOULD be here but isn't?

### 6. Test Coverage Gaps
What critical paths are not tested?

### 7. Documentation Lies
Where does the documentation claim something the code doesn't deliver?

---

## VDD Context

This is **Round 1** of adversarial review. The code was just instrumented with basic tests. Your job is to find everything wrong so the Builder can iterate.

**Remember**: Assume incompetence. Assume malice. Assume every edge case will be hit in production. Tear this apart.
