//! Colours and glyphs.
//!
//! Meaning is never carried by colour alone: a flagged row is marked by a glyph
//! *and* a word, so the dashboard stays readable on a monochrome terminal and to
//! a colour-blind reader.

use ratatui::style::Color;

// ------------------------------------------------------------------ the ramp
//
// The brand is two colours: Ink #181818 and Paper #f5f5f0. Every step between
// them is one mixed into the other, so no third hue enters and this dashboard
// stays the same object as the docs site — the ramp below is the one in
// `docs/stylesheets/extra.css`, step for step.
//
// The site quotes its contrast against Paper, because its page is light. These
// are quoted against **Ink**, because a terminal's is dark — and Ink is very
// nearly what a dark terminal's background already is, which is why the two-tone
// scheme costs this dashboard nothing to adopt.
//
// To re-brand, swap these five and the two accents below. Nothing else in the
// TUI names a colour.
const PAPER: Color = Color::Rgb(0xf5, 0xf5, 0xf0); // 16.2:1
const MIST: Color = Color::Rgb(0xd8, 0xd8, 0xd4); // 12.4:1
const ASH: Color = Color::Rgb(0xc1, 0xc1, 0xbd); //  9.8:1
const SMOKE: Color = Color::Rgb(0x8c, 0x8c, 0x8a); //  5.3:1
const SLATE: Color = Color::Rgb(0x6f, 0x6f, 0x6d); //  3.5:1

/// Paper — the most ink there is. The mark, and a focused panel's title.
pub const ACCENT: Color = PAPER;
/// Mist — primary text, 12.4:1. A step below [`ACCENT`] on purpose, so the mark
/// and a focused title still read as brighter than the words around them; with
/// only two colours, hierarchy has to come from lightness and weight.
pub const TEXT: Color = MIST;
/// Smoke — secondary text, 5.3:1, which is AA.
pub const MUTED: Color = SMOKE;
/// Slate — borders, rules and axis marks, 3.5:1.
pub const DIM: Color = SLATE;

// --------------------------------------------------------- the two exceptions
//
// The only hues on an otherwise two-tone dashboard, and both are kept because
// they mark a claim rather than decorate one. Neither is load-bearing: every
// flag they colour is also marked by a glyph *and* a word, per the rule at the
// top of this file, so the dashboard still reads with both switched off.

/// Something the reader should notice: an autonomous agent, an unpriced model, a
/// coverage gap. Always paired with a word, never used on its own. 9.7:1.
pub const WARN: Color = Color::Rgb(0xfa, 0xb2, 0x19);

/// A figure that is real money, as opposed to a token count. 5.2:1.
pub const MONEY: Color = Color::Rgb(0x19, 0x9e, 0x70);

/// The mark, in the three columns a collapsed sidebar can spare: the hexagon's
/// left and right points and the one solid cell inside them — the same two
/// shapes as `docs/assets/logo-mark.svg`, which is a ring with one cell in it.
///
/// Brackets and `\u{25a0}` rather than `\u{2b21}` (⬡), the hexagon itself,
/// because Menlo and SF Mono — the macOS Terminal and iTerm defaults — contain
/// no hexagon codepoint at all, and neither does Courier New. A missing glyph is
/// substituted from some other font at some other width, and one column of drift
/// pushes the sidebar out of line with the body beside it. `\u{25a0}` is in all
/// three stock faces, as is every other glyph this dashboard draws.
pub const MARK: &str = "<\u{25a0}>";

