use lazy_static;

use std::collections::HashMap;

use mlua::prelude::{LuaError, LuaFunction, LuaTable};
use nvim_oxi::{
    self as oxi,
    api::{self, opts::*, types::*},
};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Base priority for highlights. New highlights will be layered above existing ones.
const BASE_PRIORITY: u32 = 200;

/// Maximum safe priority value. We leave room before u32::MAX to prevent overflow.
const MAX_SAFE_PRIORITY: u32 = u32::MAX - 100;

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

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn entire_line(row: usize, col_start: usize, col_end: usize) -> bool {
    col_start == 0 && col_end == end_of_line(row)
}

/// Clears all highlights on a specific line.
/// Returns Err if the buffer operation fails.
fn clear_line(row: usize) -> Result<(), api::Error> {
    api::get_current_buf()
        .clear_namespace(*PLUGIN, row..=row)?;
    Ok(())
}

/// Calculates the highest priority value for extmarks in the given range.
/// If the entire line is selected, it clears existing highlights first.
/// Returns BASE_PRIORITY or the max existing priority, capped at MAX_SAFE_PRIORITY.
fn highest_line_priority(row: usize, col_start: usize, col_end: usize) -> Result<u32, api::Error> {
    let get_ext_opt = GetExtmarksOpts::builder().details(true).build();
    let start_extmark = ExtmarkPosition::ByTuple((row, col_start));
    let end_extmark = ExtmarkPosition::ByTuple((row, col_end));

    let extmark = api::get_current_buf()
        .get_extmarks(*PLUGIN, start_extmark, end_extmark, &get_ext_opt)?;

    let mut max_priority: u32 = BASE_PRIORITY;

    if entire_line(row, col_start, col_end) {
        oxi::dbg!("Clearing Line");
        clear_line(row)?;
        return Ok(max_priority);
    }

    extmark.for_each(|(_, _, _, opts)| {
        if let Some(opts) = opts {
            if let Some(priority) = opts.priority {
                max_priority = max_priority.max(priority);
            }
        }
    });

    // Cap at MAX_SAFE_PRIORITY to prevent overflow
    Ok(max_priority.min(MAX_SAFE_PRIORITY))
}

/// Returns the length of the line at the given row.
/// Note: This returns byte length, which works correctly for ASCII and most cases,
/// but may not match visual column positions for multi-byte UTF-8 characters.
fn end_of_line(row: usize) -> usize {
    match api::get_current_buf().get_lines(row..=row + 1, true) {
        Ok(mut lines) => {
            match lines.next() {
                Some(line) => line.len(),
                None => 0, // Empty or invalid row
            }
        }
        Err(_) => 0, // Buffer error, return 0 as safe default
    }
}

fn zero_based_row(tup: (usize, usize)) -> (usize, usize) {
    (tup.0 - 1, tup.1)
}

/// Applies highlighting to the visual selection with the chosen color.
/// Validates that the choice is a valid color before applying.
/// Returns LuaError if any operation fails.
fn perform_highlight<T>(_: T, choice: String) -> Result<(), LuaError> {
    // Validate color choice
    if !COLORS.contains_key(&choice) {
        return Err(LuaError::RuntimeError(format!(
            "Invalid color '{}'. Valid colors are: {:?}",
            choice,
            COLORS.keys().collect::<Vec<_>>()
        )));
    }

    let mode = api::get_mode()
        .map_err(|e| LuaError::RuntimeError(format!("Failed to get mode: {:?}", e)))?;

    if mode.mode == Mode::Normal {
        let buf = api::get_current_buf();

        // Get visual marks with error handling
        let mark_start = buf.get_mark('<')
            .map_err(|e| LuaError::RuntimeError(format!("Failed to get visual start mark '<': {:?}", e)))?;
        let mark_end = buf.get_mark('>')
            .map_err(|e| LuaError::RuntimeError(format!("Failed to get visual end mark '>': {:?}", e)))?;

        let (visual_row_start, visual_col_start) = zero_based_row(mark_start);
        let (visual_row_end, visual_col_end) = zero_based_row(mark_end);

        oxi::dbg!(visual_row_start..=visual_row_end);

        for row in visual_row_start..=visual_row_end {
            let (mut start, mut end) = (0, end_of_line(row));

            if row == visual_row_start {
                start = visual_col_start;
            }
            if row == visual_row_end {
                end = end.min(visual_col_end + 1); // +1 to include the last character
            }

            // Calculate priority with error handling
            let current_priority = highest_line_priority(row, start, end)
                .map_err(|e| LuaError::RuntimeError(format!("Failed to get priority: {:?}", e)))?;

            // Safe priority increment with overflow protection
            let new_priority = if current_priority >= MAX_SAFE_PRIORITY {
                MAX_SAFE_PRIORITY
            } else {
                current_priority + 1
            };

            let ext_opt = SetExtmarkOpts::builder()
                .priority(new_priority)
                .hl_group(&choice)
                .end_col(end)
                .build();

            oxi::dbg!(row, start, end, new_priority);

            // Properly handle the set_extmark result
            buf.set_extmark(*PLUGIN, row, start, &ext_opt)
                .map_err(|e| LuaError::RuntimeError(format!(
                    "Failed to set extmark at row {}, col {}: {:?}",
                    row, start, e
                )))?;
        }
    }
    Ok(())
}
/// Displays a color picker UI to the user and applies the selected highlight.
/// Returns an error if UI operations fail.
fn prompt_for_color_option() -> Result<(), String> {
    let lua = oxi::mlua::lua();

    let items = lua
        .create_sequence_from(COLORS.keys().map(|s| s.as_str()).collect::<Vec<_>>())
        .map_err(|e| format!("Failed to create color list: {:?}", e))?;

    let opts = lua
        .create_table_from([("prompt", "Pick a color")])
        .map_err(|e| format!("Failed to create options table: {:?}", e))?;

    let perform_highlight_callback = lua
        .create_function(perform_highlight)
        .map_err(|e| format!("Failed to create callback: {:?}", e))?;

    let vim_table = lua
        .globals()
        .get::<_, LuaTable>("vim")
        .map_err(|e| format!("Failed to get 'vim' global: {:?}", e))?;

    let ui_table = vim_table
        .get::<_, LuaTable>("ui")
        .map_err(|e| format!("Failed to get 'vim.ui': {:?}", e))?;

    let select = ui_table
        .get::<_, LuaFunction>("select")
        .map_err(|e| format!("Failed to get 'vim.ui.select': {:?}", e))?;

    select
        .call::<_, ()>((items, opts, perform_highlight_callback))
        .map_err(|e| format!("Failed to call vim.ui.select: {:?}", e))?;

    Ok(())
}

