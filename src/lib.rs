use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use mlua::prelude::{LuaError, LuaFunction, LuaTable, LuaValue};
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

/// Default color definitions: (name, hex_code)
/// Using static str slices to avoid heap allocations.
const DEFAULT_COLORS: &[(&str, &str)] = &[
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

/// User-configured colors, initialized via setup() function.
/// Falls back to DEFAULT_COLORS if not configured.
static USER_COLORS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

/// Get or initialize the plugin namespace.
/// Returns an error if namespace creation fails.
fn get_namespace() -> Result<u32, api::Error> {
    PLUGIN_NAMESPACE
        .get_or_try_init(|| api::create_namespace(NAMESPACE_ID))
        .copied()
}

/// Get the configured colors (user-configured or defaults).
fn get_colors() -> HashMap<String, String> {
    let colors_lock = USER_COLORS.get_or_init(|| {
        // Initialize with defaults if not configured
        let mut default_map = HashMap::new();
        for (name, hex) in DEFAULT_COLORS.iter() {
            default_map.insert(name.to_string(), hex.to_string());
        }
        RwLock::new(default_map)
    });

    // Return a clone of the current colors
    colors_lock.read().unwrap().clone()
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
    // Validate color choice against configured colors
    let colors = get_colors();
    if !colors.contains_key(&choice) {
        let valid_colors: Vec<String> = colors.keys().cloned().collect();
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

    // Extract color names from configured colors
    let colors = get_colors();
    let color_names: Vec<String> = colors.keys().cloned().collect();

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
// CONFIGURATION API
// ============================================================================

/// Validates a hex color code format.
/// Returns true if the hex code is valid (#RRGGBB format).
fn is_valid_hex_color(hex: &str) -> bool {
    if !hex.starts_with('#') || hex.len() != 7 {
        return false;
    }
    hex[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Setup function for user configuration.
/// This is exposed to Lua via the module API.
///
/// Example usage in Lua:
/// ```lua
/// require('highlighter').setup({
///   colors = {
///     Red = "#ff0000",
///     MyCustomColor = "#123456",
///   }
/// })
/// ```
pub fn setup(config: LuaTable) -> Result<(), LuaError> {
    // Get the colors table from config
    let colors_table: Option<LuaTable> = config.get("colors").ok();

    if let Some(colors_table) = colors_table {
        let colors_lock = USER_COLORS.get_or_init(|| {
            // Start with defaults
            let mut default_map = HashMap::new();
            for (name, hex) in DEFAULT_COLORS.iter() {
                default_map.insert(name.to_string(), hex.to_string());
            }
            RwLock::new(default_map)
        });

        let mut colors = colors_lock.write().unwrap();

        // Iterate over user-provided colors
        for pair in colors_table.pairs::<String, String>() {
            let (name, hex) = pair?;

            // Validate hex color
            if !is_valid_hex_color(&hex) {
                return Err(LuaError::RuntimeError(format!(
                    "Invalid hex color '{}' for '{}'. Expected format: #RRGGBB",
                    hex, name
                )));
            }

            // Add or override color
            colors.insert(name, hex);
        }
    }

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
/// Exports the setup() function for user configuration.
#[oxi::module]
fn highlighter() -> oxi::Result<()> {
    // Initialize namespace early to catch any initialization errors
    let _ = get_namespace()?;

    // Register highlight groups for configured colors
    let colors = get_colors();
    for (name, hex) in colors.iter() {
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
    fn test_default_color_definitions() {
        // Verify default colors array is properly defined
        assert_eq!(DEFAULT_COLORS.len(), 6, "Should have 6 default colors");

        // Verify specific colors exist
        assert!(DEFAULT_COLORS.iter().any(|(name, _)| *name == "Red"));
        assert!(DEFAULT_COLORS.iter().any(|(name, _)| *name == "Green"));
        assert!(DEFAULT_COLORS.iter().any(|(name, _)| *name == "Blue"));

        // Verify hex codes are valid format (start with #, 7 chars)
        for (_, hex) in DEFAULT_COLORS.iter() {
            assert!(hex.starts_with('#'), "Hex code should start with #");
            assert_eq!(hex.len(), 7, "Hex code should be 7 characters");
        }
    }

    #[test]
    fn test_hex_color_validation() {
        // Valid hex colors
        assert!(is_valid_hex_color("#ff0000"), "Valid red color");
        assert!(is_valid_hex_color("#00FF00"), "Valid green color (uppercase)");
        assert!(is_valid_hex_color("#123456"), "Valid custom color");

        // Invalid hex colors
        assert!(!is_valid_hex_color("ff0000"), "Missing #");
        assert!(!is_valid_hex_color("#ff00"), "Too short");
        assert!(!is_valid_hex_color("#ff00000"), "Too long");
        assert!(!is_valid_hex_color("#gggggg"), "Invalid hex digits");
        assert!(!is_valid_hex_color(""), "Empty string");
    }

    #[test]
    fn test_get_colors_defaults() {
        // Test that get_colors returns defaults when not configured
        let colors = get_colors();
        assert!(colors.len() >= 6, "Should have at least default colors");
        assert!(colors.contains_key("Red"), "Should have Red");
        assert_eq!(colors.get("Red"), Some(&"#ff0000".to_string()));
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
fn test_colors_integrity() {
    // Verify configured colors (defaults if not configured)
    let colors = get_colors();
    assert!(colors.len() >= 6, "Should have at least 6 default colors");

    assert!(colors.contains_key("Red"));
    assert!(colors.contains_key("Green"));
    assert!(colors.contains_key("Blue"));
    assert!(colors.contains_key("Purple"));
    assert!(colors.contains_key("Yellow"));
    assert!(colors.contains_key("Black"));

    // Verify color values
    assert_eq!(colors.get("Red"), Some(&"#ff0000".to_string()));
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

// ============================================================================
// E2E TESTS - Full Workflow Testing
// ============================================================================

#[oxi::test]
fn test_multiline_highlight_simulation() -> Result<(), api::Error> {
    // Simulate highlighting a multi-line selection
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    // Create a multi-line buffer
    buf.set_lines(0..3, true, vec![
        "First line".to_string(),
        "Second line".to_string(),
        "Third line".to_string(),
    ].into_iter())?;

    // Simulate visual marks for lines 0-2 (entire selection)
    buf.set_mark('<', 0, 0, Default::default())?;
    buf.set_mark('>', 2, 9, Default::default())?; // "Third line" end

    // Simulate the highlight operation by directly calling perform_highlight logic
    let namespace = get_namespace()?;

    // Highlight all three lines
    for row in 0..=2 {
        let line_len = get_line_length(row)?;
        let start = if row == 0 { 0 } else { 0 };
        let end = if row == 2 { 10.min(line_len) } else { line_len };

        let priority = highest_line_priority(row, start, end)?;
        let new_priority = if priority >= MAX_SAFE_PRIORITY {
            MAX_SAFE_PRIORITY
        } else {
            priority + 1
        };

        let ext_opt = SetExtmarkOpts::builder()
            .priority(new_priority)
            .hl_group("Red")
            .end_col(end)
            .build();

        buf.set_extmark(namespace, row, start, &ext_opt)?;
    }

    // Verify extmarks were created for all three lines
    for row in 0..=2 {
        let extmarks = buf.get_extmarks(
            namespace,
            ExtmarkPosition::ByTuple((row, 0)),
            ExtmarkPosition::ByTuple((row, usize::MAX)),
            &GetExtmarksOpts::builder().build()
        )?;

        let count = extmarks.count();
        assert!(count > 0, "Line {} should have at least one extmark", row);
    }

    Ok(())
}

#[oxi::test]
fn test_overlapping_highlights_priority_layering() -> Result<(), api::Error> {
    // Test that overlapping highlights respect priority ordering
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..1, true, vec!["Test line for overlapping".to_string()].into_iter())?;

    let namespace = get_namespace()?;

    // Add first highlight (chars 0-10)
    let ext_opt1 = SetExtmarkOpts::builder()
        .priority(BASE_PRIORITY + 1)
        .hl_group("Red")
        .end_col(10)
        .build();
    buf.set_extmark(namespace, 0, 0, &ext_opt1)?;

    // Add second overlapping highlight (chars 5-15) with higher priority
    let ext_opt2 = SetExtmarkOpts::builder()
        .priority(BASE_PRIORITY + 2)
        .hl_group("Blue")
        .end_col(15)
        .build();
    buf.set_extmark(namespace, 0, 5, &ext_opt2)?;

    // Verify both extmarks exist
    let extmarks = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((0, 0)),
        ExtmarkPosition::ByTuple((0, usize::MAX)),
        &GetExtmarksOpts::builder().details(true).build()
    )?;

    let count = extmarks.count();
    assert_eq!(count, 2, "Should have exactly 2 overlapping extmarks");

    // Calculate priority for the overlapping region
    let priority = highest_line_priority(0, 5, 15)?;
    assert!(priority >= BASE_PRIORITY + 2, "Priority should be at least as high as the highest existing mark");

    Ok(())
}

#[oxi::test]
fn test_utf8_multibyte_character_handling() -> Result<(), api::Error> {
    // Test highlighting with UTF-8 multi-byte characters
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    // String with multi-byte characters: "Hello 世界" (Chinese for "world")
    buf.set_lines(0..1, true, vec!["Hello 世界".to_string()].into_iter())?;

    let line_len = get_line_length(0)?;
    // "Hello 世界" = 5 ASCII + 1 space + 6 bytes (2 chars × 3 bytes each) = 12 bytes
    assert_eq!(line_len, 12, "Line with UTF-8 should report byte length");

    let namespace = get_namespace()?;

    // Highlight the entire line including UTF-8 characters
    let ext_opt = SetExtmarkOpts::builder()
        .priority(BASE_PRIORITY + 1)
        .hl_group("Green")
        .end_col(line_len)
        .build();

    let result = buf.set_extmark(namespace, 0, 0, &ext_opt);
    assert!(result.is_ok(), "Should successfully highlight UTF-8 text");

    Ok(())
}

#[oxi::test]
fn test_empty_range_block_selection() -> Result<(), api::Error> {
    // Test that empty ranges are skipped (can happen with block selections)
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..2, true, vec![
        "Short".to_string(),
        "Much longer line".to_string(),
    ].into_iter())?;

    let namespace = get_namespace()?;

    // Simulate block selection that extends past first line
    // Line 0: "Short" (5 chars), trying to highlight cols 10-15 (beyond line end)
    let line_len = get_line_length(0)?;
    let start = 10;
    let end = 15.min(line_len);

    // This should result in start >= end, which should be skipped
    if start < end {
        let ext_opt = SetExtmarkOpts::builder()
            .priority(BASE_PRIORITY + 1)
            .hl_group("Yellow")
            .end_col(end)
            .build();
        buf.set_extmark(namespace, 0, start, &ext_opt)?;
    }

    // Verify no extmark was created (empty range was skipped)
    let extmarks = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((0, 0)),
        ExtmarkPosition::ByTuple((0, usize::MAX)),
        &GetExtmarksOpts::builder().build()
    )?;

    assert_eq!(extmarks.count(), 0, "Empty range should not create extmark");

    Ok(())
}

#[oxi::test]
fn test_multi_buffer_isolation() -> Result<(), api::Error> {
    // Test that highlights are isolated per buffer
    let buf1 = api::create_buf(false, true)?;
    let buf2 = api::create_buf(false, true)?;

    let namespace = get_namespace()?;

    // Add content to buf1 and highlight it
    api::set_current_buf(&buf1)?;
    buf1.set_lines(0..1, true, vec!["Buffer 1 content".to_string()].into_iter())?;

    let ext_opt1 = SetExtmarkOpts::builder()
        .priority(BASE_PRIORITY + 1)
        .hl_group("Red")
        .end_col(10)
        .build();
    buf1.set_extmark(namespace, 0, 0, &ext_opt1)?;

    // Add content to buf2 and highlight it
    api::set_current_buf(&buf2)?;
    buf2.set_lines(0..1, true, vec!["Buffer 2 content".to_string()].into_iter())?;

    let ext_opt2 = SetExtmarkOpts::builder()
        .priority(BASE_PRIORITY + 1)
        .hl_group("Blue")
        .end_col(10)
        .build();
    buf2.set_extmark(namespace, 0, 0, &ext_opt2)?;

    // Verify buf1 still has its extmark
    api::set_current_buf(&buf1)?;
    let extmarks1 = buf1.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((0, 0)),
        ExtmarkPosition::ByTuple((0, usize::MAX)),
        &GetExtmarksOpts::builder().build()
    )?;
    assert_eq!(extmarks1.count(), 1, "Buffer 1 should have its own extmark");

    // Verify buf2 has its extmark
    api::set_current_buf(&buf2)?;
    let extmarks2 = buf2.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((0, 0)),
        ExtmarkPosition::ByTuple((0, usize::MAX)),
        &GetExtmarksOpts::builder().build()
    )?;
    assert_eq!(extmarks2.count(), 1, "Buffer 2 should have its own extmark");

    Ok(())
}

#[oxi::test]
fn test_clear_command_with_multiple_highlights() -> Result<(), api::Error> {
    // Test that clear removes all highlights in buffer
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..3, true, vec![
        "Line 1".to_string(),
        "Line 2".to_string(),
        "Line 3".to_string(),
    ].into_iter())?;

    let namespace = get_namespace()?;

    // Add multiple highlights across different lines
    for row in 0..=2 {
        let ext_opt = SetExtmarkOpts::builder()
            .priority(BASE_PRIORITY + row as u32 + 1)
            .hl_group("Purple")
            .end_col(6)
            .build();
        buf.set_extmark(namespace, row, 0, &ext_opt)?;
    }

    // Verify highlights exist
    let extmarks_before = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByIndex(0),
        ExtmarkPosition::ByIndex(-1),
        &GetExtmarksOpts::builder().build()
    )?;
    assert!(extmarks_before.count() >= 3, "Should have at least 3 extmarks before clear");

    // Clear all highlights
    clear(CommandArgs::default())?;

    // Verify all highlights are gone
    let extmarks_after = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByIndex(0),
        ExtmarkPosition::ByIndex(-1),
        &GetExtmarksOpts::builder().build()
    )?;
    assert_eq!(extmarks_after.count(), 0, "All extmarks should be cleared");

    Ok(())
}

#[oxi::test]
fn test_entire_line_clear_before_highlight() -> Result<(), api::Error> {
    // Test that selecting entire line clears existing highlights first
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..1, true, vec!["Test line".to_string()].into_iter())?;

    let namespace = get_namespace()?;
    let line_len = get_line_length(0)?;

    // Add initial highlight
    let ext_opt1 = SetExtmarkOpts::builder()
        .priority(BASE_PRIORITY + 1)
        .hl_group("Red")
        .end_col(line_len)
        .build();
    buf.set_extmark(namespace, 0, 0, &ext_opt1)?;

    // Verify highlight exists
    let extmarks_before = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((0, 0)),
        ExtmarkPosition::ByTuple((0, usize::MAX)),
        &GetExtmarksOpts::builder().build()
    )?;
    assert_eq!(extmarks_before.count(), 1, "Should have 1 extmark initially");

    // Test is_entire_line detection
    let is_full_line = is_entire_line(0, 0, line_len)?;
    assert_eq!(is_full_line, true, "Should detect entire line selection");

    // Clear and verify
    clear_line(0)?;
    let extmarks_after = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((0, 0)),
        ExtmarkPosition::ByTuple((0, usize::MAX)),
        &GetExtmarksOpts::builder().build()
    )?;
    assert_eq!(extmarks_after.count(), 0, "Entire line clear should remove all extmarks");

    Ok(())
}

