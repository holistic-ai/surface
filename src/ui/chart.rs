//! Hand-drawn chart primitives.
//!
//! Everything is `Span`s inside a `Paragraph`. ratatui ships `BarChart` and
//! `Sparkline`, but neither stacks by series with a scale gutter and a legend
//! carrying per-series totals, which is what these views need.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{panel, theme};

/// One column of a stacked chart.
#[derive(Debug, Clone, Default)]
pub struct Bucket {
    /// Short axis label.
    pub label: String,
    /// Series name -> value. Values are integers whatever the unit: cost is
    /// carried in micro-dollars so the same stacking maths serves both charts.
    pub series: std::collections::BTreeMap<String, u64>,
    pub total: u64,
}

/// What to draw, beyond the data itself.
pub struct Spec<'a> {
    /// Series names, fixing both the stacking order and the slot each one draws
    /// in. Slot 0 sits at the bottom of a bar.
    pub series: &'a [String],
    /// Turns a raw value into something readable — the axis marks, the per-column
    /// labels and the legend totals all go through it.
    pub format: fn(u64) -> String,
    pub title: &'a str,
    /// Which bucket the reader has picked out, if any.
    pub cursor: Option<usize>,
    /// The glyph and ink of one slot.
    pub swatch: fn(usize) -> (&'static str, Color),
}

