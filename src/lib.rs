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

#[oxi::test]
fn set_get_del_var() {}
