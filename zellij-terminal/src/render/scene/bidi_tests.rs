use super::test_helpers::*;
use super::tests::entry_of;
use super::*;
use crate::testing::painter::Painter;
use crate::text::font::{FontOptions, FontStack};

fn cache_laying_out(bidi: BidiMode) -> GlyphCache {
    GlyphCache::new(
        FontStack::build(&FontOptions {
            system_fonts: false,
            bidi,
            ..FontOptions::default()
        })
        .unwrap(),
    )
}

fn shade(index: usize) -> Srgb {
    [40 + index as u8 * 10, 0, 0]
}

fn scene_of(state: &TerminalState, cache: &mut GlyphCache) -> Scene {
    build_at(
        state,
        cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        None,
        None,
    )
}

/// The column each character of `text` is drawn at, the text being written
/// from the first column on.
fn columns_drawn(text: &str, cols: usize, bidi: BidiMode) -> Vec<usize> {
    let state = Painter::state(1, cols, |painter| {
        for (index, character) in text.chars().enumerate() {
            let [red, green, blue] = shade(index);
            painter.styled(0, index, &character.to_string(), |cell| {
                cell.bg = WireColor::Rgb(red, green, blue).pack()
            });
        }
    });
    let mut cache = cache_laying_out(bidi);
    let metrics = cache.metrics();
    let scene = scene_of(&state, &mut cache);
    (0..text.chars().count())
        .map(|index| {
            let rect = scene
                .rects
                .iter()
                .find(|rect| {
                    rect.color == shade(index)
                        && rect.width == metrics.width
                        && rect.height == metrics.height
                })
                .expect("every cell fills its background");
            (rect.x as u32 / metrics.width) as usize
        })
        .collect()
}

#[test]
fn hebrew_is_drawn_from_the_right_and_latin_from_the_left() {
    assert_eq!(
        columns_drawn("abשלום", 8, BidiMode::Ltr),
        vec![0, 1, 5, 4, 3, 2]
    );
}

#[test]
fn nothing_moves_when_bidi_is_off() {
    assert_eq!(
        columns_drawn("abשלום", 8, BidiMode::Off),
        vec![0, 1, 2, 3, 4, 5]
    );
}

#[test]
fn a_line_starting_in_hebrew_is_drawn_from_the_right_edge_when_direction_is_automatic() {
    assert_eq!(
        columns_drawn("של ab", 8, BidiMode::Auto),
        vec![7, 6, 5, 3, 4]
    );
    assert_eq!(
        columns_drawn("ab של", 8, BidiMode::Auto),
        vec![0, 1, 2, 4, 3]
    );
}

#[test]
fn latin_is_drawn_from_the_right_edge_when_direction_is_right_to_left() {
    assert_eq!(columns_drawn("abc", 8, BidiMode::Rtl), vec![5, 6, 7]);
}

fn beam(scene: &Scene, metrics: CellMetrics) -> Rect {
    *scene
        .rects
        .iter()
        .find(|rect| rect.width == metrics.stroke.max(1) && rect.height == metrics.height)
        .expect("a beam cursor is drawn")
}

#[test]
fn the_cursor_is_drawn_on_the_character_it_is_at() {
    let mut cache = cache_laying_out(BidiMode::Ltr);
    let metrics = cache.metrics();
    let stroke = metrics.stroke.max(1);

    let on_the_first_letter = Painter::state(1, 8, |painter| {
        painter.text(0, 0, "שלום");
        painter.cursor(0, 0, CursorShape::Beam);
    });
    let scene = scene_of(&on_the_first_letter, &mut cache);
    assert_eq!(
        beam(&scene, metrics).x,
        (4 * metrics.width - stroke) as i32,
        "the first letter is the rightmost, and the beam stands before it"
    );

    let after_the_word = Painter::state(1, 8, |painter| {
        painter.text(0, 0, "שלום");
        painter.cursor(0, 4, CursorShape::Beam);
    });
    let scene = scene_of(&after_the_word, &mut cache);
    assert_eq!(beam(&scene, metrics).x, (4 * metrics.width) as i32);
}

#[test]
fn a_bracket_inside_hebrew_is_drawn_facing_the_other_way() {
    let mut cache = cache_laying_out(BidiMode::Ltr);
    let metrics = cache.metrics();
    let closing = entry_of(&mut cache, FaceStyle::Regular, ')');
    let opening = entry_of(&mut cache, FaceStyle::Regular, '(');
    assert_ne!(closing, opening);

    let state = Painter::state(1, 6, |painter| painter.text(0, 0, "א(ב"));
    let scene = scene_of(&state, &mut cache);
    let brackets: Vec<&GlyphQuad> = scene
        .glyphs
        .iter()
        .filter(|quad| quad.entry == closing || quad.entry == opening)
        .collect();
    assert_eq!(brackets.len(), 1);
    assert_eq!(brackets[0].entry, closing);
    assert_eq!(brackets[0].x, metrics.width as i32 + closing.left);

    let state = Painter::state(1, 6, |painter| painter.text(0, 0, "a(b"));
    let scene = scene_of(&state, &mut cache);
    assert!(scene.glyphs.iter().any(|quad| quad.entry == opening));
    assert!(!scene.glyphs.iter().any(|quad| quad.entry == closing));
}

#[test]
fn a_cluster_is_still_drawn_after_it_moved() {
    let mut cache = cache_laying_out(BidiMode::Rtl);
    let metrics = cache.metrics();
    let state = Painter::state(1, 4, |painter| {
        painter.cluster(0, 0, "e\u{301}", 1);
        painter.text(0, 1, "x");
    });
    let scene = scene_of(&state, &mut cache);
    let columns: Vec<u32> = scene
        .glyphs
        .iter()
        .map(|quad| quad.x.max(0) as u32 / metrics.width)
        .collect();
    assert!(columns.contains(&2), "{:?}", columns);
    assert!(columns.contains(&3), "{:?}", columns);
    assert!(!columns.contains(&0), "{:?}", columns);
}

#[test]
fn a_row_without_right_to_left_text_is_drawn_the_same_with_bidi_on_or_off() {
    let mut plain = cache_laying_out(BidiMode::Off);
    let mut reordering = cache_laying_out(BidiMode::Ltr);
    let state = Painter::state(1, 8, |painter| painter.text(0, 0, "a -> b"));
    assert_eq!(
        scene_of(&state, &mut plain).glyphs,
        scene_of(&state, &mut reordering).glyphs,
        "a row without right-to-left text is drawn the same either way"
    );
}

#[test]
fn a_hebrew_letter_before_a_space_stays_in_its_own_cell() {
    let state = Painter::state(1, 12, |painter| painter.text(0, 0, "בוקר טוב abc"));
    let mut cache = GlyphCache::new(
        FontStack::build(&FontOptions {
            bidi: BidiMode::Ltr,
            ..FontOptions::default()
        })
        .unwrap(),
    );
    let width = cache.metrics().width as i32;
    let scene = scene_of(&state, &mut cache);
    let mut cells: Vec<i32> = Vec::new();
    for quad in &scene.glyphs {
        let centre = 2 * quad.x + quad.entry.width as i32;
        let cell = centre.div_euclid(2 * width);
        let off_centre = (centre - (2 * cell + 1) * width).abs();
        assert!(
            off_centre <= width / 2,
            "the glyph at {} sits off the centre of its cell: {:?}",
            quad.x,
            quad.entry
        );
        cells.push(cell);
    }
    cells.sort();
    assert!(cells.windows(2).all(|pair| pair[0] != pair[1]), "{cells:?}");
}
