use super::*;
use zellij_utils::structured_render::{PaneRect, PANE_PLUGIN_UI};

fn cells(text: &str) -> Vec<BidiCell> {
    text.chars()
        .map(|lead| BidiCell {
            lead,
            attached: false,
        })
        .collect()
}

fn pane(x: u16, cols: u16) -> PaneRect {
    PaneRect {
        x,
        y: 0,
        cols,
        rows: 4,
        ..PaneRect::default()
    }
}

fn framed(x: u16, cols: u16) -> PaneRect {
    PaneRect {
        top: 1,
        bottom: 1,
        left: 1,
        right: 1,
        ..pane(x, cols)
    }
}

fn geometry(panes: &[PaneRect]) -> GeometryRecord {
    GeometryRecord {
        panes: panes.to_vec(),
    }
}

fn drawn_within(text: &str, geometry: &GeometryRecord, row: usize, mode: BidiMode) -> String {
    let cells = cells(text);
    let mut order = RowOrder::default();
    if !order.arrange(&cells, geometry, row, mode) {
        return text.to_owned();
    }
    order
        .columns()
        .iter()
        .map(|column| cells[*column].lead)
        .collect()
}

fn drawn(text: &str, mode: BidiMode) -> String {
    drawn_within(text, &GeometryRecord::default(), 0, mode)
}

#[test]
fn a_row_without_right_to_left_text_is_drawn_as_it_is_held() {
    let mut order = RowOrder::default();
    let cells = cells("ls -la /home  ");
    for mode in [BidiMode::Off, BidiMode::Ltr, BidiMode::Auto] {
        assert!(
            !order.arrange(&cells, &GeometryRecord::default(), 0, mode),
            "{:?}",
            mode
        );
        assert!(order.columns().is_empty());
        assert_eq!(order.logical(3), 3);
        assert_eq!(order.visual(3), 3);
        assert!(!order.is_rtl(3));
    }
}

#[test]
fn nothing_is_reordered_when_bidi_is_off() {
    assert_eq!(drawn("abc שלום def", BidiMode::Off), "abc שלום def");
}

#[test]
fn a_hebrew_word_is_drawn_right_to_left_where_it_stands() {
    assert_eq!(drawn("abc שלום def", BidiMode::Ltr), "abc םולש def");
}

#[test]
fn hebrew_words_follow_one_another_from_the_right_and_blanks_after_them_stay_put() {
    assert_eq!(drawn("שלום עולם    ", BidiMode::Ltr), "םלוע םולש    ");
}

#[test]
fn a_number_among_hebrew_words_keeps_its_digits_in_order() {
    assert_eq!(drawn("מחיר 100 שקל", BidiMode::Ltr), "לקש 100 ריחמ");
}

#[test]
fn latin_words_in_a_hebrew_sentence_keep_their_letters_in_order() {
    assert_eq!(drawn("git זה כלי טוב  ", BidiMode::Ltr), "git בוט ילכ הז  ");
}

#[test]
fn arabic_is_drawn_right_to_left_too() {
    assert_eq!(drawn("x سلام y", BidiMode::Ltr), "x مالس y");
}

#[test]
fn the_cells_read_right_to_left_are_the_ones_told_apart() {
    let cells = cells("x אב (גד) הו y");
    let mut order = RowOrder::default();
    assert!(order.arrange(&cells, &GeometryRecord::default(), 0, BidiMode::Ltr));
    let leftward: String = (0..cells.len())
        .filter(|column| order.is_rtl(*column))
        .map(|column| cells[column].lead)
        .collect();
    assert_eq!(leftward, "אב (גד) הו");
    assert!(!order.is_rtl(0));
}

#[test]
fn either_order_is_found_from_the_other() {
    let cells = cells("ab שלום cd");
    let mut order = RowOrder::default();
    assert!(order.arrange(&cells, &GeometryRecord::default(), 0, BidiMode::Ltr));
    assert_eq!(order.logical(0), 0);
    assert_eq!(order.logical(3), 6);
    assert_eq!(order.logical(6), 3);
    assert_eq!(order.logical(8), 8);
    for column in 0..cells.len() {
        assert_eq!(order.logical(order.visual(column)), column);
    }
    assert_eq!(order.logical(99), 99);
    assert_eq!(order.visual(99), 99);
}

#[test]
fn text_is_not_reordered_across_a_line_drawing_character() {
    assert_eq!(drawn("│שלום│עולם│", BidiMode::Ltr), "│םולש│םלוע│");
    assert_eq!(
        drawn("│ שלום   ║ עולם   │", BidiMode::Ltr),
        "│ םולש   ║ םלוע   │"
    );
}

#[test]
fn text_is_not_reordered_across_the_edge_of_a_pane() {
    let side_by_side = geometry(&[pane(0, 5), pane(5, 5)]);
    assert_eq!(
        drawn_within("שלום עולם ", &side_by_side, 0, BidiMode::Ltr),
        "םולש םלוע "
    );
    let whole = geometry(&[pane(0, 10)]);
    assert_eq!(
        drawn_within("שלום עולם ", &whole, 0, BidiMode::Ltr),
        "םלוע םולש "
    );
}

#[test]
fn a_floating_pane_cuts_the_row_of_the_pane_beneath_it() {
    let floating = geometry(&[pane(0, 12), pane(4, 4)]);
    assert_eq!(
        drawn_within("אבגדהוזחטיכל", &floating, 0, BidiMode::Ltr),
        "דגבאחזוהלכיט"
    );
}