/// Series in fixed slot order, never cycled: a **texture** and the ink to draw
/// it in.
///
/// Two colours cannot encode four identities, and four steps of one ramp cannot
/// either — the argument that used to live here against a ramp of hues applies
/// with more force to a ramp of greys, and in a stacked bar those greys touch
/// each other directly. So identity moved to the glyph. Shade blocks are the one
/// axis a two-tone palette has spare, they read at any size, and all four are in
/// Menlo, SF Mono and Courier New, which the eighth-blocks are not.
///
/// The ink per slot is *not* decoration: it compensates the glyph's own density.
/// `░` inks a quarter of its cell, so drawn in the same grey as `█` it would read
/// a quarter as strongly and slot 3 would look like a rounding error. Solving
/// each pair for equal area-averaged contrast against Ink gives 5.3, 4.2, 5.4 and
/// 4.8:1 — near enough that no series looks more important than another, which is
/// the whole point of a fixed slot order.
///
/// Density also runs one way on purpose: slot 0 stacks at the bottom of a bar
/// (see [`stacked_column`]), so the heaviest texture sits on the baseline and the
/// stack lightens upward instead of floating.
///
/// Four slots, because four is what a real machine needs: Claude Code, Codex,
/// OpenCode and a local runtime is an ordinary install. A fifth series must fold
/// into "other" rather than be given a fifth texture — `▁` and friends are not in
/// Courier New, and inventing one would put a series in a glyph that silently
/// vanishes on someone's terminal.
pub const SERIES: [(&str, Color); 4] = [
    ("\u{2588}", SMOKE), // █ full
    ("\u{2593}", SMOKE), // ▓ three quarters
    ("\u{2592}", ASH),   // ▒ half
    ("\u{2591}", PAPER), // ░ a quarter
];

/// The texture and ink of one series slot, wrapping past the fourth.
pub fn series(slot: usize) -> (&'static str, Color) {
    SERIES[slot % SERIES.len()]
}

// ------------------------------------------------------------------- models
//
// A third deliberate exception to the two-tone rule, alongside WARN and MONEY:
// models get a colour of their own, and it is the *same* colour in the table and
// in the chart above it — every chart with a palette is keyed by model for exactly
// that reason, so a segment and a row are visibly one thing.
//
// Tools are a different axis and keep the SERIES textures. Nothing is ever both,
// so a colour means "model" everywhere and a texture means "tool" everywhere.

/// Model colours: the brand blue ramp, refitted for a dark terminal.
///
/// The ramp as given is ten steps of one hue for a light page — Smart Blue
/// `#0466c8` down through Prussian Blue `#001233` and out to Lavender Grey
/// `#979dac`. Two things had to change, and one earlier objection to it was wrong.
///
/// **The objection that was wrong.** A single-hue ramp was rejected here once as
/// unable to carry identity. That holds for an *unordered* categorical set, and
/// these series are not one: `model_names` is ranked, slot 0 is the largest
/// spender, so a sequential ramp encodes rank as well as identity — which is what
/// a sequential ramp is for. It is also the more robust choice for colour-vision
/// deficiency than a hue-based set, because every dichromacy preserves lightness
/// and none preserves hue. Okabe–Ito, which briefly lived here, buys separation
/// for normal vision at the cost of that property.
///
/// **What had to change: contrast.** Against Ink only four of the ten clear 3:1
/// and the three Prussian Blues land at 1.03–1.21:1, which is invisible. Each
/// step keeps its hue and its saturation and only its lightness is re-solved, for
/// a target spread across 4.5:1–13:1. Six of the ten are used; the rest would
/// not fit that band without crowding their neighbours.
///
/// **What had to change: the order.** The slots below are *not* the ramp's order,
/// and that is the whole trick. Stacked segments touch, and slots stack in series
/// order, so a ramp laid out in its own order puts every bar's least
/// distinguishable pair against each other. Interleaving the light and dark halves
/// alternates a vivid blue with a near-grey at every boundary, so adjacent
/// segments differ in lightness *and* chroma: stack-neighbour separation is ΔE
/// 45.8 where the ramp's own order gave 5.1 in L*. Across the whole legend the
/// closest pair is ΔE 14.5, at the usual categorical floor.
pub const MODELS: [Color; 6] = [
    // Vivid and near-grey alternate. Contrast runs 7.9, 13.0, 6.2, 9.6, 4.5, 11.3.
    Color::Rgb(0x6b, 0xb1, 0xff), // Prussian Blue    #002855
    Color::Rgb(0xda, 0xdd, 0xe2), // Slate Grey       #7d8597
    Color::Rgb(0x5f, 0x97, 0xff), // Prussian Blue 2  #001845
    Color::Rgb(0xb4, 0xbf, 0xd5), // Twilight Indigo  #33415c
    Color::Rgb(0x31, 0x7a, 0xff), // Prussian Blue 3  #001233
    Color::Rgb(0xa8, 0xd3, 0xfd), // Smart Blue       #0466c8
];