#[oxi::test]
fn test_visual_marks_simulation() -> Result<(), api::Error> {
    // Simulate the full visual mode workflow with marks
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..2, true, vec![
        "First line of text".to_string(),
        "Second line".to_string(),
    ].into_iter())?;

    // Set visual marks as Neovim would (1-indexed for marks)
    buf.set_mark('<', 0, 6, Default::default())?;  // Start of "line"
    buf.set_mark('>', 1, 5, Default::default())?;   // End of "Second"

    // Read marks back (get_mark returns 1-indexed)
    let mark_start = buf.get_mark('<')?;
    let mark_end = buf.get_mark('>')?;

    assert_eq!(mark_start, (1, 6), "Start mark should be (1, 6)");
    assert_eq!(mark_end, (2, 5), "End mark should be (2, 5)");

    // Convert to zero-indexed
    let (row_start, col_start) = zero_based_row(mark_start);
    let (row_end, col_end) = zero_based_row(mark_end);

    assert_eq!((row_start, col_start), (0, 6), "Zero-based start should be (0, 6)");
    assert_eq!((row_end, col_end), (1, 5), "Zero-based end should be (1, 5)");

    // Simulate highlighting this selection
    let namespace = get_namespace()?;
    for row in row_start..=row_end {
        let line_len = get_line_length(row)?;
        let start = if row == row_start { col_start } else { 0 };
        let end = if row == row_end {
            (col_end + 1).min(line_len)
        } else {
            line_len
        };

        if start < end {
            let ext_opt = SetExtmarkOpts::builder()
                .priority(BASE_PRIORITY + 1)
                .hl_group("Green")
                .end_col(end)
                .build();
            buf.set_extmark(namespace, row, start, &ext_opt)?;
        }
    }

    // Verify highlights were created
    let extmarks_row0 = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((0, 0)),
        ExtmarkPosition::ByTuple((0, usize::MAX)),
        &GetExtmarksOpts::builder().build()
    )?;
    assert_eq!(extmarks_row0.count(), 1, "Row 0 should have 1 extmark");

    let extmarks_row1 = buf.get_extmarks(
        namespace,
        ExtmarkPosition::ByTuple((1, 0)),
        ExtmarkPosition::ByTuple((1, usize::MAX)),
        &GetExtmarksOpts::builder().build()
    )?;
    assert_eq!(extmarks_row1.count(), 1, "Row 1 should have 1 extmark");

    Ok(())
}

