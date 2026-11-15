//! The order a row of cells is drawn in when it holds right-to-left text.
//!
//! The screen keeps every row in logical order: the order its text was
//! written in, which is the order applications address, select and copy it
//! in. Hebrew and Arabic are read right to left, so a row holding them is
//! drawn in another order, the visual one. This module works that order out
//! with the Unicode Bidirectional Algorithm and answers, for either order,
//! which column a cell holds in the other.
//!
//! A row is not one paragraph. Panes sit side by side on it, and a pane may
//! draw boxes of its own. Text never crosses from one of those into the next,
//! so the row is cut into pieces at pane edges and at line-drawing
//! characters, and each piece is ordered on its own.

use unicode_bidi::{BidiInfo, Level};
pub use zellij_utils::input::terminal::BidiMode;
use zellij_utils::structured_render::GeometryRecord;

use crate::emulation::screen_buffer::Occupancy;
use crate::emulation::terminal::TerminalState;

/// What ordering needs to know of one cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BidiCell {
    /// The first character of the cell's text.
    pub lead: char,
    /// The cell belongs to the unit starting to its left: it is the tail of
    /// a wide character, or is grouped with the cell before it. A unit moves
    /// as one and keeps its cells in order.
    pub attached: bool,
}

/// The visual order of one row.
#[derive(Debug, Default)]
pub struct RowOrder {
    /// The logical column drawn at each visual column.
    visual: Vec<usize>,
    /// Whether each logical column sits in a right-to-left run.
    rtl: Vec<bool>,
    text: String,
    starts: Vec<usize>,
    offsets: Vec<usize>,
}

impl RowOrder {
    /// The logical column drawn at the visual column `visual`.
    pub fn logical(&self, visual: usize) -> usize {
        self.visual.get(visual).copied().unwrap_or(visual)
    }

    /// The visual column the logical column `logical` is drawn at.
    pub fn visual(&self, logical: usize) -> usize {
        if logical >= self.visual.len() {
            return logical;
        }
        self.visual
            .iter()
            .position(|column| *column == logical)
            .unwrap_or(logical)
    }

    /// Whether the logical column `logical` is read right to left.
    pub fn is_rtl(&self, logical: usize) -> bool {
        self.rtl.get(logical).copied().unwrap_or(false)
    }

    /// The logical column drawn at each visual column, left to right.
    pub fn columns(&self) -> &[usize] {
        &self.visual
    }

    /// Works out the order `cells` are drawn in, and returns whether it
    /// differs at all from the order they are held in. When it does not, the
    /// order is left empty and every column maps to itself.
    pub fn arrange(
        &mut self,
        cells: &[BidiCell],
        geometry: &GeometryRecord,
        row: usize,
        mode: BidiMode,
    ) -> bool {
        self.visual.clear();
        self.rtl.clear();
        let everywhere = mode == BidiMode::Rtl;
        if mode == BidiMode::Off || cells.is_empty() {
            return false;
        }
        if !everywhere && !cells.iter().any(|cell| is_right_to_left(cell.lead)) {
            return false;
        }
        self.visual.extend(0..cells.len());
        self.rtl.resize(cells.len(), false);

        let mut differs = false;
        let mut start = 0;
        while start < cells.len() {
            if separates(&cells[start]) {
                start += 1;
                continue;
            }
            let span = Span::at(geometry, row, start);
            let mut end = start + 1;
            while end < cells.len()
                && !separates(&cells[end])
                && Span::at(geometry, row, end).id == span.id
            {
                end += 1;
            }
            let base = match mode {
                BidiMode::Rtl if !span.fixed => Some(Level::rtl()),
                BidiMode::Auto if !span.fixed => None,
                _ => Some(Level::ltr()),
            };
            let reads_leftward = base != Some(Level::ltr())
                || cells[start..end]
                    .iter()
                    .any(|cell| is_right_to_left(cell.lead));
            if reads_leftward {
                differs |= self.arrange_piece(cells, start, end, base);
            }
            start = end;
        }
        if !differs {
            self.visual.clear();
            self.rtl.clear();
        }
        differs
    }

    fn arrange_piece(
        &mut self,
        cells: &[BidiCell],
        start: usize,
        end: usize,
        base: Option<Level>,
    ) -> bool {
        let RowOrder {
            visual,
            rtl,
            text,
            starts,
            offsets,
        } = self;
        text.clear();
        starts.clear();
        offsets.clear();
        for (col, cell) in cells.iter().enumerate().take(end).skip(start) {
            if cell.attached && !starts.is_empty() {
                continue;
            }
            starts.push(col);
            offsets.push(text.len());
            text.push(within_a_paragraph(cell.lead));
        }

        let info = BidiInfo::new(text.as_str(), base);
        let [paragraph] = info.paragraphs.as_slice() else {
            return false;
        };
        let (levels, runs) = info.visual_runs(paragraph, paragraph.range.clone());

        let mut differs = false;
        let mut at = start;
        for run in runs {
            let leftward = levels[run.start].is_rtl();
            let first = offsets.partition_point(|offset| *offset < run.start);
            let last = offsets.partition_point(|offset| *offset < run.end);
            let mut place = |unit: usize| {
                let from = starts[unit];
                let to = starts.get(unit + 1).copied().unwrap_or(end);
                for col in from..to {
                    if at >= end {
                        return;
                    }
                    differs |= at != col || leftward;
                    visual[at] = col;
                    rtl[col] = leftward;
                    at += 1;
                }
            };
            if leftward {
                (first..last).rev().for_each(&mut place);
            } else {
                (first..last).for_each(&mut place);
            }
        }
        differs
    }
}