#[test]
fn a_line_starting_right_to_left_starts_at_the_right_edge_when_direction_is_automatic() {
    assert_eq!(drawn("שלום abc   ", BidiMode::Auto), "   abc םולש");
    assert_eq!(drawn("abc שלום   ", BidiMode::Auto), "abc םולש   ");
}

#[test]
fn automatic_direction_is_taken_within_the_pane_and_within_the_box() {
    let side_by_side = geometry(&[pane(0, 6), pane(6, 6)]);
    assert_eq!(
        drawn_within("של    ab    ", &side_by_side, 0, BidiMode::Auto),
        "    לשab    "
    );
    assert_eq!(drawn("│של  │ab  │", BidiMode::Auto), "│  לש│ab  │");
}

#[test]
fn every_line_starts_at_the_right_edge_when_direction_is_right_to_left() {
    assert_eq!(drawn("abc   ", BidiMode::Rtl), "   abc");
    assert_eq!(drawn("abc שלום  ", BidiMode::Rtl), "  םולש abc");
    let cells = cells("    ");
    let mut order = RowOrder::default();
    assert!(order.arrange(&cells, &GeometryRecord::default(), 0, BidiMode::Rtl));
    assert_eq!(order.visual(0), 3);
}

#[test]
fn frames_and_bars_always_read_left_to_right() {
    let framed = geometry(&[framed(0, 10)]);
    assert_eq!(
        drawn_within(" title    ", &framed, 0, BidiMode::Rtl),
        " title    "
    );
    assert_eq!(
        drawn_within("│abc     │", &framed, 1, BidiMode::Rtl),
        "│     abc│"
    );
    assert_eq!(
        drawn_within(" abc      ", &framed, 1, BidiMode::Rtl),
        "      abc "
    );
    let bar = geometry(&[PaneRect {
        flags: PANE_PLUGIN_UI,
        ..pane(0, 10)
    }]);
    assert_eq!(
        drawn_within("Tab #1    ", &bar, 0, BidiMode::Rtl),
        "Tab #1    "
    );
    assert_eq!(
        drawn_within("Tab שלום  ", &bar, 0, BidiMode::Auto),
        "Tab םולש  "
    );
    assert_eq!(
        drawn_within("שלום Tab  ", &bar, 0, BidiMode::Auto),
        "םולש Tab  "
    );
}

#[test]
fn a_wide_character_moves_as_one_and_keeps_its_halves_in_order() {
    let mut cells = cells("אב漢 ג");
    cells[3].attached = true;
    let mut order = RowOrder::default();
    assert!(order.arrange(&cells, &GeometryRecord::default(), 0, BidiMode::Rtl));
    assert_eq!(order.columns(), &[4, 2, 3, 1, 0]);
}

#[test]
fn cells_grouped_with_the_one_before_them_move_with_it() {
    let mut cells = cells("אxyב");
    cells[1].lead = 'ג';
    cells[2].attached = true;
    let mut order = RowOrder::default();
    assert!(order.arrange(&cells, &GeometryRecord::default(), 0, BidiMode::Ltr));
    assert_eq!(order.columns(), &[3, 1, 2, 0]);
}

#[test]
fn a_control_character_does_not_end_the_paragraph() {
    assert_eq!(drawn("אב\nגד", BidiMode::Ltr), "דג\nבא");
}

#[test]
fn brackets_are_mirrored_and_other_characters_are_not() {
    assert_eq!(mirrored('('), Some(')'));
    assert_eq!(mirrored(')'), Some('('));
    assert_eq!(mirrored('<'), Some('>'));
    assert_eq!(mirrored('»'), Some('«'));
    assert_eq!(mirrored('a'), None);
    assert_eq!(mirrored('א'), None);
    assert_eq!(mirrored('/'), None);
}

#[test]
fn hebrew_and_arabic_are_recognized_and_latin_is_not() {
    for character in ['א', 'ת', 'ש', 'ب', '\u{fb2a}', '\u{200f}'] {
        assert!(is_right_to_left(character), "{:?}", character);
    }
    for character in ['a', '1', ' ', '漢', '│', 'é'] {
        assert!(!is_right_to_left(character), "{:?}", character);
    }
}

#[test]
fn a_spot_of_the_window_and_the_cell_drawn_there_are_found_from_one_another() {
    use crate::testing::painter::Painter;

    let state = Painter::state(2, 12, |painter| {
        painter.text(0, 0, "ab שלום");
        painter.text(1, 0, "א");
        painter.wide(1, 1, '漢');
        painter.text(1, 3, "ב");
    });
    assert_eq!(logical_column(&state, BidiMode::Ltr, 0, 3), 6);
    assert_eq!(logical_column(&state, BidiMode::Ltr, 0, 6), 3);
    assert_eq!(logical_column(&state, BidiMode::Ltr, 0, 1), 1);
    assert_eq!(logical_column(&state, BidiMode::Ltr, 0, 9), 9);
    assert_eq!(visual_column(&state, BidiMode::Ltr, 0, 3), 6);
    assert_eq!(visual_column(&state, BidiMode::Ltr, 0, 7), 7);
    assert_eq!(logical_column(&state, BidiMode::Off, 0, 3), 3);
    assert_eq!(visual_column(&state, BidiMode::Off, 0, 3), 3);
    assert_eq!(logical_column(&state, BidiMode::Ltr, 9, 3), 3);

    let mut cells = Vec::new();
    cells_of(&state, 1, &mut cells);
    assert!(cells[2].attached, "the right half of a wide character");
    assert!(!cells[1].attached);
    assert_eq!(cells[1].lead, '漢');
}