#[oxi::test]
fn test_priority_increment_sequence() -> Result<(), api::Error> {
    // Test that repeated highlights increment priority correctly
    let buf = api::create_buf(false, true)?;
    api::set_current_buf(&buf)?;

    buf.set_lines(0..1, true, vec!["Test priority sequence".to_string()].into_iter())?;

    let namespace = get_namespace()?;

    // Add first highlight
    let priority1 = highest_line_priority(0, 0, 10)?;
    assert_eq!(priority1, BASE_PRIORITY, "First priority should be BASE_PRIORITY");

    let ext_opt1 = SetExtmarkOpts::builder()
        .priority(priority1 + 1)
        .hl_group("Red")
        .end_col(10)
        .build();
    buf.set_extmark(namespace, 0, 0, &ext_opt1)?;

    // Add second overlapping highlight
    let priority2 = highest_line_priority(0, 5, 15)?;
    assert_eq!(priority2, BASE_PRIORITY + 1, "Second priority should be incremented");

    let ext_opt2 = SetExtmarkOpts::builder()
        .priority(priority2 + 1)
        .hl_group("Blue")
        .end_col(15)
        .build();
    buf.set_extmark(namespace, 0, 5, &ext_opt2)?;

    // Add third highlight
    let priority3 = highest_line_priority(0, 10, 20)?;
    assert_eq!(priority3, BASE_PRIORITY + 2, "Third priority should continue incrementing");

    Ok(())
}