/// The stretch of a row one cell belongs to. Cells are only ever reordered
/// among those sharing their stretch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    id: usize,
    /// The stretch is not a pane's own text (a frame, a bar, the gap between
    /// panes) and always reads left to right.
    fixed: bool,
}

impl Span {
    fn at(geometry: &GeometryRecord, row: usize, col: usize) -> Span {
        if geometry.panes.is_empty() {
            return Span {
                id: 0,
                fixed: false,
            };
        }
        let outside = Span { id: 0, fixed: true };
        let (Ok(x), Ok(y)) = (u16::try_from(col), u16::try_from(row)) else {
            return outside;
        };
        let Some(index) = geometry.panes.iter().rposition(|pane| pane.contains(x, y)) else {
            return outside;
        };
        let pane = &geometry.panes[index];
        if pane.content_contains(x, y) {
            Span {
                id: index * 2 + 2,
                fixed: pane.plugin_ui(),
            }
        } else {
            Span {
                id: index * 2 + 1,
                fixed: true,
            }
        }
    }
}

/// Line-drawing characters fence text in: nothing is reordered across one,
/// and it stays where it is.
fn separates(cell: &BidiCell) -> bool {
    !cell.attached && matches!(cell.lead, '\u{2500}'..='\u{259f}')
}

/// Whether `character` can turn the text around it right to left. This errs
/// on the side of yes: it only decides whether a row is worth analysing.
pub fn is_right_to_left(character: char) -> bool {
    matches!(character,
        '\u{0590}'..='\u{08ff}'
        | '\u{fb1d}'..='\u{fdff}'
        | '\u{fe70}'..='\u{feff}'
        | '\u{10800}'..='\u{10fff}'
        | '\u{1e800}'..='\u{1efff}'
        | '\u{200f}' | '\u{202b}' | '\u{202e}' | '\u{2067}')
}

/// A piece of a row is one paragraph, so a character that would end the
/// paragraph is taken for the blank it is drawn as.
fn within_a_paragraph(character: char) -> char {
    if character.is_control() || character == '\u{2029}' {
        ' '
    } else {
        character
    }
}

/// The character drawn in place of `character` inside right-to-left text,
/// where a bracket that opens to the right opens to the left instead.
pub fn mirrored(character: char) -> Option<char> {
    const PAIRS: [(char, char); 38] = [
        ('(', ')'),
        ('[', ']'),
        ('{', '}'),
        ('<', '>'),
        ('«', '»'),
        ('‹', '›'),
        ('⁅', '⁆'),
        ('⁽', '⁾'),
        ('₍', '₎'),
        ('≤', '≥'),
        ('≦', '≧'),
        ('≪', '≫'),
        ('≮', '≯'),
        ('≰', '≱'),
        ('≺', '≻'),
        ('⊂', '⊃'),
        ('⊆', '⊇'),
        ('⊏', '⊐'),
        ('⊑', '⊒'),
        ('∈', '∋'),
        ('∉', '∌'),
        ('⌈', '⌉'),
        ('⌊', '⌋'),
        ('〈', '〉'),
        ('❨', '❩'),
        ('❪', '❫'),
        ('❬', '❭'),
        ('❮', '❯'),
        ('❰', '❱'),
        ('❲', '❳'),
        ('❴', '❵'),
        ('⟦', '⟧'),
        ('⟨', '⟩'),
        ('⟪', '⟫'),
        ('⟮', '⟯'),
        ('⦃', '⦄'),
        ('⦅', '⦆'),
        ('⸨', '⸩'),
    ];
    PAIRS.iter().find_map(|(opening, closing)| {
        if character == *opening {
            Some(*closing)
        } else if character == *closing {
            Some(*opening)
        } else {
            None
        }
    })
}

/// What ordering needs to know of the row `row` as the screen holds it.
pub fn cells_of(state: &TerminalState, row: usize, out: &mut Vec<BidiCell>) {
    out.clear();
    let screen = state.screen();
    for col in 0..state.size().cols {
        let cell = screen.cell(row, col);
        let tail = screen.occupancy(row, col) == Occupancy::WideTail;
        let lead = match screen.cluster_at(row, col) {
            Some(text) => text.chars().next().unwrap_or(' '),
            None => cell.character(),
        };
        out.push(BidiCell {
            lead,
            attached: tail || cell.is_group_continuation(),
        });
    }
}

fn order_of(state: &TerminalState, mode: BidiMode, row: usize) -> Option<RowOrder> {
    if mode == BidiMode::Off || row >= state.size().rows {
        return None;
    }
    let mut cells = Vec::new();
    cells_of(state, row, &mut cells);
    let mut order = RowOrder::default();
    order
        .arrange(&cells, state.geometry(), row, mode)
        .then_some(order)
}

/// The column of the screen holding what is drawn at the column `visual` of
/// the row `row`: where a click on that spot lands.
pub fn logical_column(state: &TerminalState, mode: BidiMode, row: usize, visual: usize) -> usize {
    match order_of(state, mode, row) {
        Some(order) => order.logical(visual),
        None => visual,
    }
}

/// The column the column `logical` of the row `row` is drawn at: where the
/// cursor shows when the screen has it there.
pub fn visual_column(state: &TerminalState, mode: BidiMode, row: usize, logical: usize) -> usize {
    match order_of(state, mode, row) {
        Some(order) => order.visual(logical),
        None => logical,
    }
}

#[cfg(test)]
mod tests;