/// A stacked bar chart with a scale gutter, thinned date labels and a legend.
///
/// `buckets` are already grouped by whatever granularity the reader chose, and
/// `series` fixes both the stacking order and the texture slots. Values are
/// integers whatever the unit — cost is carried in micro-dollars — and
/// `format` turns them back into something readable for the axis, the labels
/// and the legend.
///
/// `swatch` turns a series slot into the glyph and ink it is drawn in — tools use
/// [`theme::series`], which varies the texture and holds the ink; models use
/// [`theme::model_swatch`], which holds the glyph and varies the colour. The
/// legend takes its swatch from the same function, so it can never disagree with
/// the bars.
///
/// `cursor` indexes `buckets`, and marks one column by weight while the title
/// reports that bucket instead of the window. Out of the drawn window — the plot
/// keeps only as many buckets as fit, newest first — it highlights nothing.
///
/// Draws nothing at all below five rows or a panel narrower than its gutter,
/// which is why callers gate on height before splitting a column in two.
pub fn stacked(frame: &mut Frame, area: Rect, buckets: &[Bucket], spec: Spec) {
    let Spec {
        series,
        format,
        title,
        cursor,
        swatch,
    } = spec;
    let inner_w = area.width.saturating_sub(2) as usize;
    let inner_h = area.height.saturating_sub(2) as usize;
    // Rows: chart body, x labels, legend.
    let plot_h = inner_h.saturating_sub(2);

    // The gutter has to fit the widest axis mark. Fixed at seven it gave the
    // label a six-wide field, and `{:>6}` does not truncate — a seven-character
    // `$819.72` overflowed and shifted that whole row one column right, so the
    // bars stepped in and out. Token labels like `1.1B` always fitted, which is
    // why only the cost chart showed it.
    let peak_all = buckets.iter().map(|b| b.total).max().unwrap_or(1).max(1);
    // Every fraction a tick can land on, not just the three the axis used to
    // draw: a quarter mark is not always narrower than the peak once a currency
    // symbol and a thousands separator are involved.
    let gutter = [
        format(peak_all),
        format(peak_all * 3 / 4),
        format(peak_all / 2),
        format(peak_all / 4),
        "0".to_string(),
    ]
    .iter()
    .map(|mark| mark.chars().count())
    .max()
    .unwrap_or(1)
        + 2;

    if plot_h == 0 || inner_w <= gutter || buckets.is_empty() {
        return;
    }

    let plot_w = inner_w - gutter;
    let tools = series.to_vec();

    // Widen the bars when there is room, rather than hugging the left edge.
    // Six weeks then fill the panel instead of crowding into its first third;
    // the ceiling stops two months becoming two enormous slabs.
    let cell = (plot_w / buckets.len().max(1)).clamp(1, 20);
    // The gap is worth a column only once the bar can keep two. At a two-wide
    // cell the old `cell - 1` spent half the chart on gaps and drew a comb.
    //
    // The trade is deliberate: with no gap, neighbouring buckets of one series
    // touch, so a crowded chart reads as a filled envelope rather than as
    // separate columns. That is the better read of thirty days in eighty columns
    // — the shape is the point at that density, and a reader who wants a single
    // bucket has the cursor. Regrouping by week or month widens the cell past
    // three and the gaps come back.
    let gap = usize::from(cell >= 3);
    let bar_w = (cell - gap).max(1);
    let shown_from = buckets.len().saturating_sub(plot_w / cell);
    let shown = &buckets[shown_from..];

    let peak = shown.iter().map(|b| b.total).max().unwrap_or(1).max(1);

    // The cursor indexes the whole series; `shown` is a right-aligned window over
    // it, so a cursor on a bucket that has scrolled off highlights nothing.
    let picked = cursor.and_then(|index| index.checked_sub(shown_from));

    // A legend is mandatory past one series: identity is never colour alone. So it
    // wraps rather than clipping — a `Paragraph` does not wrap for us, and model
    // names are long enough that a single line named two of seven series and ran
    // the rest off the right edge, leaving five colours in the bars that nothing
    // on screen explained.
    //
    // The rows it needs come out of the plot, for the same reason the value row
    // does: added on top they would push the whole legend past the bottom border
    // and cost us the thing we are trying to protect.
    // Bounded to a third of the panel, though. Seven model ids at their full
    // length need seven rows in a 51-column panel, which left the spend chart two
    // plot rows and made the legend the chart. Past the budget the *names* give
    // way and the totals do not: a cut name is still a hint, and a missing figure
    // is nothing at all.
    let avail = inner_w - gutter;
    let max_rows = (inner_h / 3).max(1);
    let per_row = tools.len().div_ceil(max_rows).max(1);
    let slot_w = (avail / per_row).max(10);

    let mut legend: Vec<Vec<Span>> = vec![vec![Span::raw(" ".repeat(gutter))]];
    let mut used = gutter;
    for (slot, name) in tools.iter().enumerate() {
        let total: u64 = shown
            .iter()
            .map(|b| b.series.get(name).copied().unwrap_or(0))
            .sum();
        // The swatch comes from the same function the bars use, so the legend
        // cannot disagree with them.
        let (texture, ink) = swatch(slot);
        let figure = format(total);
        // Swatch and space, the space before the figure, and the gap after.
        let name_cap = slot_w
            .saturating_sub(2 + 1 + figure.chars().count() + 3)
            .max(6);
        let entry = format!("{} {figure}   ", super::truncate(name, name_cap));
        let width = 2 + entry.chars().count();

        if used + width > inner_w && used > gutter {
            legend.push(vec![Span::raw(" ".repeat(gutter))]);
            used = gutter;
        }
        used += width;

        let row = legend.last_mut().expect("seeded with one row");
        row.push(Span::styled(
            format!("{texture} "),
            Style::default().fg(ink),
        ));
        row.push(Span::styled(entry, Style::default().fg(theme::MUTED)));
    }

    // A value on every column, but only when the columns are wide enough to
    // hold one. At thirty daily buckets they are two characters wide and the
    // labels would overlap into noise; regrouping by week or month widens them
    // and the numbers appear.
    let values: Vec<String> = shown.iter().map(|b| format(b.total)).collect();
    let widest = values.iter().map(|v| v.chars().count()).max().unwrap_or(0);
    // Strictly narrower than the cell, so adjacent labels keep a gap. That row
    // has to come out of the plot: it used to be pushed in on top of a body
    // already sized to fill the panel, which shoved the legend past the bottom
    // border — so the legend vanished at exactly the widths that could afford
    // it, and a chart lost the only thing naming its series.
    let show_values = widest < cell && plot_h > 1;
    // One legend row was already budgeted by `inner_h - 2`; any beyond that comes
    // out of the plot too.
    let extra_legend = legend.len().saturating_sub(1);
    let plot_h = plot_h
        .saturating_sub(usize::from(show_values))
        .saturating_sub(extra_legend);
    if plot_h == 0 {
        return;
    }

    // Per bucket: which series owns each row, top-down.
    let columns: Vec<Vec<Option<usize>>> = shown
        .iter()
        .map(|bucket| {
            let values: Vec<u64> = tools
                .iter()
                .map(|tool| bucket.series.get(tool).copied().unwrap_or(0))
                .collect();
            theme::stacked_column(&values, peak, plot_h)
        })
        .collect();

    let mut lines: Vec<Line> = Vec::with_capacity(plot_h + 3);

    if show_values {
        let mut spans = vec![Span::raw(format!("{:>width$} ", "", width = gutter - 1))];
        for value in &values {
            // Aligned to the bar, not the cell: the loop below pads the gap
            // separately, and right-aligning across it drifted every label one
            // column past its column.
            spans.push(Span::styled(
                format!("{value:>w$}", w = bar_w),
                Style::default().fg(theme::MUTED),
            ));
            if cell > bar_w {
                spans.push(Span::raw(" ".repeat(cell - bar_w)));
            }
        }
        lines.push(Line::from(spans));
    }

    // Scale marks every `step` rows, counted from the bottom so the zero row is
    // always one. Four or five marks on a tall panel, two on a short one — enough
    // that a bar's height can be read off as a number rather than only compared
    // with its neighbours.
    //
    // The old three-branch chain also had a bug worth naming: its arms were
    // ordered, so at `plot_h == 2` row 1 matched the midpoint first and the zero
    // mark was never drawn at all.
    // Tick rows named outright rather than tested with a modulus. A modulus can
    // only be anchored at one end, so forcing the other end in as well put two
    // marks on adjacent rows — a seventeen-row plot drew $171.57 and $160.85 one
    // above the other, which reads as a mistake because it is one. Spacing the
    // set across the plot instead lands on both ends by construction.
    let intervals = (plot_h / 4).clamp(1, 3);
    let ticks: Vec<usize> = (0..=intervals)
        .map(|i| i * (plot_h - 1) / intervals)
        .collect();

    for row in 0..plot_h {
        let from_bottom = plot_h - 1 - row;
        let is_tick = ticks.contains(&row);
        let axis = if is_tick {
            // The row's own share of the peak, so the mark says what the row is
            // rather than what fraction of the way up it sits.
            let value = (peak as u128 * from_bottom as u128 / (plot_h.max(2) - 1) as u128) as u64;
            if from_bottom == 0 {
                "0".to_string()
            } else {
                format(value)
            }
        } else {
            String::new()
        };

        let mut spans = vec![Span::styled(
            format!("{axis:>width$} ", width = gutter - 1),
            Style::default().fg(theme::DIM),
        )];

        // A gridline on the tick rows, so the eye can carry a level across the
        // panel. `·` rather than `─`: a solid rule competes with the bars, and it
        // is in every stock monospace font, which the dashed rules are not.
        //
        // Only where there is no bar. Filling the gap *between* bars would undo
        // the gap — thirty buckets ran together into one ribbon — so the gap stays
        // blank and the gridline reads as sky above the bars, which is where a
        // level is actually useful.
        let empty = if is_tick { "\u{b7}" } else { " " };

        for (index, column) in columns.iter().enumerate() {
            let (glyph, color) = match column[row] {
                Some(slot) => swatch(slot),
                None => (empty, theme::DIM),
            };
            let mut style = Style::default().fg(color);
            if picked == Some(index) {
                // Weight, not a fifth hue: the four series colours are the whole
                // budget, so the cursor cannot be one of them.
                style = style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(glyph.repeat(bar_w), style));
            if cell > bar_w {
                spans.push(Span::raw(" ".repeat(cell - bar_w)));
            }
        }
        lines.push(Line::from(spans));
    }

    // Labels, thinned so they never collide at the chosen bar width.
    // Space labels by how wide they actually are, and give each one the whole
    // gap it earns — truncating to a single cell produced "06-2".
    let label_w = shown
        .iter()
        .map(|b| b.label.chars().count())
        .max()
        .unwrap_or(2);
    let every = ((label_w + 1) as f64 / cell as f64).ceil() as usize;
    let every = every.max(1);

    // One span per label rather than one for the whole line, so the bucket under
    // the cursor can be named in a colour the rest of the axis does not use.
    let mut label_spans = vec![Span::raw(format!("{:>width$} ", "", width = gutter - 1))];
    let mut index = 0usize;
    while index < shown.len() {
        let room = (every * cell).min((shown.len() - index) * cell);
        let text: String = shown[index].label.chars().take(room).collect();
        // A label stands for the run of buckets up to the next one, so it lights
        // up when the cursor is anywhere in that run.
        let holds_cursor = picked.is_some_and(|at| at >= index && at < index + every);
        label_spans.push(Span::styled(
            format!("{text:<room$}"),
            if holds_cursor {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::DIM)
            },
        ));
        index += every;
    }
    lines.push(Line::from(label_spans));

    for row in legend {
        lines.push(Line::from(row));
    }

    let window: u64 = shown.iter().map(|b| b.total).sum();
    // With a bucket under the cursor the title answers for *it* — the window
    // total is still one keystroke away, and a reader who has moved the cursor
    // there is asking about that bucket. Its per-series split goes in too, which
    // is the figure the legend cannot give for a single column.
    let title = match picked.and_then(|at| shown.get(at)) {
        Some(bucket) => {
            let mut split: Vec<String> = tools
                .iter()
                .filter_map(|tool| {
                    let value = bucket.series.get(tool).copied().unwrap_or(0);
                    (value > 0).then(|| format!("{tool} {}", format(value)))
                })
                .collect();
            split.truncate(3);
            format!(
                "{title} \u{b7} {} \u{b7} {} \u{b7} {} \u{b7} \u{5b} \u{5d} bucket \u{b7} [w] regroup",
                bucket.label,
                format(bucket.total),
                if split.is_empty() {
                    "nothing".to_string()
                } else {
                    split.join(" \u{b7} ")
                },
            )
        }
        None => format!(
            "{title} \u{b7} {} buckets \u{b7} {} total \u{b7} peak {} \u{b7} [w] regroup",
            shown.len(),
            format(window),
            format(peak),
        ),
    };
    frame.render_widget(Paragraph::new(lines).block(panel(&title)), area);
}