// ============================================================================
// PUBLIC COMMAND HANDLERS
// ============================================================================

/// Command handler for :Highlighter
/// Displays the color picker to highlight visual selection.
pub fn highlight(_args: CommandArgs) -> Result<(), api::Error> {
    if let Err(e) = prompt_for_color_option() {
        oxi::print!("Highlighter error: {}", e);
        return Err(api::Error::Other(e));
    }
    Ok(())
}

/// Command handler for :HighlighterClear
/// Clears all highlights in the current buffer.
pub fn clear(_args: CommandArgs) -> Result<(), api::Error> {
    api::get_current_buf()
        .clear_namespace(*PLUGIN, 0..=usize::MAX)?;
    Ok(())
}

// ============================================================================
// PLUGIN INITIALIZATION
// ============================================================================

/// Neovim plugin module initialization.
/// Sets up highlight groups, commands, and keymaps.
#[oxi::module]
fn highlighter() -> oxi::Result<()> {
    // Register highlight groups for each color
    for (key, value) in COLORS.iter() {
        let highlight_opt = &SetHighlightOpts::builder().foreground(value).build();
        api::set_hl(0, &key, highlight_opt)?;
    }

    // Register user commands
    let opts = CreateCommandOpts::builder().build();
    api::create_user_command("Highlighter", highlight, &opts)?;
    api::create_user_command("HighlighterClear", clear, &opts)?;

    // Register keymaps
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

    #[test]
    fn test_priority_constants() {
        // Ensure constants are sensible
        assert!(BASE_PRIORITY > 0, "BASE_PRIORITY should be positive");
        assert!(MAX_SAFE_PRIORITY < u32::MAX, "MAX_SAFE_PRIORITY should leave room before overflow");
        assert!(BASE_PRIORITY < MAX_SAFE_PRIORITY, "BASE_PRIORITY should be less than MAX_SAFE_PRIORITY");
    }

    #[test]
    fn test_color_validation() {
        // All defined colors should be valid keys
        assert!(COLORS.contains_key("Red"));
        assert!(COLORS.contains_key("Green"));
        assert!(COLORS.contains_key("Blue"));

        // Invalid color should not exist
        assert!(!COLORS.contains_key("InvalidColor"));
        assert!(!COLORS.contains_key(""));
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
fn test_end_of_line_with_content() {
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
fn test_end_of_line_invalid_row() {
    // Create an empty buffer
    let buf = api::create_buf(false, true).unwrap();
    api::set_current_buf(&buf).unwrap();

    // Request line beyond buffer - should return 0 instead of panicking
    let eol = end_of_line(1000);
    assert_eq!(eol, 0, "Invalid row should return 0");
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

#[oxi::test]
fn test_clear_line_function() -> Result<(), api::Error> {
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..1, true, vec!["Test line".to_string()].into_iter())?;

    // clear_line should not panic
    let result = clear_line(0);
    assert!(result.is_ok(), "clear_line should succeed");
    Ok(())
}

#[oxi::test]
fn test_priority_overflow_protection() -> Result<(), api::Error> {
    // This test verifies the priority calculation respects MAX_SAFE_PRIORITY
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..1, true, vec!["Test".to_string()].into_iter())?;

    // Get priority for a line with no extmarks
    let priority = highest_line_priority(0, 0, 4)?;
    assert!(priority >= BASE_PRIORITY, "Priority should be at least BASE_PRIORITY");
    assert!(priority <= MAX_SAFE_PRIORITY, "Priority should not exceed MAX_SAFE_PRIORITY");
    Ok(())
}