/// What `other` is drawn in.
///
/// Smoke, from the two-tone ramp rather than from the model palette: a
/// fold-together bucket should not look like one more model. It clears the nearest
/// series by ΔE 23, and the two accent hues clear their nearest by 82 (amber) and
/// 56 (green), so nothing on a chart can be mistaken for a series that is not one.
pub const MODEL_OTHER: Color = SMOKE;

/// The colour of one model slot; past the sixth, the neutral for `other`.
///
/// Slots are positions in `App::model_names`, which both the chart and the table
/// index into — that shared list is what makes a segment and a row the same
/// colour. There is no wrapping: a seventh model is not given a reused colour, it
/// is folded into `other` upstream.
pub fn model(slot: usize) -> Color {
    MODELS.get(slot).copied().unwrap_or(MODEL_OTHER)
}

/// A model's swatch, in the shape [`crate::ui::chart::stacked`] wants: models are
/// drawn as solid blocks and told apart by colour, where tools are drawn in one
/// ink and told apart by texture.
pub fn model_swatch(slot: usize) -> (&'static str, Color) {
    ("\u{2588}", model(slot))
}

/// Magnitude bars in the tables — the `TOTAL` and share columns.
///
/// Ash, so a solid run of `█` reads as a bar rather than as a line of text. It is
/// deliberately not a series texture: these bars rank rows within one column and
/// have nothing to do with which tool produced them.
pub const SEQUENTIAL: Color = ASH;

/// Which rows of a column belong to which series, bottom-up, so a stacked bar
/// totals exactly the height its value earns.
///
/// Returns one entry per row from the *top*; `None` is empty space.
pub fn stacked_column(values: &[u64], peak: u64, height: usize) -> Vec<Option<usize>> {
    if height == 0 || peak == 0 {
        return vec![None; height];
    }

    // Cumulative boundaries, so rounding never loses or invents a row.
    let mut boundaries = Vec::with_capacity(values.len());
    let mut running = 0u128;
    for value in values {
        running += *value as u128;
        let rows = ((running * height as u128) / peak as u128) as usize;
        boundaries.push(rows.min(height));
    }

    // The height the column has earned, and the one number the fix-up below is
    // not allowed to change. Growing a bar so a small series can be seen is the
    // difference between a chart and a lie: with more series than rows the old
    // code nudged every one of them and walked the stack to the top of the plot,
    // so a column worth a fifth of the peak rendered exactly as tall as the peak.
    // Seven models in a six-row panel drew the Overview as a solid block.
    let cap = *boundaries.last().unwrap_or(&0);

    // Per-series rows, which is the easier shape to redistribute in.
    let mut rows: Vec<usize> = Vec::with_capacity(values.len());
    let mut previous = 0usize;
    for edge in &boundaries {
        rows.push(edge.saturating_sub(previous));
        previous = *edge;
    }

    // A non-zero series claims a row, taken from whichever series has the most
    // rather than added to the top. That keeps both truths: the bar is as tall as
    // its total, and nothing that contributed is invisible. It can only be done
    // while there are rows to go round — a nine-model column three rows tall
    // cannot show nine things, and the legend is what names the rest.
    let starved: Vec<usize> = (0..values.len())
        .filter(|index| values[*index] > 0 && rows[*index] == 0)
        .collect();
    for index in starved {
        let Some(donor) = (0..rows.len())
            .filter(|candidate| rows[*candidate] > 1)
            .max_by_key(|candidate| rows[*candidate])
        else {
            break;
        };
        rows[donor] -= 1;
        rows[index] = 1;
    }

    // Back to cumulative edges for the row walk below.
    let mut running_rows = 0usize;
    for (index, count) in rows.iter().enumerate() {
        running_rows += count;
        boundaries[index] = running_rows;
    }
    debug_assert!(
        boundaries.last().copied().unwrap_or(0) == cap,
        "the fix-up must not change the column's height"
    );

    (0..height)
        .map(|row_from_top| {
            let row_from_bottom = height - 1 - row_from_top;
            boundaries.iter().position(|edge| row_from_bottom < *edge)
        })
        .collect()
}

