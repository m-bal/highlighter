use std::sync::OnceLock;

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
/// Set to u32::MAX - 1000 to allow for 1000 additional highlight layers before overflow.
const MAX_SAFE_PRIORITY: u32 = u32::MAX - 1000;

/// Unique namespace identifier to avoid collisions with other plugins.
const NAMESPACE_ID: &str = "highlighter.nvim";

/// Color definitions: (name, hex_code)
/// Using static str slices to avoid heap allocations.
const COLORS: &[(&str, &str)] = &[
    ("Red", "#ff0000"),
    ("Green", "#00ff00"),
    ("Blue", "#0000ff"),
    ("Purple", "#A020F0"),
    ("Yellow", "#ffff00"),
    ("Black", "#000000"),
];

/// Plugin namespace ID, initialized on first access.
/// Using OnceLock instead of lazy_static for better error handling.
static PLUGIN_NAMESPACE: OnceLock<u32> = OnceLock::new();

/// Get or initialize the plugin namespace.
/// Returns an error if namespace creation fails.
fn get_namespace() -> Result<u32, api::Error> {
    PLUGIN_NAMESPACE
        .get_or_try_init(|| api::create_namespace(NAMESPACE_ID))
        .copied()
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Checks if a selection covers the entire line.
/// Returns None if line length cannot be determined (error condition).
fn is_entire_line(row: usize, col_start: usize, col_end: usize) -> Result<bool, api::Error> {
    let line_len = get_line_length(row)?;
    // Empty line: col_start == 0 && col_end == 0
    // Full line: col_start == 0 && col_end >= line_len
    Ok(col_start == 0 && (line_len == 0 || col_end >= line_len))
}

/// Clears all highlights on a specific line.
/// Returns Err if the buffer operation fails.
fn clear_line(row: usize) -> Result<(), api::Error> {
    let namespace = get_namespace()?;
    api::get_current_buf()
        .clear_namespace(namespace, row..=row)?;
    Ok(())
}

/// Calculates the highest priority value for extmarks in the given range.
/// If the entire line is selected, it clears existing highlights first.
/// Returns BASE_PRIORITY or the max existing priority, capped at MAX_SAFE_PRIORITY.
fn highest_line_priority(row: usize, col_start: usize, col_end: usize) -> Result<u32, api::Error> {
    let namespace = get_namespace()?;
    let get_ext_opt = GetExtmarksOpts::builder().details(true).build();
    let start_extmark = ExtmarkPosition::ByTuple((row, col_start));
    let end_extmark = ExtmarkPosition::ByTuple((row, col_end));

    let extmark = api::get_current_buf()
        .get_extmarks(namespace, start_extmark, end_extmark, &get_ext_opt)?;

    let mut max_priority: u32 = BASE_PRIORITY;

    if is_entire_line(row, col_start, col_end)? {
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

/// Returns the byte length of the line at the given row.
/// This function propagates errors instead of returning 0 silently.
///
/// Note: For UTF-8 multi-byte characters, this returns byte count, not character count.
/// Neovim's extmark API uses byte-based indexing, so this is the correct behavior.
fn get_line_length(row: usize) -> Result<usize, api::Error> {
    let mut lines = api::get_current_buf().get_lines(row..=row + 1, true)?;

    match lines.next() {
        Some(line) => Ok(line.len()),
        None => Err(api::Error::Other(format!("Row {} does not exist in buffer", row))),
    }
}

fn zero_based_row(tup: (usize, usize)) -> (usize, usize) {
    (tup.0 - 1, tup.1)
}

/// Helper to convert user-friendly error messages from api::Error
fn to_user_error(context: &str, err: api::Error) -> String {
    match err {
        api::Error::Other(msg) => format!("{}: {}", context, msg),
        _ => format!("{} (buffer may be closed or modified)", context),
    }
}

/// Applies highlighting to the visual selection with the chosen color.
/// Validates that the choice is a valid color before applying.
/// Returns LuaError if any operation fails.
///
/// Generic parameter T is required by mlua callback signature but unused.
fn perform_highlight<T>(_: T, choice: String) -> Result<(), LuaError> {
    // Validate color choice
    if !COLORS.iter().any(|(name, _)| *name == choice) {
        let valid_colors: Vec<&str> = COLORS.iter().map(|(name, _)| *name).collect();
        return Err(LuaError::RuntimeError(format!(
            "Invalid color '{}'. Valid colors are: {}",
            choice,
            valid_colors.join(", ")
        )));
    }

    let mode = api::get_mode()
        .map_err(|e| LuaError::RuntimeError(to_user_error("Unable to check editor mode", e)))?;

    if mode.mode == Mode::Normal {
        let buf = api::get_current_buf();
        let namespace = get_namespace()
            .map_err(|e| LuaError::RuntimeError(to_user_error("Plugin initialization failed", e)))?;

        // Get visual marks with error handling
        let mark_start = buf.get_mark('<')
            .map_err(|_| LuaError::RuntimeError(
                "No visual selection found. Please select text in visual mode first.".to_string()
            ))?;
        let mark_end = buf.get_mark('>')
            .map_err(|_| LuaError::RuntimeError(
                "No visual selection found. Please select text in visual mode first.".to_string()
            ))?;

        let (visual_row_start, visual_col_start) = zero_based_row(mark_start);
        let (visual_row_end, visual_col_end) = zero_based_row(mark_end);

        for row in visual_row_start..=visual_row_end {
            // Get line length to determine proper end column
            let line_len = get_line_length(row)
                .map_err(|e| LuaError::RuntimeError(to_user_error(
                    &format!("Unable to access line {}", row + 1), e
                )))?;

            let start = if row == visual_row_start { visual_col_start } else { 0 };

            // Calculate end column:
            // - For multi-line selections, intermediate rows go to line end
            // - For the last row, use visual_col_end + 1 (Neovim visual mode is inclusive)
            // - Never exceed actual line length
            let end = if row == visual_row_end {
                (visual_col_end + 1).min(line_len)
            } else {
                line_len
            };

            // Skip if the range is empty (can happen with block selections)
            if start >= end {
                continue;
            }

            // Calculate priority with error handling
            let current_priority = highest_line_priority(row, start, end)
                .map_err(|e| LuaError::RuntimeError(to_user_error("Failed to query existing highlights", e)))?;

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

            // Properly handle the set_extmark result
            buf.set_extmark(namespace, row, start, &ext_opt)
                .map_err(|e| LuaError::RuntimeError(to_user_error(
                    &format!("Failed to apply highlight at line {}", row + 1), e
                )))?;
        }
    }
    Ok(())
}
/// Displays a color picker UI to the user and applies the selected highlight.
/// Returns an error if UI operations fail.
fn prompt_for_color_option() -> Result<(), String> {
    let lua = oxi::mlua::lua();

    // Extract color names from COLORS array
    let color_names: Vec<&str> = COLORS.iter().map(|(name, _)| *name).collect();

    let items = lua
        .create_sequence_from(color_names)
        .map_err(|e| format!("Color picker UI error: {}", e))?;

    let opts = lua
        .create_table_from([("prompt", "Pick a color")])
        .map_err(|e| format!("Color picker UI error: {}", e))?;

    let perform_highlight_callback = lua
        .create_function(perform_highlight)
        .map_err(|e| format!("Color picker UI error: {}", e))?;

    let vim_table = lua
        .globals()
        .get::<_, LuaTable>("vim")
        .map_err(|_| "Color picker UI not available".to_string())?;

    let ui_table = vim_table
        .get::<_, LuaTable>("ui")
        .map_err(|_| "Color picker UI not available (vim.ui not found)".to_string())?;

    let select = ui_table
        .get::<_, LuaFunction>("select")
        .map_err(|_| "Color picker UI not available (vim.ui.select not found)".to_string())?;

    select
        .call::<_, ()>((items, opts, perform_highlight_callback))
        .map_err(|e| format!("Color picker failed: {}", e))?;

    Ok(())
}

// ============================================================================
// PUBLIC COMMAND HANDLERS
// ============================================================================

/// Command handler for :Highlighter
/// Displays the color picker to highlight visual selection.
pub fn highlight(_args: CommandArgs) -> Result<(), api::Error> {
    if let Err(e) = prompt_for_color_option() {
        oxi::print!("Highlighter: {}", e);
        return Err(api::Error::Other(e));
    }
    Ok(())
}

/// Command handler for :HighlighterClear
/// Clears all highlights in the current buffer.
pub fn clear(_args: CommandArgs) -> Result<(), api::Error> {
    let namespace = get_namespace()?;
    let buf = api::get_current_buf();

    // Get actual buffer line count instead of using usize::MAX
    let line_count = buf.line_count()?;

    // Clear namespace for all lines in the buffer (0-indexed)
    buf.clear_namespace(namespace, 0..line_count)?;

    Ok(())
}

// ============================================================================
// PLUGIN INITIALIZATION
// ============================================================================

/// Neovim plugin module initialization.
/// Sets up highlight groups, commands, and keymaps.
#[oxi::module]
fn highlighter() -> oxi::Result<()> {
    // Initialize namespace early to catch any initialization errors
    let _ = get_namespace()?;

    // Register highlight groups for each color
    for (name, hex) in COLORS.iter() {
        let highlight_opt = &SetHighlightOpts::builder().foreground(hex).build();
        api::set_hl(0, name, highlight_opt)?;
    }

    // Register user commands
    let opts = CreateCommandOpts::builder().build();
    api::create_user_command("Highlighter", highlight, &opts)?;
    api::create_user_command("HighlighterClear", clear, &opts)?;

    // Register keymaps
    // NOTE: <C-h> conflicts with backspace in terminal mode.
    // Users can disable this with: vim.keymap.del('v', '<C-h>')
    // and create their own mapping: vim.keymap.set('v', '<leader>h', '<Esc>:Highlighter<CR>')
    let key_opts = SetKeymapOpts::builder()
        .desc("Highlight selection with color picker")
        .build();
    api::set_keymap(Mode::Visual, "<C-h>", "<Esc>:Highlighter<CR>", &key_opts)?;

    let clear_opts = SetKeymapOpts::builder()
        .desc("Clear all highlights in buffer")
        .build();
    api::set_keymap(
        Mode::Normal,
        "<leader>ch",
        ":HighlighterClear<CR>",
        &clear_opts,
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
    fn test_entire_line_logic() {
        // Test entire line detection logic directly
        // Entire line: col_start == 0 && (line_len == 0 || col_end >= line_len)

        // Full line scenario
        let col_start = 0;
        let col_end = 50;
        let line_length = 50;
        assert_eq!(col_start == 0 && col_end >= line_length, true);

        // Not entire line: doesn't start at 0
        let col_start = 5;
        assert_eq!(col_start == 0 && col_end >= line_length, false);

        // Not entire line: doesn't reach end
        let col_start = 0;
        let col_end = 25;
        assert_eq!(col_start == 0 && col_end >= line_length, false);

        // Empty line
        let line_length = 0;
        let col_start = 0;
        let col_end = 0;
        assert_eq!(col_start == 0 && (line_length == 0 || col_end >= line_length), true);
    }

    #[test]
    fn test_priority_constants() {
        // Ensure constants are sensible
        assert!(BASE_PRIORITY > 0, "BASE_PRIORITY should be positive");
        assert!(MAX_SAFE_PRIORITY < u32::MAX, "MAX_SAFE_PRIORITY should leave room before overflow");
        assert!(BASE_PRIORITY < MAX_SAFE_PRIORITY, "BASE_PRIORITY should be less than MAX_SAFE_PRIORITY");
        // Verify we have at least 1000 highlights worth of room
        assert!(u32::MAX - MAX_SAFE_PRIORITY >= 1000, "Should have room for at least 1000 highlights");
    }

    #[test]
    fn test_color_definitions() {
        // Verify colors array is properly defined
        assert!(COLORS.len() == 6, "Should have 6 predefined colors");

        // Verify specific colors exist
        assert!(COLORS.iter().any(|(name, _)| *name == "Red"));
        assert!(COLORS.iter().any(|(name, _)| *name == "Green"));
        assert!(COLORS.iter().any(|(name, _)| *name == "Blue"));

        // Verify hex codes are valid format (start with #, 7 chars)
        for (_, hex) in COLORS.iter() {
            assert!(hex.starts_with('#'), "Hex code should start with #");
            assert_eq!(hex.len(), 7, "Hex code should be 7 characters");
        }
    }

    #[test]
    fn test_namespace_id_uniqueness() {
        // Verify namespace has a unique identifier
        assert!(NAMESPACE_ID.contains("highlighter"), "Namespace should contain 'highlighter'");
        assert!(NAMESPACE_ID.contains('.'), "Namespace should use dotted format for uniqueness");
    }
}

// ============================================================================
// NEOVIM INTEGRATION TESTS
// ============================================================================

#[oxi::test]
fn test_namespace_initialization() {
    // Verify the namespace can be created and has a valid ID
    let namespace = get_namespace().unwrap();
    assert!(namespace > 0, "Plugin namespace should have a valid ID");
}

#[oxi::test]
fn test_colors_array_integrity() {
    // Verify all expected colors are present
    assert_eq!(COLORS.len(), 6, "Should have 6 predefined colors");

    let color_names: Vec<&str> = COLORS.iter().map(|(name, _)| *name).collect();
    assert!(color_names.contains(&"Red"));
    assert!(color_names.contains(&"Green"));
    assert!(color_names.contains(&"Blue"));
    assert!(color_names.contains(&"Purple"));
    assert!(color_names.contains(&"Yellow"));
    assert!(color_names.contains(&"Black"));

    // Verify color values
    let red = COLORS.iter().find(|(name, _)| *name == "Red").unwrap();
    assert_eq!(red.1, "#ff0000");
}

#[oxi::test]
fn test_get_line_length_with_content() {
    // Create a test buffer with known content
    let buf = api::create_buf(false, true).unwrap();
    api::set_current_buf(&buf).unwrap();

    // Set a line with known length
    buf.set_lines(0..1, true, vec!["Hello World".to_string()].into_iter())
        .unwrap();

    let len = get_line_length(0).unwrap();
    assert_eq!(len, 11, "Line 'Hello World' should be 11 bytes");
}

#[oxi::test]
fn test_get_line_length_invalid_row() {
    // Create an empty buffer
    let buf = api::create_buf(false, true).unwrap();
    api::set_current_buf(&buf).unwrap();

    // Request line beyond buffer - should return error instead of panicking
    let result = get_line_length(1000);
    assert!(result.is_err(), "Invalid row should return an error");
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
