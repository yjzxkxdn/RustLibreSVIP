//! Terminal output helpers: a `rich`-compatible table renderer and colour
//! detection.
//!
//! The Python CLI renders `plugin list` with `rich.table.Table`. Column widths,
//! padding, shrinking and box-drawing characters are reproduced here so the
//! plain-text output (pipes, CI) matches the reference implementation.

use std::io::IsTerminal;

/// Display width of a string, treating East Asian wide characters as two cells
/// (the same rule `rich`/`wcwidth` applies).
pub fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(character: char) -> usize {
    let code = character as u32;
    if code == 0 {
        return 0;
    }
    // Combining marks and zero-width spaces.
    if matches!(code, 0x0300..=0x036F | 0x200B..=0x200F | 0xFE00..=0xFE0F | 0xFEFF) {
        return 0;
    }
    if is_wide(code) {
        2
    } else {
        1
    }
}

fn is_wide(code: u32) -> bool {
    matches!(code,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE10..=0xFE19
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD
    )
}

pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// Console width used by `rich` (`COLUMNS` wins, then the terminal, then 80).
pub fn console_width() -> usize {
    if let Ok(value) = std::env::var("COLUMNS") {
        if let Ok(width) = value.trim().parse::<usize>() {
            if width > 0 {
                return width;
            }
        }
    }
    if std::io::stdout().is_terminal() {
        terminal_width().unwrap_or(80)
    } else {
        80
    }
}

fn terminal_width() -> Option<usize> {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleScreenBufferInfo(
                handle: *mut core::ffi::c_void,
                info: *mut ConsoleScreenBufferInfo,
            ) -> i32;
            fn GetStdHandle(which: u32) -> *mut core::ffi::c_void;
        }
        #[repr(C)]
        struct Coord {
            x: i16,
            y: i16,
        }
        #[repr(C)]
        struct SmallRect {
            left: i16,
            top: i16,
            right: i16,
            bottom: i16,
        }
        #[repr(C)]
        struct ConsoleScreenBufferInfo {
            size: Coord,
            cursor_position: Coord,
            attributes: u16,
            window: SmallRect,
            maximum_window_size: Coord,
        }
        unsafe {
            let handle = GetStdHandle(0xFFFF_FFF5); // STD_OUTPUT_HANDLE
            let mut info = ConsoleScreenBufferInfo {
                size: Coord { x: 0, y: 0 },
                cursor_position: Coord { x: 0, y: 0 },
                attributes: 0,
                window: SmallRect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                maximum_window_size: Coord { x: 0, y: 0 },
            };
            if GetConsoleScreenBufferInfo(handle, &mut info) != 0 {
                let width = (info.window.right - info.window.left + 1) as usize;
                if width > 0 {
                    return Some(width);
                }
            }
        }
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
}

/// `rich.cells.set_cell_size`: resize text to exactly `total` cells.
fn set_cell_size(text: &str, total: usize) -> String {
    let mut out = String::new();
    let mut accumulated = 0usize;
    for character in text.chars() {
        let character_width = char_width(character);
        if accumulated + character_width > total {
            if accumulated < total {
                out.push_str(&" ".repeat(total - accumulated));
            }
            return out;
        }
        out.push(character);
        accumulated += character_width;
    }
    if accumulated < total {
        out.push_str(&" ".repeat(total - accumulated));
    }
    out
}

/// `rich.text.Text.rstrip_end`: crop trailing whitespace using the **character**
/// length (rich compares `len(text)` against the cell width here).
fn rstrip_end(text: &mut String, width: usize) {
    let char_len = text.chars().count();
    if char_len > width {
        let excess = char_len - width;
        let whitespace_count = text.chars().rev().take_while(|c| c.is_whitespace()).count();
        let crop = whitespace_count.min(excess);
        let keep = char_len - crop;
        *text = text.chars().take(keep).collect();
    }
}

/// Port of `rich._wrap.divide_line` with `fold=False` (cell overflow mode).
fn divide_offsets(text: &str, width: usize) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut breaks: Vec<usize> = Vec::new();
    let mut cell_offset: i64 = 0;
    let mut pos = 0usize;
    while pos < chars.len() {
        let start = pos;
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < chars.len() && !chars[pos].is_whitespace() {
            pos += 1;
        }
        let word_end = pos;
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        let word: String = chars[start..pos].iter().collect();
        if word.is_empty() {
            break;
        }
        let trimmed: String = chars[start..word_end].iter().collect::<String>().trim().to_owned();
        let word_length = display_width(&trimmed) as i64;
        let remaining_space = width as i64 - cell_offset;
        if remaining_space >= word_length {
            cell_offset += display_width(&word) as i64;
        } else if word_length > width as i64 {
            if start > 0 {
                breaks.push(start);
            }
            cell_offset = display_width(&word) as i64;
        } else if cell_offset > 0 && start > 0 {
            breaks.push(start);
            // `divide_line` uses the cell length of the word *including* its
            // trailing whitespace here.
            cell_offset = display_width(&word) as i64;
        }
        if word_end == start {
            break;
        }
    }
    breaks
}