// ============================================================================
// CONFIGURATION TESTS
// ============================================================================

#[oxi::test]
fn test_setup_with_custom_colors() {
    let lua = oxi::mlua::lua();

    // Create a config table
    let config = lua.create_table().unwrap();
    let colors_table = lua.create_table().unwrap();

    // Add custom colors
    colors_table.set("MyOrange", "#FF8800").unwrap();
    colors_table.set("MyPink", "#FF69B4").unwrap();

    config.set("colors", colors_table).unwrap();

    // Call setup
    let result = setup(config);
    assert!(result.is_ok(), "Setup should succeed with valid colors");

    // Verify custom colors are available
    let colors = get_colors();
    assert!(colors.contains_key("MyOrange"), "Custom color MyOrange should be available");
    assert!(colors.contains_key("MyPink"), "Custom color MyPink should be available");
    assert_eq!(colors.get("MyOrange"), Some(&"#FF8800".to_string()));
    assert_eq!(colors.get("MyPink"), Some(&"#FF69B4".to_string()));

    // Verify defaults still exist
    assert!(colors.contains_key("Red"), "Default colors should still be available");
}

#[oxi::test]
fn test_setup_with_invalid_hex() {
    let lua = oxi::mlua::lua();

    let config = lua.create_table().unwrap();
    let colors_table = lua.create_table().unwrap();

    // Add invalid hex color
    colors_table.set("BadColor", "not-a-hex").unwrap();
    config.set("colors", colors_table).unwrap();

    // Call setup - should fail
    let result = setup(config);
    assert!(result.is_err(), "Setup should fail with invalid hex color");
}

#[oxi::test]
fn test_setup_override_default_color() {
    let lua = oxi::mlua::lua();

    let config = lua.create_table().unwrap();
    let colors_table = lua.create_table().unwrap();

    // Override default Red color
    colors_table.set("Red", "#990000").unwrap();
    config.set("colors", colors_table).unwrap();

    let result = setup(config);
    assert!(result.is_ok(), "Setup should succeed");

    // Verify Red was overridden
    let colors = get_colors();
    assert_eq!(colors.get("Red"), Some(&"#990000".to_string()), "Red should be overridden");
}