/// `1089256372` -> `1.1B`. Axis labels have no room for digit groups.
pub fn compact(value: u64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (1_000_000_000_000, "T"),
        (1_000_000_000, "B"),
        (1_000_000, "M"),
        (1_000, "K"),
    ];
    for (scale, suffix) in UNITS {
        if value >= scale {
            let scaled = value as f64 / scale as f64;
            return if scaled >= 100.0 {
                format!("{scaled:.0}{suffix}")
            } else {
                format!("{scaled:.1}{suffix}")
            };
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_totals_the_height_its_value_earns() {
        // Two equal series at full peak fill every row, split evenly.
        let column = stacked_column(&[50, 50], 100, 8);
        assert_eq!(column.iter().filter(|c| c.is_some()).count(), 8);
        assert_eq!(column[0], Some(1), "the later series sits on top");
        assert_eq!(column[7], Some(0), "the first series sits at the bottom");
    }

    #[test]
    fn a_stack_leaves_empty_space_above_a_partial_column() {
        let column = stacked_column(&[25], 100, 8);
        assert_eq!(column.iter().filter(|c| c.is_some()).count(), 2);
        assert_eq!(column[0], None);
        assert_eq!(column[7], Some(0));
    }

    #[test]
    fn a_tiny_contributor_still_claims_a_row() {
        // 1 token beside 999,999 would round to nothing and disappear.
        let column = stacked_column(&[999_999, 1], 1_000_000, 10);
        assert!(
            column.contains(&Some(1)),
            "a non-zero series must be visible: {column:?}"
        );
    }

    #[test]
    fn a_stack_never_exceeds_its_height() {
        for height in [1usize, 3, 12] {
            let column = stacked_column(&[10, 10, 10], 30, height);
            assert_eq!(column.len(), height);
        }
    }

    #[test]
    fn an_empty_stack_is_all_space() {
        assert_eq!(stacked_column(&[0, 0], 100, 3), vec![None, None, None]);
        assert_eq!(stacked_column(&[5], 0, 3), vec![None, None, None]);
    }

    /// Relative luminance, for the two assertions below.
    fn luminance(color: Color) -> f64 {
        let Color::Rgb(r, g, b) = color else {
            panic!("the palette is all explicit RGB: {color:?}");
        };
        let channel = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    /// With two colours, hierarchy *is* the luminance order — there is no hue to
    /// fall back on. An edit that put MUTED above TEXT would not look like a bug
    /// in the diff, so it is pinned here.
    #[test]
    fn the_ramp_runs_brightest_to_dimmest() {
        let steps = [
            ("ACCENT", ACCENT),
            ("TEXT", TEXT),
            ("SEQUENTIAL", SEQUENTIAL),
            ("MUTED", MUTED),
            ("DIM", DIM),
        ];
        for pair in steps.windows(2) {
            let (above, below) = (pair[0], pair[1]);
            assert!(
                luminance(above.1) > luminance(below.1),
                "{} must be brighter than {}",
                above.0,
                below.0
            );
        }
    }

    /// Identity is the texture, so two series sharing one would make two tools
    /// indistinguishable — which no amount of ink can fix.
    #[test]
    fn every_series_has_its_own_texture() {
        for (i, (texture, _)) in SERIES.iter().enumerate() {
            for (j, (other, _)) in SERIES.iter().enumerate() {
                assert!(
                    i == j || texture != other,
                    "slots {i} and {j} share the texture {texture}"
                );
            }
        }
        // And the density runs one way, so a stack lightens upward from its
        // baseline rather than floating: slot 0 is the heaviest ink per cell.
        let coverage = ["\u{2588}", "\u{2593}", "\u{2592}", "\u{2591}"];
        let drawn: Vec<&str> = SERIES.iter().map(|(texture, _)| *texture).collect();
        assert_eq!(drawn, coverage, "textures must run heaviest to lightest");
    }

    /// Contrast against Ink, the background this palette is solved for.
    fn contrast(color: Color) -> f64 {
        let ink = Color::Rgb(0x18, 0x18, 0x18);
        let (a, b) = (luminance(color), luminance(ink));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// The reason the given hexes are not used verbatim. If someone "restores"
    /// them, three models go invisible and this catches it.
    #[test]
    fn every_model_tint_is_readable_on_a_dark_terminal() {
        for (index, tint) in MODELS.iter().enumerate() {
            let ratio = contrast(*tint);
            assert!(
                ratio >= 4.0,
                "model tint {index} is {ratio:.2}:1, under the 4:1 floor"
            );
        }
    }

    /// Ten tints that were three-quarters the same colour would be worse than
    /// none, since the reader would trust a tag that is not saying anything.
    #[test]
    fn model_tints_are_told_apart() {
        let rgb = |color: Color| match color {
            Color::Rgb(r, g, b) => (f64::from(r), f64::from(g), f64::from(b)),
            other => panic!("the palette is all explicit RGB: {other:?}"),
        };
        for (i, a) in MODELS.iter().enumerate() {
            for (j, b) in MODELS.iter().enumerate().skip(i + 1) {
                let (ar, ag, ab) = rgb(*a);
                let (br, bg, bb) = rgb(*b);
                let distance = ((ar - br).powi(2) + (ag - bg).powi(2) + (ab - bb).powi(2)).sqrt();
                assert!(
                    distance >= 25.0,
                    "model tints {i} and {j} are only {distance:.0} apart in RGB"
                );
            }
        }
    }

    /// Every named model slot is distinct, and nothing past them reuses a colour
    /// — a seventh model folds into `other` upstream rather than borrowing the
    /// first model's colour and claiming to be it.
    #[test]
    fn model_slots_never_reuse_a_colour() {
        for slot in 0..MODELS.len() {
            for other in slot + 1..MODELS.len() {
                assert_ne!(
                    model(slot),
                    model(other),
                    "slots {slot} and {other} share a colour"
                );
            }
        }
        assert_eq!(model(MODELS.len()), MODEL_OTHER, "past the last is `other`");
        assert_eq!(model(99), MODEL_OTHER);
        assert!(
            !MODELS.contains(&MODEL_OTHER),
            "`other` must not look like a model"
        );
    }

    /// The interleaving, which is the whole reason the slots are not in the ramp's
    /// own order. Stacked segments touch, and slots stack in series order, so a
    /// ramp laid out in its own order puts every bar's least distinguishable pair
    /// against each other. This is the test that stops someone tidying the array
    /// back into a gradient — which would look neater in the source and be worse
    /// on screen.
    #[test]
    fn adjacent_model_slots_are_far_apart() {
        let rgb = |color: Color| match color {
            Color::Rgb(r, g, b) => (f64::from(r), f64::from(g), f64::from(b)),
            other => panic!("the palette is all explicit RGB: {other:?}"),
        };
        let distance = |a: Color, b: Color| {
            let ((ar, ag, ab), (br, bg, bb)) = (rgb(a), rgb(b));
            ((ar - br).powi(2) + (ag - bg).powi(2) + (ab - bb).powi(2)).sqrt()
        };

        for pair in MODELS.windows(2) {
            let step = distance(pair[0], pair[1]);
            assert!(
                step >= 80.0,
                "slots that stack against each other are only {step:.0} apart"
            );
        }

        // And the array is deliberately not a gradient. Monotone luminance either
        // way means the interleaving has been undone.
        let ordered = |ascending: bool| {
            MODELS.windows(2).all(|w| {
                let (a, b) = (luminance(w[0]), luminance(w[1]));
                if ascending {
                    a <= b
                } else {
                    a >= b
                }
            })
        };
        assert!(
            !ordered(true) && !ordered(false),
            "the slots have been re-sorted into a ramp"
        );
    }

    #[test]
    fn a_series_slot_wraps_past_the_fourth() {
        assert_eq!(series(0), SERIES[0]);
        assert_eq!(series(4), SERIES[0], "a fifth tool reuses slot 0");
        assert_eq!(series(5), SERIES[1]);
    }

    /// A bar may not overstate its own total, whatever the min-one-row nudge
    /// wants. With more series than rows the nudge used to walk every stack to the
    /// top of the plot, so a column carrying a tenth of the peak rendered exactly
    /// as tall as the peak — the Overview drew a solid block of seven models in a
    /// six-row panel and called it a chart.
    #[test]
    fn a_short_column_never_grows_to_fill_the_plot() {
        // Seven series, six rows, and a column worth a fifth of the peak.
        let values = [3u64, 3, 3, 3, 3, 3, 3];
        let peak = 105;
        let height = 6;
        let column = stacked_column(&values, peak, height);

        let filled = column.iter().filter(|cell| cell.is_some()).count();
        let honest = (values.iter().sum::<u64>() as usize * height) / peak as usize;
        assert_eq!(
            filled,
            honest,
            "a column of {}/{peak} should be {honest} of {height} rows, drew {filled}",
            values.iter().sum::<u64>()
        );
        assert!(filled < height, "and must not reach the top of the plot");
    }

    /// A tiny series is still made visible, now by taking a row from the largest
    /// contributor rather than by growing the bar. Both truths survive: the column
    /// keeps its height and nothing that contributed is invisible.
    #[test]
    fn a_tiny_series_takes_a_row_from_the_largest_rather_than_growing_the_bar() {
        // Saturated: the pair fills the column, so there is no room above it.
        let column = stacked_column(&[1_000_000, 1], 1_000_000, 10);
        assert_eq!(
            column.iter().filter(|cell| cell.is_some()).count(),
            10,
            "still a full-height column"
        );
        assert!(
            column.contains(&Some(1)),
            "and the small series is drawn: {column:?}"
        );
        assert_eq!(
            column.iter().filter(|cell| **cell == Some(1)).count(),
            1,
            "with exactly the one row it was given"
        );
    }

    /// When there are more non-zero series than rows, the ones that cannot fit are
    /// dropped rather than the bar being stretched to hold them. The legend names
    /// them; the bar is still the right height.
    #[test]
    fn a_column_too_short_for_its_series_keeps_its_height() {
        let values = [5u64, 5, 5, 5, 5, 5, 5, 5];
        let peak = 200;
        let height = 8;
        let column = stacked_column(&values, peak, height);

        let honest = (values.iter().sum::<u64>() as usize * height) / peak as usize;
        assert_eq!(
            column.iter().filter(|cell| cell.is_some()).count(),
            honest,
            "height is the total's, not the series count's"
        );
    }

    #[test]
    fn compact_numbers_fit_an_axis_label() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_500), "1.5K");
        assert_eq!(compact(1_089_256_372), "1.1B");
        assert_eq!(compact(250_000_000_000), "250B");
    }
}