/// `rich.cells.chop_cells`: split a word into chunks that fit `width` cells.
fn chop_cells(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for character in text.chars() {
        let character_width = char_width(character);
        if current_width + character_width > width && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(character);
        current_width += character_width;
    }
    if !current.is_empty() || chunks.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// `divide_line(..., fold=True)` — like [`divide_offsets`] but overlong words are
/// folded instead of cropped.
fn divide_offsets_fold(text: &str, width: usize) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut breaks: Vec<usize> = Vec::new();
    let mut cell_offset: i64 = 0;
    let mut pos = 0usize;
    while pos < chars.len() {
        let start = pos;
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < chars.len() && !chars[pos].is_whitespace() {
            pos += 1;
        }
        let word_end = pos;
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        let word: String = chars[start..pos].iter().collect();
        if word.is_empty() {
            break;
        }
        let trimmed: String = chars[start..word_end].iter().collect::<String>().trim().to_owned();
        let word_length = display_width(&trimmed) as i64;
        let remaining_space = width as i64 - cell_offset;
        if remaining_space >= word_length {
            cell_offset += display_width(&word) as i64;
        } else if word_length > width as i64 {
            let chunks = chop_cells(&word, width);
            let mut offset = start;
            let last_index = chunks.len().saturating_sub(1);
            for (index, chunk) in chunks.iter().enumerate() {
                if offset > 0 {
                    breaks.push(offset);
                }
                if index == last_index {
                    cell_offset = display_width(chunk) as i64;
                } else {
                    offset += chunk.chars().count();
                }
            }
        } else if cell_offset > 0 && start > 0 {
            breaks.push(start);
            cell_offset = display_width(&word) as i64;
        }
        if word_end == start {
            break;
        }
    }
    breaks
}

/// Rich `Text.wrap` with `overflow="fold"` (used for prompts): word wrap with
/// hard breaks for overlong words, trailing whitespace kept when it fits.
pub fn wrap_fold(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let chars: Vec<char> = text.chars().collect();
    let breaks = divide_offsets_fold(text, width);
    let mut lines: Vec<String> = Vec::new();
    let mut previous = 0usize;
    for point in breaks {
        if point > previous {
            lines.push(chars[previous..point].iter().collect());
            previous = point;
        }
    }
    if previous < chars.len() {
        lines.push(chars[previous..].iter().collect());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
        .into_iter()
        .map(|mut line| {
            rstrip_end(&mut line, width);
            if display_width(&line) > width {
                line = set_cell_size(&line, width);
            }
            line
        })
        .collect()
}

/// Rich-compatible cell wrapping (`Column(overflow="ellipsis")`, the default).
pub fn wrap_cell(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let chars: Vec<char> = text.chars().collect();
    let breaks = divide_offsets(text, width);
    let mut slices: Vec<String> = Vec::new();
    let mut previous = 0usize;
    for point in breaks {
        if point > previous {
            slices.push(chars[previous..point].iter().collect());
            previous = point;
        }
    }
    if previous < chars.len() {
        slices.push(chars[previous..].iter().collect());
    }
    if slices.is_empty() {
        slices.push(String::new());
    }
    slices
        .into_iter()
        .map(|mut line| {
            rstrip_end(&mut line, width);
            if display_width(&line) > width {
                line = set_cell_size(&line, width - 1);
                line.push('\u{2026}');
            }
            line
        })
        .collect()
}

/// A rich-style table.
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Render every column with the `cyan` style (rich `add_column(style="cyan")`).
    pub color: bool,
}

impl Table {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>, color: bool) -> Self {
        Self {
            headers,
            rows,
            color,
        }
    }

    fn natural_widths(&self) -> Vec<usize> {
        let mut widths: Vec<usize> = self.headers.iter().map(|h| display_width(h)).collect();
        for row in &self.rows {
            for (index, cell) in row.iter().enumerate() {
                if index < widths.len() {
                    widths[index] = widths[index].max(display_width(cell));
                }
            }
        }
        widths
    }

    /// `_calculate_column_widths`: shrink wrappable columns so the table fits.
    fn column_widths(&self, max_width: Option<usize>) -> Vec<usize> {
        let natural = self.natural_widths();
        let Some(max_width) = max_width else {
            return natural;
        };
        let columns = natural.len();
        // borders (columns + 1) + padding (2 per column)
        let overhead = (columns + 1) + columns * 2;
        let Some(available) = max_width.checked_sub(overhead) else {
            return vec![1; columns];
        };
        let total: usize = natural.iter().sum();
        if total <= available {
            return natural;
        }
        // Columns whose cells cannot wrap (no spaces) keep their natural width.
        let wrappable: Vec<bool> = (0..columns)
            .map(|index| {
                self.rows
                    .iter()
                    .any(|row| row.get(index).map(|cell| cell.contains(' ')).unwrap_or(false))
            })
            .collect();
        let fixed_sum: usize = natural
            .iter()
            .zip(wrappable.iter())
            .filter(|(_, wrappable)| !**wrappable)
            .map(|(width, _)| *width)
            .sum();
        let mut widths = natural.clone();
        let flexible_count = wrappable.iter().filter(|flag| **flag).count();
        if flexible_count == 0 {
            // Nothing can wrap: shrink every column proportionally.
            let scale = available as f64 / total as f64;
            return natural
                .iter()
                .map(|width| ((*width as f64 * scale).floor() as usize).max(1))
                .collect();
        }
        let available_flexible = available.saturating_sub(fixed_sum);
        let share = available_flexible / flexible_count;
        let mut assigned = 0usize;
        let mut flexible_indices: Vec<usize> = Vec::new();
        for index in 0..columns {
            if !wrappable[index] {
                continue;
            }
            flexible_indices.push(index);
            let width = natural[index].min(share);
            widths[index] = width;
            assigned += width;
        }
        // Spread the remainder, smallest natural width first (rich's behaviour).
        let mut leftover = available_flexible.saturating_sub(assigned);
        let mut order = flexible_indices.clone();
        order.sort_by_key(|index| natural[*index]);
        while leftover > 0 {
            let mut progress = false;
            for index in &order {
                if leftover == 0 {
                    break;
                }
                if widths[*index] < natural[*index] {
                    widths[*index] += 1;
                    leftover -= 1;
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }
        widths.iter().map(|width| (*width).max(1)).collect()
    }

    fn render_row(&self, cells: &[String], widths: &[usize], header: bool) -> Vec<String> {
        let wrapped: Vec<Vec<String>> = widths
            .iter()
            .enumerate()
            .map(|(index, width)| {
                let cell = cells.get(index).map(String::as_str).unwrap_or("");
                wrap_cell(cell.trim_end(), *width)
            })
            .collect();
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        let mut lines = Vec::new();
        for line_index in 0..height {
            let mut line = String::from("│");
            for (column, width) in widths.iter().enumerate() {
                let content = wrapped[column]
                    .get(line_index)
                    .map(String::as_str)
                    .unwrap_or("");
                let padding = width.saturating_sub(display_width(content));
                line.push(' ');
                if self.color {
                    let style = if header { "\x1b[1;35m" } else { "\x1b[36m" };
                    line.push_str(style);
                    line.push_str(content);
                    line.push_str("\x1b[0m");
                } else {
                    line.push_str(content);
                }
                line.push_str(&" ".repeat(padding));
                line.push(' ');
                line.push('│');
            }
            lines.push(line);
        }
        lines
    }

    /// Renders the table (no trailing newline).
    pub fn render(&self) -> String {
        self.render_with_width(Some(console_width().saturating_sub(1)))
    }

    pub fn render_with_width(&self, max_width: Option<usize>) -> String {
        let widths = self.column_widths(max_width);
        let mut out = String::new();
        let border = |left: char, middle: char, right: char| {
            let mut line = String::new();
            line.push(left);
            for (index, width) in widths.iter().enumerate() {
                if index > 0 {
                    line.push(middle);
                }
                line.push_str(&"─".repeat(width + 2));
            }
            line.push(right);
            line
        };

        out.push_str(&border('┌', '┬', '┐'));
        out.push('\n');
        for line in self.render_row(&self.headers, &widths, true) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str(&border('├', '┼', '┤'));
        out.push('\n');
        for row in &self.rows {
            for line in self.render_row(row, &widths, false) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out.push_str(&border('└', '┴', '┘'));
        out
    }
}

/// `typer.echo`.
pub fn echo(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

/// `typer.secho(..., err=True)`.
pub fn eecho(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}
