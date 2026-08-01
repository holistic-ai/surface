//! Drawing. One module, six views, no widget framework beyond ratatui's
//! `Paragraph`, `Table` and `Block`.

pub mod chart;
pub mod theme;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{App, Tab, Unit};
use crate::format::{human_bytes, thousands};
use crate::ledger::Tokens;
use crate::pricing::{format_usd, Cost};

/// Where the last frame put the things a mouse can hit.
///
/// Returned by [`draw`] rather than recomputed by the event handler, so a click
/// is resolved against the very geometry that was drawn. The alternative — a
/// second copy of the tab widths and the panel insets living in the input code —
/// is the kind of duplication that goes wrong the first time a label changes.
#[derive(Debug, Default, Clone)]
pub struct Hits {
    /// One band per view in the sidebar, in the order they are drawn.
    tabs: Vec<(Rect, Tab)>,
    /// The selectable rows of this view's main table, absent on a view with none.
    rows: Option<Rows>,
    /// The sessions pane beside the projects table, on the one view that has it.
    sessions: Option<Rows>,
}

/// The band of a table that holds data rows.
#[derive(Debug, Clone, Copy)]
struct Rows {
    /// Data rows only: the border and the header are already excluded.
    area: Rect,
    /// The scroll offset ratatui settled on, so row 0 of `area` is this index.
    offset: usize,
    /// Terminal lines each data row occupies — two while the detail line is on.
    lines_per_row: u16,
}

impl Hits {
    /// The view whose entry in the sidebar is under the pointer.
    pub fn tab_at(&self, column: u16, row: u16) -> Option<Tab> {
        self.tabs
            .iter()
            .find(|(band, _)| contains(*band, column, row))
            .map(|(_, tab)| *tab)
    }

    /// The row index under the pointer in this view's main table. May be past the
    /// end of a short table — the caller holds the row count, so it does the
    /// rejecting.
    pub fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        index_at(self.rows, column, row)
    }

    /// The same, for the sessions pane. A separate method rather than a wider
    /// return type on [`Hits::row_at`], because the two panes hold different
    /// kinds of selection and the caller has to know which it moved.
    pub fn session_row_at(&self, column: u16, row: u16) -> Option<usize> {
        index_at(self.sessions, column, row)
    }

    /// Whether the pointer is over the sessions pane at all, for wheel targeting.
    pub fn over_sessions(&self, column: u16, row: u16) -> bool {
        self.sessions
            .is_some_and(|rows| contains(rows.area, column, row))
    }
}

/// Which data row a point falls on, given how many lines each row occupies.
///
/// Dividing by `lines_per_row` is right because ratatui's `TableState::offset`
/// counts *rows*, not terminal lines: `Table::get_row_bounds` clamps it with
/// `offset.min(self.rows.len() - 1)` and walks `self.rows.iter().skip(offset)`.
/// So a click anywhere in a two-line row resolves to that one row.
fn index_at(rows: Option<Rows>, column: u16, row: u16) -> Option<usize> {
    let rows = rows?;
    contains(rows.area, column, row).then(|| {
        let line = (row - rows.area.y) as usize;
        rows.offset + line / rows.lines_per_row.max(1) as usize
    })
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

/// Sidebar width with the view titles spelled out.
const NAV_WIDE: u16 = 18;

/// Collapsed sidebar: the digits and the mark, nothing else. The whole band is
/// still the hit target, so a click works the same as it does when expanded.
///
/// Five, not four: one column goes to the rule, and the mark needs the other
/// four. At four the closing bracket was cut off.
const NAV_COMPACT: u16 = 5;

/// Below this total width the titles cost more than they are worth — an
/// 18-column sidebar on an 80-column terminal is a quarter of the dashboard.
const NAV_COMPACT_BELOW: u16 = 90;

fn nav_width(total: u16) -> u16 {
    if total < NAV_COMPACT_BELOW {
        NAV_COMPACT
    } else {
        NAV_WIDE
    }
}

pub fn draw(frame: &mut Frame, app: &App) -> Hits {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),    // sidebar and body
            Constraint::Length(1), // footer, full width under both
        ])
        .split(frame.area());

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(nav_width(rows[0].width)),
            Constraint::Min(20),
        ])
        .split(rows[0]);

    let tabs = draw_nav(frame, columns[0], app);
    let body = columns[1];

    // Only Projects draws a second clickable pane, so the other five say so
    // rather than every view carrying a shape it does not use.
    let (table, sessions) = match app.tab {
        Tab::Overview => {
            draw_overview(frame, body, app);
            (None, None)
        }
        Tab::Tools => (draw_tools(frame, body, app), None),
        Tab::Sites => (draw_sites(frame, body, app), None),
        Tab::Usage => (draw_usage(frame, body, app), None),
        Tab::Cost => (draw_cost(frame, body, app), None),
        Tab::Projects => draw_projects(frame, body, app),
    };

    draw_footer(frame, rows[1], app);

    if app.show_help {
        draw_help(frame, frame.area());
    }

    Hits {
        tabs,
        rows: table,
        sessions,
    }
}

/// The view list, down the left edge.
///
/// One row per view, each the full width of the sidebar so the current view
/// reads as a band rather than a highlighted word. The mark heads the column,
/// which is where a dashboard's logo belongs.
///
/// Returns the band of each entry, so a click resolves against the rows that
/// were drawn rather than a recomputation of where they ought to be.
fn draw_nav(frame: &mut Frame, area: Rect, app: &App) -> Vec<(Rect, Tab)> {
    // A rule rather than a full box: the sidebar is a column of the page, not a
    // panel sitting in it.
    let frame_block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme::DIM));
    let inner = frame_block.inner(area);
    frame.render_widget(frame_block, area);

    let compact = inner.width < NAV_WIDE.saturating_sub(1);
    let width = inner.width as usize;

    // Mark and name in one paint, like the lockup in docs/assets. Collapsed, the
    // mark stands in for the whole lockup.
    let lockup = if compact {
        format!(" {}", theme::MARK)
    } else {
        format!(" {} surface", theme::MARK)
    };
    let mut lines = vec![
        Line::from(Span::styled(
            lockup,
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let mut hits = Vec::with_capacity(Tab::ALL.len());
    for (i, tab) in Tab::ALL.iter().enumerate() {
        let label = if compact {
            format!(" {} ", i + 1)
        } else {
            format!(" {}\u{a0}{} ", i + 1, tab.title())
        };

        let y = inner.y + lines.len() as u16;
        // A short terminal runs out of sidebar before it runs out of views. The
        // entries that did not fit are not drawn, so they are not clickable
        // either — the digit keys still reach them.
        if y >= inner.y.saturating_add(inner.height) {
            break;
        }
        hits.push((
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
            *tab,
        ));

        lines.push(Line::from(Span::styled(
            // Padded to the full width, so the band spans the sidebar.
            format!("{label:<width$}"),
            if *tab == app.tab {
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(theme::MUTED)
            },
        )));
    }

    // Mock data has to be unmistakable, and the sidebar is on screen in every
    // view and never scrolls away.
    if app.scan.demo {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            if compact {
                " ! ".to_string()
            } else {
                " DEMO ".to_string()
            },
            Style::default()
                .fg(theme::WARN)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )));
    }

    frame.render_widget(Paragraph::new(lines), inner);
    hits
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(status) = &app.status_line {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {status}"),
                Style::default().fg(theme::WARN),
            ))),
            area,
        );
        return;
    }

    let mut parts = vec![format!("scanned in {}ms", app.timings.total_ms)];
    if app.scan.usage.bytes_read > 0 {
        parts.push(format!("read {}", human_bytes(app.scan.usage.bytes_read)));
    }
    if !app.scan.failed.is_empty() {
        parts.push(format!("FAILED: {}", app.scan.failed.join(", ")));
    }
    if app.scan.usage.ledger_write_failed {
        parts.push("ledger not saved".to_string());
    }

    let keys = "[tab] view  [jk] move  [enter] pane  [du] units  [?] help  [q] quit";
    let left = format!(" {} ", parts.join(" \u{b7} "));
    let pad = (area.width as usize).saturating_sub(left.chars().count() + keys.chars().count() + 1);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, Style::default().fg(theme::DIM)),
            Span::raw(" ".repeat(pad)),
            Span::styled(keys, Style::default().fg(theme::DIM)),
        ])),
        area,
    );
}

// ---------------------------------------------------------------- overview

/// Rows the card band takes: a border, the figure, the label, three qualifiers.
const CARDS_H: u16 = 7;

/// Rows a ranking takes: a border, a title-less header of nothing, five entries
/// and a `+N more` line.
const RANKS_H: u16 = 8;

/// The chart's floor. Below five rows `chart::stacked` draws nothing at all, and
/// the legend wants two more.
const CHART_MIN: u16 = 8;

/// Cards, one chart, three rankings — down the page, not across it.
///
/// It used to be cards, then a 60/40 split with two stacked charts on the left and
/// the repository ranking on the right. Three things were wrong with that. The two
/// charts were the same data in two units, so their silhouettes matched and their
/// legend — up to three rows of it — was printed twice. The repository panel took
/// 40% of the width and every row it was given, so a tall terminal listed fifty
/// repositories most of which had spent pennies. And the tool axis had nowhere to
/// be at all, once the charts were keyed by model.
///
/// One chart, full width, in whichever unit the reader asked for; then the three
/// axes the scan actually knows, ranked side by side. Nothing on the page is drawn
/// twice.
fn draw_overview(frame: &mut Frame, area: Rect, app: &App) {
    // Gated on what is left rather than on the frame, so dropping the cards hands
    // their rows to the bands below instead of leaving a hole.
    let show_cards = area.height >= CARDS_H + CHART_MIN;
    let rest = area.height - if show_cards { CARDS_H } else { 0 };
    let show_ranks = rest >= CHART_MIN + RANKS_H;

    let mut bands = Vec::with_capacity(3);
    if show_cards {
        bands.push(Constraint::Length(CARDS_H));
    }
    bands.push(Constraint::Min(CHART_MIN));
    if show_ranks {
        bands.push(Constraint::Length(RANKS_H));
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(bands)
        .split(area);

    let mut next = 0;
    if show_cards {
        draw_cards(frame, rows[next], app);
        next += 1;
    }
    draw_overview_chart(frame, rows[next], app);
    next += 1;
    if show_ranks {
        draw_rankings(frame, rows[next], app);
    }
}

/// The one time chart, in the unit the reader picked.
///
/// Spend by default — it is what the tool is for, and what the cards lead with —
/// and `u` swaps it for tokens. Both are stacked by model with the same colours,
/// so a segment means the same thing here as on Usage and Cost.
fn draw_overview_chart(frame: &mut Frame, area: Rect, app: &App) {
    let spend = app.unit == Unit::Spend;

    // With no price table every bucket is zero, and a chart reading "$0 total"
    // says AI is free — the one arithmetic lie this tool must not tell. The title
    // says how to see something useful instead, since this panel is the whole
    // chart now rather than one of two.
    if spend && app.prices.is_empty() {
        frame.render_widget(
            Paragraph::new(no_prices_lines()).block(panel("estimated spend \u{b7} [u] for tokens")),
            area,
        );
        return;
    }

    let mut title = if spend {
        "estimated spend".to_string()
    } else {
        "tokens".to_string()
    };
    if spend && app.unpriced_models() > 0 {
        title.push_str(" \u{b7} \u{25b2} a floor");
    }
    // Names what the key would give rather than "units", and sits at the end of
    // the caller's half so it reads before the figures the chart appends.
    title.push_str(if spend {
        " \u{b7} [u] tokens"
    } else {
        " \u{b7} [u] spend"
    });

    let buckets = if spend {
        app.model_cost_buckets()
    } else {
        app.model_token_buckets()
    };
    let cursor = bucket_cursor(app, buckets.len());
    chart::stacked(
        frame,
        area,
        &buckets,
        chart::Spec {
            series: app.model_names(),
            format: if spend {
                format_micro_usd
            } else {
                theme::compact
            },
            title: &title,
            cursor,
            swatch: theme::model_swatch,
        },
    );
}

/// The three axes the scan knows, ranked: model, tool, repository.
///
/// Side by side and equally wide, because no one of them is the answer — a model
/// is expensive, a tool ran it, a repository asked for it, and which of those a
/// reader can act on depends entirely on why they opened the dashboard.
fn draw_rankings(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 3); 3])
        .split(area);

    let by_model: Vec<Rank> = app
        .spend_by_model()
        .into_iter()
        .map(|(name, usd, unpriced)| Rank {
            name,
            usd,
            floor: unpriced > 0,
        })
        .collect();
    draw_ranking(frame, columns[0], "by model", &by_model, app);

    // No floor marker: a tool's spend is a floor only if a model under it is
    // unpriced, and that is what the model ranking beside it already says.
    let by_tool: Vec<Rank> = app
        .spend_by_tool()
        .into_iter()
        .map(|(name, usd)| Rank {
            name,
            usd,
            floor: false,
        })
        .collect();
    draw_ranking(frame, columns[1], "by tool", &by_tool, app);

    let by_repo: Vec<Rank> = app
        .repos()
        .iter()
        .map(|repo| Rank {
            name: repo.repo.clone(),
            usd: repo.usd,
            floor: repo.unpriced > 0,
        })
        .collect();
    draw_ranking(frame, columns[2], "by repository", &by_repo, app);
}

fn draw_cards(frame: &mut Frame, area: Rect, app: &App) {
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 5); 5])
        .split(area);

    draw_spend_card(frame, cards[0], app);

    let unpriced = app.unpriced_models();
    // A total with nothing to compare it against is the least useful shape a cost
    // figure can take, so the window line carries a projected rate and the line
    // under it says which way the spend is going. Both are dropped rather than
    // faked when they cannot be computed — a zero-day window, or too few active
    // days for a half-against-half comparison to mean anything.
    let mut spend_detail = vec![match app.daily_rate() {
        Some(rate) => format!(
            "over {} days \u{b7} \u{2248} {}/day",
            app.scan.usage.window_days,
            format_usd(rate)
        ),
        None => format!("over {} days", app.scan.usage.window_days),
    }];
    if let Some(trend) = app.spend_trend() {
        // The arrow carries the sign so the number stays unsigned — `▼ 12%` reads
        // faster than `-12%`. It cannot carry a colour: `card` paints one colour
        // for the whole card, and that one is already saying whether the total is
        // a floor, which matters more than which way the spend moved.
        let arrow = if trend.change >= 0.0 {
            "\u{25b2}"
        } else {
            "\u{25bc}"
        };
        spend_detail.push(format!(
            "{arrow} {:.0}% vs prior {} days",
            trend.change.abs() * 100.0,
            trend.days
        ));
    }
    spend_detail.push(if unpriced > 0 {
        format!("\u{25b2} {unpriced} model(s) unpriced")
    } else {
        "all models priced".to_string()
    });

    card(
        frame,
        cards[1],
        "token cost",
        &format_usd(app.total_usd()),
        &spend_detail,
        if unpriced > 0 {
            theme::WARN
        } else {
            theme::MONEY
        },
    );

    card(
        frame,
        cards[2],
        "tokens",
        &theme::compact(app.total_tokens()),
        &[
            format!("{} messages", thousands(app.total_messages())),
            // Distinct models, not the `(tool, model)` pairs `models()` lists: one
            // model run by two tools was counted twice here.
            format!("{} models", app.distinct_models()),
        ],
        theme::TEXT,
    );

    let s = &app.scan.tools_summary;
    card(
        frame,
        cards[3],
        "tools",
        &s.detected.to_string(),
        &[
            if s.autonomous > 0 {
                format!("\u{25b2} {} autonomous", s.autonomous)
            } else {
                "none autonomous".to_string()
            },
            format!("{} vendors", s.vendors.len()),
        ],
        if s.autonomous > 0 {
            theme::WARN
        } else {
            theme::TEXT
        },
    );

    draw_sites_card(frame, cards[4], app);
}

/// What the seats actually cost, as far as that can be known — against the
/// token arithmetic on the TOKEN COST card beside it.
///
/// Three states, in the house grammar: a configured figure is plain, a
/// detected plan's list price is `≈` an estimate, a tool with usage but no
/// figure makes the total `≥` a floor — and knowing nothing shows `–` and
/// says how to fix it, never a guess.
fn draw_spend_card(frame: &mut Frame, area: Rect, app: &App) {
    let Some(estimate) = app.spend_estimate() else {
        // Card lines have ~24 columns on a 150-column terminal; every string
        // here is written to that width rather than truncated into noise.
        card(
            frame,
            area,
            "spend",
            "\u{2013}",
            &[
                "no seat price known".to_string(),
                "set [cost.subscriptions]".to_string(),
                "see token cost".to_string(),
            ],
            theme::DIM,
        );
        return;
    };

    let value = format!(
        "{}{}{}/mo",
        if estimate.unpriced_tools > 0 {
            "\u{2265}"
        } else {
            ""
        },
        if estimate.estimated() { "\u{2248}" } else { "" },
        format_usd(estimate.monthly_usd)
    );

    let mut detail = vec![match (estimate.configured, estimate.detected) {
        (c, 0) => format!("{c} configured seat(s)"),
        (0, d) => format!("{d} plan(s) detected"),
        (c, d) => format!("{c} configured \u{b7} {d} detected"),
    }];
    // The comparison the Cost view makes per tool, summed: the same tools'
    // window tokens at API rates. Signed like `SubscriptionRow::saving`.
    let saving = estimate.api_equivalent - estimate.monthly_usd;
    detail.push(if saving >= 0.0 {
        format!("{} under API rates", format_usd(saving))
    } else {
        format!("{} over API rates", format_usd(-saving))
    });
    if estimate.unpriced_tools > 0 {
        detail.push(format!(
            "\u{25b2} {} tool(s) unpriced",
            estimate.unpriced_tools
        ));
    }

    card(
        frame,
        area,
        "spend",
        &value,
        &detail,
        if estimate.unpriced_tools > 0 {
            theme::WARN
        } else {
            theme::MONEY
        },
    );
}

#[cfg(feature = "sqlite")]
fn draw_sites_card(frame: &mut Frame, area: Rect, app: &App) {
    let sites = &app.scan.sites;
    let blind = sites.blind_spots.len();
    card(
        frame,
        area,
        "AI sites",
        &sites.sites.len().to_string(),
        &[
            format!("{} visits", thousands(sites.total_visits())),
            if blind > 0 {
                format!("\u{25b2} {blind} browser(s) unreadable")
            } else {
                format!("{} profile(s) read", sites.profiles_scanned)
            },
        ],
        if blind > 0 { theme::WARN } else { theme::TEXT },
    );
}

#[cfg(not(feature = "sqlite"))]
fn draw_sites_card(frame: &mut Frame, area: Rect, _app: &App) {
    card(
        frame,
        area,
        "AI sites",
        "\u{2013}",
        &[
            "not compiled in".to_string(),
            "needs the sqlite feature".to_string(),
        ],
        theme::DIM,
    );
}

/// A headline figure with two qualifier lines under it.
fn card(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    detail: &[String],
    colour: ratatui::style::Color,
) {
    let mut lines = vec![
        Line::from(Span::styled(
            value.to_string(),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            label.to_uppercase(),
            Style::default().fg(theme::MUTED),
        )),
    ];
    for d in detail {
        lines.push(Line::from(Span::styled(
            d.clone(),
            Style::default().fg(theme::DIM),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::DIM)),
        ),
        area,
    );
}

/// One entry of a ranked panel.
pub struct Rank {
    pub name: String,
    pub usd: f64,
    /// An unpriced model sits under this figure, so it is a floor not a total.
    pub floor: bool,
}

/// Entries a ranking names before summing the rest into `+N more`.
///
/// Five, because the panel is a fifth of the page and a long tail is noise: the
/// repository panel used to take every row it was given, so a sixty-row terminal
/// listed fifty repositories of which the last twenty had spent under a dollar.
/// The tail is not dropped, it is added up — the panel still accounts for the
/// whole total.
const RANK_SHOWN: usize = 5;

/// A ranked panel: name, bar, amount, biggest first.
///
/// The column arithmetic is the repository panel's, which this replaced: reserve
/// the widest *visible* amount first and give the bar whatever is left. Deriving
/// the bar width from the area alone let the longest bar push its own amount off
/// the edge.
fn draw_ranking(frame: &mut Frame, area: Rect, title: &str, rows: &[Rank], app: &App) {
    // Every money surface explains itself rather than printing a column of zeros.
    // The repository panel used to be the one that did not, rendering `≥$0` per row
    // when there was no price table at all.
    if app.prices.is_empty() {
        frame.render_widget(Paragraph::new(no_prices_lines()).block(panel(title)), area);
        return;
    }
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "nothing to rank",
                Style::default().fg(theme::DIM),
            )))
            .block(panel(title)),
            area,
        );
        return;
    }

    let width = area.width.saturating_sub(2) as usize;
    // One row of the panel goes to the `+N more` line when there is a tail.
    let room = (area.height.saturating_sub(2) as usize).min(RANK_SHOWN);
    let shown = room.min(rows.len());
    let tail = &rows[shown..];
    let peak = rows.first().map(|r| r.usd).unwrap_or(0.0).max(0.000_001);

    let tail_label = format!("+ {} more", tail.len());
    let amount_w = rows
        .iter()
        .take(shown)
        .map(|r| format_usd(r.usd).chars().count())
        .chain(std::iter::once(
            format_usd(tail.iter().map(|r| r.usd).sum()).chars().count(),
        ))
        .max()
        .unwrap_or(1);
    // A name field proportional to the panel, since these sit three abreast and a
    // fixed 22 would leave nothing for the bar at a third of the body.
    let name_w = width
        .saturating_sub(amount_w + 2)
        .saturating_mul(3)
        .saturating_div(5)
        .clamp(8, 28);
    let bar_max = width.saturating_sub(name_w + 2 + amount_w);

    let mut lines = Vec::new();
    for row in rows.iter().take(shown) {
        let filled = ((row.usd / peak) * bar_max as f64).round() as usize;
        // A figure with an unpriced model under it is a floor, marked so the bar is
        // not read as the whole story. Its own column, so the amounts stay
        // right-aligned whether or not a row carries one.
        let (marker, style) = if row.floor {
            ("\u{2265}", Style::default().fg(theme::WARN))
        } else {
            (" ", Style::default().fg(theme::MUTED))
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<name_w$} ", truncate(&row.name, name_w)),
                Style::default().fg(theme::TEXT),
            ),
            Span::styled("\u{2588}".repeat(filled), Style::default().fg(theme::MONEY)),
            Span::raw(" ".repeat(bar_max.saturating_sub(filled))),
            Span::styled(marker.to_string(), style),
            Span::styled(
                format!("{:>amount_w$}", format_usd(row.usd)),
                Style::default().fg(theme::MUTED),
            ),
        ]));
    }

    if !tail.is_empty() {
        let rest: f64 = tail.iter().map(|r| r.usd).sum();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<name_w$} ", truncate(&tail_label, name_w)),
                Style::default().fg(theme::DIM),
            ),
            Span::raw(" ".repeat(bar_max + 1)),
            Span::styled(
                format!("{:>amount_w$}", format_usd(rest)),
                Style::default().fg(theme::DIM),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).block(panel(title)), area);
}

// ------------------------------------------------------------------- tools

fn draw_tools(frame: &mut Frame, area: Rect, app: &App) -> Option<Rows> {
    let rows: Vec<Row> = app
        .tools()
        .iter()
        .map(|t| {
            let flag = if t.autonomous {
                Span::styled("\u{25b2} autonomous", Style::default().fg(theme::WARN))
            } else {
                Span::styled("\u{2013}", Style::default().fg(theme::DIM))
            };
            // The raw slug the tool wrote (`team_tier_1`), not a prettied
            // name: the same string [cost.subscriptions] docs and the price
            // table speak, so a reader can act on what they see.
            let plan = match &t.plan {
                Some(plan) => Span::styled(plan.clone(), Style::default().fg(theme::MUTED)),
                None => Span::styled("\u{2013}", Style::default().fg(theme::DIM)),
            };
            // The price beside the plan, in the same grammar as everywhere
            // else: plain when configured, `≈` when it is a list-price
            // estimate — which is exactly where a wrong estimate gets seen
            // and corrected with a `[cost.subscriptions]` entry.
            let seat = match t.monthly {
                Some((usd, false)) => {
                    Span::styled(format_usd(usd), Style::default().fg(theme::MONEY))
                }
                Some((usd, true)) => Span::styled(
                    format!("\u{2248}{}", format_usd(usd)),
                    Style::default().fg(theme::MUTED),
                ),
                None => Span::styled("\u{2013}", Style::default().fg(theme::DIM)),
            };
            Row::new(vec![
                Cell::from(t.name),
                Cell::from(t.vendor),
                Cell::from(t.kind),
                Cell::from(Line::from(flag)),
                Cell::from(Line::from(plan)),
                Cell::from(Line::from(seat)),
                Cell::from(truncate(&t.evidence.join(", "), 60)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(22),
            Constraint::Length(14),
            Constraint::Length(18),
            Constraint::Length(14),
            Constraint::Length(22),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header(&[
        "TOOL", "VENDOR", "KIND", "CAN ACT", "PLAN", "$/MO", "FOUND BY",
    ]))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .block(panel(&format!(
        "{} AI tools \u{b7} {} can act on this machine",
        app.scan.tools_summary.detected, app.scan.tools_summary.autonomous
    )));

    let mut table_state = state(app.selected);
    frame.render_stateful_widget(table, area, &mut table_state);
    row_hits(area, &table_state, 1)
}

// ------------------------------------------------------------------- sites

#[cfg(feature = "sqlite")]
fn draw_sites(frame: &mut Frame, area: Rect, app: &App) -> Option<Rows> {
    let sites = &app.scan.sites;

    if sites.disabled {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Browser history scanning is switched off.",
                    Style::default().fg(theme::TEXT),
                )),
                Line::from(Span::styled(
                    "Set `scan_history = true` under [web] to turn it on.",
                    Style::default().fg(theme::DIM),
                )),
            ])
            .block(panel("AI sites")),
            area,
        );
        return None;
    }

    let peak = sites.sites.first().map(|s| s.visits).unwrap_or(1).max(1);
    let rows: Vec<Row> = sites
        .sites
        .iter()
        .map(|s| {
            let bar = ((s.visits as f64 / peak as f64) * 16.0).round() as usize;
            Row::new(vec![
                Cell::from(s.domain.clone()),
                Cell::from(s.vendor),
                Cell::from(
                    s.kind
                        .map(|k| format!("{k:?}").to_lowercase())
                        .unwrap_or_else(|| "\u{2013}".to_string()),
                ),
                Cell::from(Line::from(Span::styled(
                    format!("{:>7} {}", thousands(s.visits), "\u{2588}".repeat(bar)),
                    Style::default().fg(theme::SEQUENTIAL),
                ))),
                Cell::from(
                    s.last_seen
                        .as_deref()
                        .map(|d| d.get(..10).unwrap_or(d).to_string())
                        // Never render a missing timestamp as an epoch date.
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
            ])
        })
        .collect();

    let mut title = format!(
        "{} AI domains \u{b7} {} profile(s) \u{b7} {} day lookback",
        sites.sites.len(),
        sites.profiles_scanned,
        sites.lookback_days
    );
    if !sites.blind_spots.is_empty() {
        let names: Vec<_> = sites.blind_spots.iter().map(|b| b.name).collect();
        title.push_str(&format!(
            " \u{b7} \u{25b2} unreadable: {}",
            names.join(", ")
        ));
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(26),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(26),
            Constraint::Min(12),
        ],
    )
    .header(header(&["DOMAIN", "VENDOR", "KIND", "VISITS", "LAST SEEN"]))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .block(panel(&title));

    let mut table_state = state(app.selected);
    frame.render_stateful_widget(table, area, &mut table_state);
    row_hits(area, &table_state, 1)
}

#[cfg(not(feature = "sqlite"))]
fn draw_sites(frame: &mut Frame, area: Rect, _app: &App) -> Option<Rows> {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "This build cannot read browser history.",
                Style::default().fg(theme::TEXT),
            )),
            Line::from(Span::styled(
                "It was compiled without the `sqlite` feature, which browser",
                Style::default().fg(theme::DIM),
            )),
            Line::from(Span::styled(
                "history and OpenCode's token store both need.",
                Style::default().fg(theme::DIM),
            )),
        ])
        .block(panel("AI sites \u{b7} not compiled in")),
        area,
    );
    // Nothing to select: this build has no sites to list.
    None
}

// ------------------------------------------------------------------- usage

fn draw_usage(frame: &mut Frame, area: Rect, app: &App) -> Option<Rows> {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Min(6)])
        .split(area);

    // Stacked by model, matching the table below it: the same name is the same
    // colour in both, which is the only way a segment and a row read as one
    // thing. Tools are still on the rows, as a texture.
    let buckets = app.model_token_buckets();
    let cursor = bucket_cursor(app, buckets.len());
    chart::stacked(
        frame,
        rows[0],
        &buckets,
        chart::Spec {
            series: app.model_names(),
            format: theme::compact,
            title: "tokens by model",
            cursor,
            swatch: theme::model_swatch,
        },
    );

    let peak = app
        .models()
        .iter()
        .map(|m| m.tokens.total())
        .max()
        .unwrap_or(1)
        .max(1);

    let total_tokens: u64 = app.models().iter().map(|m| m.tokens.total()).sum();

    // `TOTAL` is the one figure both modes keep in the same place, so the eye
    // does not have to relearn the table when the detail line is toggled.
    let total_cell = |t: &Tokens| {
        let bar = ((t.total() as f64 / peak as f64) * 12.0).round() as usize;
        Cell::from(Line::from(Span::styled(
            format!(
                "{:>8} {}",
                theme::compact(t.total()),
                "\u{2588}".repeat(bar)
            ),
            Style::default().fg(theme::SEQUENTIAL),
        )))
    };

    let (table_rows, widths, labels) = if app.detail {
        let rows: Vec<Row> = app
            .models()
            .iter()
            .map(|m| {
                Row::new(vec![
                    detail_cell(
                        model_swatch(app, &m.model),
                        vec![
                            Span::styled(
                                m.model.clone(),
                                Style::default().fg(model_tint(app, &m.model)),
                            ),
                            Span::styled(
                                format!("  {}", m.tool),
                                Style::default().fg(theme::MUTED),
                            ),
                        ],
                        token_detail(&m.tokens),
                    ),
                    share_cell(m.tokens.total(), total_tokens),
                    Cell::from(thousands(m.tokens.messages)),
                    total_cell(&m.tokens),
                ])
                .height(DETAIL_LINES)
            })
            .collect();
        (
            rows,
            vec![
                Constraint::Min(46),
                Constraint::Length(9),
                Constraint::Length(10),
                Constraint::Length(22),
            ],
            vec!["MODEL", "SHARE", "MESSAGES", "TOTAL"],
        )
    } else {
        let rows: Vec<Row> = app
            .models()
            .iter()
            .map(|m| {
                let t = &m.tokens;
                Row::new(vec![
                    Cell::from(m.tool.clone()),
                    Cell::from(Line::from(Span::styled(
                        m.model.clone(),
                        Style::default().fg(model_tint(app, &m.model)),
                    ))),
                    Cell::from(thousands(t.input)),
                    Cell::from(thousands(t.output)),
                    Cell::from(thousands(t.cache_read)),
                    Cell::from(thousands(t.messages)),
                    total_cell(t),
                ])
            })
            .collect();
        (
            rows,
            vec![
                Constraint::Length(14),
                Constraint::Min(20),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(14),
                Constraint::Length(10),
                Constraint::Length(22),
            ],
            vec![
                "TOOL",
                "MODEL",
                "INPUT",
                "OUTPUT",
                "CACHE READ",
                "MESSAGES",
                "TOTAL",
            ],
        )
    };

    let table = Table::new(table_rows, widths)
        .header(header(&labels))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(panel(&format!("{} models", app.models().len())));

    let mut table_state = state(app.selected);
    frame.render_stateful_widget(table, rows[1], &mut table_state);
    row_hits(rows[1], &table_state, row_lines(app))
}

// -------------------------------------------------------------------- cost

fn draw_cost(frame: &mut Frame, area: Rect, app: &App) -> Option<Rows> {
    if app.prices.is_empty() {
        frame.render_widget(Paragraph::new(no_prices_lines()).block(panel("cost")), area);
        return None;
    }

    let subs = app.subscriptions();
    let constraints = if subs.is_empty() {
        vec![Constraint::Percentage(45), Constraint::Min(6)]
    } else {
        vec![
            Constraint::Percentage(35),
            Constraint::Min(6),
            Constraint::Length(subs.len() as u16 + 3),
        ]
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // By model, for the same reason as Usage.
    let buckets = app.model_cost_buckets();
    let cursor = bucket_cursor(app, buckets.len());
    chart::stacked(
        frame,
        rows[0],
        &buckets,
        chart::Spec {
            series: app.model_names(),
            format: format_micro_usd,
            title: "spend by model",
            cursor,
            swatch: theme::model_swatch,
        },
    );

    let peak = app
        .models()
        .iter()
        .map(|m| m.cost.usd())
        .fold(0.0f64, f64::max)
        .max(0.000_001);

    // The three cost states are visually distinct in both modes: "unpriced" must
    // never read as "$0.00".
    let amount_cell = |cost: &Cost| {
        let (amount, style) = match cost {
            Cost::Known(usd) => (format_usd(*usd), Style::default().fg(theme::MONEY)),
            Cost::Local => ("local".to_string(), Style::default().fg(theme::DIM)),
            Cost::Unpriced => (
                "\u{25b2} unpriced".to_string(),
                Style::default().fg(theme::WARN),
            ),
        };
        Cell::from(Line::from(Span::styled(amount, style)))
    };
    let bar_cell = |cost: &Cost| {
        let bar = ((cost.usd() / peak) * 12.0).round() as usize;
        Cell::from(Line::from(Span::styled(
            "\u{2588}".repeat(bar),
            Style::default().fg(theme::MONEY),
        )))
    };

    let spend: f64 = app.models().iter().map(|m| m.cost.usd()).sum();

    let (table_rows, widths, labels) = if app.detail {
        let rows: Vec<Row> = app
            .models()
            .iter()
            .map(|m| {
                Row::new(vec![
                    detail_cell(
                        model_swatch(app, &m.model),
                        vec![
                            Span::styled(
                                m.model.clone(),
                                Style::default().fg(model_tint(app, &m.model)),
                            ),
                            Span::styled(
                                format!("  {}", m.tool),
                                Style::default().fg(theme::MUTED),
                            ),
                        ],
                        token_detail(&m.tokens),
                    ),
                    // Share of spend, not of tokens: this is the money view.
                    share_cell(
                        (m.cost.usd() * 1_000_000.0) as u64,
                        (spend * 1_000_000.0) as u64,
                    ),
                    Cell::from(theme::compact(m.tokens.total())),
                    amount_cell(&m.cost),
                    bar_cell(&m.cost),
                ])
                .height(DETAIL_LINES)
            })
            .collect();
        (
            rows,
            vec![
                Constraint::Min(46),
                Constraint::Length(9),
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(14),
            ],
            vec!["MODEL", "SHARE", "TOKENS", "COST", ""],
        )
    } else {
        let rows: Vec<Row> = app
            .models()
            .iter()
            .map(|m| {
                Row::new(vec![
                    Cell::from(m.tool.clone()),
                    Cell::from(Line::from(Span::styled(
                        m.model.clone(),
                        Style::default().fg(model_tint(app, &m.model)),
                    ))),
                    Cell::from(theme::compact(m.tokens.total())),
                    amount_cell(&m.cost),
                    bar_cell(&m.cost),
                ])
            })
            .collect();
        (
            rows,
            vec![
                Constraint::Length(14),
                Constraint::Min(20),
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(14),
            ],
            vec!["TOOL", "MODEL", "TOKENS", "COST", ""],
        )
    };

    let unpriced = app.unpriced_models();
    let mut title = format!(
        "{} over {} days",
        format_usd(app.total_usd()),
        app.scan.usage.window_days
    );
    if unpriced > 0 {
        title.push_str(&format!(
            " \u{b7} \u{25b2} a floor: {unpriced} model(s) have no price"
        ));
    }
    if app.prices.is_builtin() {
        // The reader should know the rates are as old as the release.
        title.push_str(" \u{b7} built-in price table");
    }

    let table = Table::new(table_rows, widths)
        .header(header(&labels))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(panel(&title));

    let mut table_state = state(app.selected);
    frame.render_stateful_widget(table, rows[1], &mut table_state);

    if !subs.is_empty() {
        draw_subscriptions(frame, rows[2], &subs);
    }

    row_hits(rows[1], &table_state, row_lines(app))
}

fn draw_subscriptions(frame: &mut Frame, area: Rect, subs: &[crate::app::SubscriptionRow]) {
    let rows: Vec<Row> = subs
        .iter()
        .map(|s| {
            let saving = s.saving();
            let verdict = if saving >= 0.0 {
                Span::styled(
                    format!("subscription saves {}", format_usd(saving)),
                    Style::default().fg(theme::MONEY),
                )
            } else {
                Span::styled(
                    format!("\u{25b2} API would be {} cheaper", format_usd(-saving)),
                    Style::default().fg(theme::WARN),
                )
            };
            Row::new(vec![
                Cell::from(s.tool.clone()),
                Cell::from(format!(
                    "{}{}",
                    format_usd(s.monthly),
                    if s.estimated { " est" } else { "" }
                )),
                Cell::from(format_usd(s.api_equivalent)),
                Cell::from(Line::from(verdict)),
            ])
        })
        .collect();

    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(16),
                Constraint::Length(14),
                Constraint::Length(16),
                Constraint::Min(20),
            ],
        )
        .header(header(&["TOOL", "SUBSCRIPTION", "SAME AT API RATES", ""]))
        .block(panel("subscription vs pay-per-token")),
        area,
    );
}

// ---------------------------------------------------------------- projects

/// Name column. A repository slug is `owner/name`, which this fits without
/// truncating for all but the longest.
const PROJECT_W: usize = 28;

fn draw_projects(frame: &mut Frame, area: Rect, app: &App) -> (Option<Rows>, Option<Rows>) {
    let repos = app.repos();
    if repos.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "No repository-attributed usage in the window.",
                    Style::default().fg(theme::TEXT),
                )),
                Line::from(Span::styled(
                    "Usage is attributed by the working directory a session ran",
                    Style::default().fg(theme::DIM),
                )),
                Line::from(Span::styled(
                    "in, resolved to its git `origin` slug.",
                    Style::default().fg(theme::DIM),
                )),
            ])
            .block(panel("projects")),
            area,
        );
        return (None, None);
    }

    // Sessions of the selected project, beside the table that selected it.
    let sessions: Vec<&crate::app::SessionRow> = match app.selected_project() {
        Some(selected) => app.sessions_in(&selected.repo),
        None => Vec::new(),
    };
    let beside = !sessions.is_empty() && area.width >= SIDE_BY_SIDE_MIN;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(38), Constraint::Min(6)])
        .split(area);

    // The chart keeps the full width above the split. Its panel title is the only
    // place the selected repository is named, and a narrow panel clips a title.
    if let Some(selected) = app.selected_project() {
        let series = [selected.repo.clone()];
        let buckets = app.project_buckets(&selected.repo);
        let cursor = bucket_cursor(app, buckets.len());
        chart::stacked(
            frame,
            rows[0],
            &buckets,
            chart::Spec {
                series: &series,
                format: theme::compact,
                title: &truncate(&selected.repo, 40),
                cursor,
                swatch: theme::series,
            },
        );
    }

    // Side by side, both tables give up columns: 88 columns of project table and
    // 96 of session table cannot share 100. The projects table keeps the three
    // figures a ranking is read for — name, tokens, money — and sheds the rest,
    // which the breakdown beside it carries anyway.
    let (table_area, session_area) = if beside {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(rows[1]);
        (split[0], Some(split[1]))
    } else {
        (rows[1], None)
    };

    let peak = repos.iter().map(|r| r.tokens).max().unwrap_or(1).max(1);

    let table_rows: Vec<Row> = repos
        .iter()
        .map(|r| {
            let bar = ((r.tokens as f64 / peak as f64) * 12.0).round() as usize;
            // A repo with unpriced models under it has a floor, not a total.
            let (amount, style) = if r.unpriced > 0 {
                (
                    format!("\u{2265}{}", format_usd(r.usd)),
                    Style::default().fg(theme::WARN),
                )
            } else {
                (format_usd(r.usd), Style::default().fg(theme::MONEY))
            };
            let name = Cell::from(truncate(&r.repo, PROJECT_W));
            let tokens = Cell::from(theme::compact(r.tokens));
            let money = Cell::from(Line::from(Span::styled(amount, style)));
            let share = Cell::from(Line::from(Span::styled(
                "\u{2588}".repeat(bar),
                Style::default().fg(theme::SEQUENTIAL),
            )));
            if beside {
                Row::new(vec![name, tokens, money, share])
            } else {
                Row::new(vec![
                    name,
                    tokens,
                    Cell::from(thousands(r.messages)),
                    money,
                    Cell::from(Line::from(Span::styled(
                        r.last_day.clone(),
                        Style::default().fg(theme::MUTED),
                    ))),
                    Cell::from(Line::from(Span::styled(
                        "\u{2588}".repeat(bar),
                        Style::default().fg(theme::SEQUENTIAL),
                    ))),
                ])
            }
        })
        .collect();

    let mut title = format!(
        "{} project{} over {} days",
        repos.len(),
        if repos.len() == 1 { "" } else { "s" },
        app.scan.usage.window_days
    );
    let floors = repos.iter().filter(|r| r.unpriced > 0).count();
    if floors > 0 {
        // Shortened when the panel is half the body: a title clipped mid-sentence
        // is worse than a terse one, and the \u{2265} on the row says it too.
        title.push_str(&if beside {
            format!(" \u{b7} \u{25b2} {floors} \u{2265} a floor")
        } else {
            format!(" \u{b7} \u{25b2} {floors} carry unpriced models, so \u{2265} is a floor")
        });
    }

    // The share bar takes the slack, not the name column: a fixed name column
    // keeps the figures in the same place on a wide terminal as on a narrow one.
    let (widths, labels) = if beside {
        // Same doctrine as the wide table: the name column is fixed and the share
        // bar takes the slack, so the figures do not wander as the split moves.
        (
            vec![
                Constraint::Length(PROJECT_W as u16 + 2),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Min(4),
            ],
            vec!["PROJECT", "TOKENS", "COST", ""],
        )
    } else {
        (
            vec![
                Constraint::Length(PROJECT_W as u16 + 2),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Min(14),
            ],
            vec!["PROJECT", "TOKENS", "MESSAGES", "COST", "LAST USED", ""],
        )
    };

    let focused = app.focus() == crate::app::Pane::Primary;
    let table = Table::new(table_rows, widths)
        .header(header(&labels))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(focus_panel(&title, beside && focused));

    let mut table_state = state(app.selected);
    frame.render_stateful_widget(table, table_area, &mut table_state);

    let session_hits =
        session_area.and_then(|area| draw_sessions(frame, area, &sessions, app, !focused));

    (row_hits(table_area, &table_state, 1), session_hits)
}

/// How many sessions the breakdown lists. A busy repository has hundreds, and
/// Body width at which the breakdown moves beside the projects table.
///
/// Below it the two tables cannot both give a useful account of themselves —
/// the projects table alone wants 88 columns of figures — so the breakdown is
/// not drawn at all and the ranking gets the whole width. Nothing is lost that
/// the Usage view does not also show.
const SIDE_BY_SIDE_MIN: u16 = 96;

/// Name column of the breakdown. Narrower than the projects table's, because it
/// shares the body with it and a session's title is prose rather than a slug.
const SESSION_W: usize = 26;

/// Sessions of the selected project: what ran, on which models, and what it cost.
///
/// Scrollable and selectable in its own right, which is why it takes the whole
/// list rather than a capped slice. Its second line is what makes that fit: the
/// tool, the full model list and the token split have no room as columns beside
/// a projects table.
fn draw_sessions(
    frame: &mut Frame,
    area: Rect,
    sessions: &[&crate::app::SessionRow],
    app: &App,
    focused: bool,
) -> Option<Rows> {
    let lines = row_lines(app);
    // Truncated here rather than left to the cell, so a model list that does not
    // fit ends in an ellipsis instead of mid-word. Two borders, the cost column
    // and its spacing come off the width; the two-space indent comes off too.
    let detail_w = (area.width as usize).saturating_sub(2 + 11 + 1 + 2);

    let rows: Vec<Row> = sessions
        .iter()
        .map(|s| {
            // Same floor rule as every other money column: a session with an
            // unpriced model under it has a floor, not a total.
            let (amount, style) = if s.unpriced > 0 {
                (
                    format!("\u{2265}{}", format_usd(s.usd)),
                    Style::default().fg(theme::WARN),
                )
            } else {
                (format_usd(s.usd), Style::default().fg(theme::MONEY))
            };
            let money = Cell::from(Line::from(Span::styled(amount, style)));

            if app.detail {
                Row::new(vec![
                    detail_cell(
                        series_dot(app.series(), &s.tool),
                        vec![Span::raw(truncate(&s.label(), SESSION_W))],
                        // Most identifying first, so what survives a narrow pane
                        // is what tells two sessions apart. The model list goes
                        // last because the Usage view lists every model anyway.
                        truncate(
                            &format!(
                                "{} \u{b7} {} \u{b7} {} \u{b7} {} msg \u{b7} {}",
                                s.tool,
                                s.last_day,
                                theme::compact(s.tokens.total()),
                                thousands(s.tokens.messages),
                                s.models.join(", ")
                            ),
                            detail_w,
                        ),
                    ),
                    money,
                ])
                .height(DETAIL_LINES)
            } else {
                Row::new(vec![
                    Cell::from(truncate(&s.label(), SESSION_W)),
                    Cell::from(theme::compact(s.tokens.total())),
                    money,
                ])
            }
        })
        .collect();

    let title = format!(
        "{} session{} in this project",
        sessions.len(),
        if sessions.len() == 1 { "" } else { "s" }
    );

    let (widths, labels) = if app.detail {
        (
            vec![Constraint::Min(24), Constraint::Length(11)],
            vec!["SESSION", "COST"],
        )
    } else {
        (
            vec![
                Constraint::Min(20),
                Constraint::Length(9),
                Constraint::Length(11),
            ],
            vec!["SESSION", "TOKENS", "COST"],
        )
    };

    let table = Table::new(rows, widths)
        .header(header(&labels))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(focus_panel(&title, focused));

    let mut table_state = state(app.session_selected().min(sessions.len().saturating_sub(1)));
    frame.render_stateful_widget(table, area, &mut table_state);
    row_hits(area, &table_state, lines)
}

/// Why there is nothing to cost. Shown wherever money would otherwise be
/// rendered as zero.
fn no_prices_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "No model prices are available, so nothing can be costed.",
            Style::default().fg(theme::TEXT),
        )),
        Line::from(Span::styled(
            "Run without --offline once to fetch the table.",
            Style::default().fg(theme::DIM),
        )),
    ]
}

/// Micro-dollars back to a readable amount, for the cost chart's axis.
fn format_micro_usd(micro: u64) -> String {
    format_usd(micro as f64 / 1_000_000.0)
}

// ------------------------------------------------------------------ shared

fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::DIM))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(theme::MUTED),
        ))
}

/// A panel that says whether its pane has the keyboard.
///
/// The focused pane brightens its border to `MUTED` and bolds its title. It is
/// the only affordance for a view with two tables — without it, `j`/`k` moving
/// "the wrong" list is inexplicable rather than merely surprising. Weight, not
/// hue, in keeping with the rest of this theme.
fn focus_panel(title: &str, focused: bool) -> Block<'static> {
    if !focused {
        return panel(title);
    }
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::MUTED))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ))
}

fn header(labels: &[&str]) -> Row<'static> {
    Row::new(
        labels
            .iter()
            .map(|l| Cell::from(l.to_string()))
            .collect::<Vec<_>>(),
    )
    .style(
        Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD),
    )
    .bottom_margin(1)
}

/// ratatui owns the scroll offset, so it cannot drift out of step with the
/// selection.
fn state(selected: usize) -> TableState {
    TableState::default().with_selected(Some(selected))
}

// ------------------------------------------------------- the two-line style
//
// Three tables carry more figures than a terminal has columns for, so a row is
// a name line with an indented detail line under it. The detail line is not a
// column — a `Cell` is clipped to its column's width, and no figure column is
// wide enough to hold `In: … · Out: … · CR: … · CW: …` — so it lives as the
// second line of the *name* cell, which is the one column with slack.

/// Terminal lines a row takes in each mode.
const DETAIL_LINES: u16 = 2;
const FLAT_LINES: u16 = 1;

/// How tall the rows of a detail-capable table are this frame.
///
/// The renderer owns this and hands the answer to [`row_hits`]. The input path
/// must never re-derive it from `app.detail`: a pane too narrow for the detail
/// line draws flat rows whatever the setting says, and a hit test that trusted
/// the setting would be out by a factor of two.
/// The chart cursor as an index into `buckets`, from the app's "back from the
/// newest" form. Out of range — a cursor left over from a longer series — simply
/// yields nothing, and the chart draws no highlight.
fn bucket_cursor(app: &App, count: usize) -> Option<usize> {
    count.checked_sub(1 + app.bucket_back()?)
}

fn row_lines(app: &App) -> u16 {
    if app.detail {
        DETAIL_LINES
    } else {
        FLAT_LINES
    }
}

/// The swatch beside a model row: the very glyph and colour the chart's legend
/// gives that model.
///
/// It is the *model's*, not the tool's, because on these two views the chart is
/// stacked by model — a tool texture here would point at a chart that is not on
/// screen. The tool is still named in the row, and still carries its texture in
/// the sessions pane, where the tool is what the row is about.
fn model_swatch(app: &App, model: &str) -> Span<'static> {
    Span::styled(
        format!("{} ", theme::model_swatch(0).0),
        Style::default().fg(model_tint(app, model)),
    )
}

/// The tint a model's name is written in.
///
/// By position in the app's stable model list, so the same model reads the same
/// in the Usage table and the Cost table. A model not in the list — which the
/// builder cannot produce, but a future caller might — falls back to plain text
/// rather than to an arbitrary tint.
fn model_tint(app: &App, model: &str) -> Color {
    let charted = app.fold_model(model);
    app.model_names()
        .iter()
        .position(|known| *known == charted)
        .map_or(theme::MODEL_OTHER, theme::model)
}

/// The swatch that ties a row to its series in the chart above it.
///
/// By *tool*, not by model, and the same texture the bars are drawn in — so the
/// signal that links a table row to a chart segment is one signal, not a colour
/// here and a pattern there.
///
/// A tool with no slot gets `·`: past the fourth series there is no honest
/// texture to give it — see the `SERIES` doc comment — and a repeated one would
/// claim two tools were the same.
fn series_dot(series: &[String], tool: &str) -> Span<'static> {
    match series
        .iter()
        .position(|known| known == tool)
        .filter(|slot| *slot < theme::SERIES.len())
    {
        Some(slot) => {
            let (texture, ink) = theme::series(slot);
            Span::styled(format!("{texture} "), Style::default().fg(ink))
        }
        None => Span::styled("\u{b7} ", Style::default().fg(theme::MUTED)),
    }
}

/// The token split, for the detail line under a row's name.
///
/// Cache creation and reasoning are here because they are billed —
/// `Prices::cost` charges reasoning at the output rate — and were previously
/// summed into `TOTAL` with no column of their own, so the visible figures did
/// not add up to the total beside them.
fn token_detail(tokens: &Tokens) -> String {
    let mut parts = vec![
        format!("In: {}", theme::compact(tokens.input)),
        format!("Out: {}", theme::compact(tokens.output)),
        format!("CR: {}", theme::compact(tokens.cache_read)),
        format!("CW: {}", theme::compact(tokens.cache_creation)),
    ];
    // Only models that report reasoning earn the slot; most never do, and a
    // permanent `R: 0` would cost width every row to say nothing.
    if tokens.reasoning > 0 {
        parts.push(format!("R: {}", theme::compact(tokens.reasoning)));
    }
    parts.join(" \u{b7} ")
}

/// A name cell: the dot and the name, with the detail line indented beneath.
fn detail_cell<'a>(dot: Span<'a>, name: Vec<Span<'a>>, detail: String) -> Cell<'a> {
    let mut head = vec![dot];
    head.extend(name);
    Cell::from(vec![
        Line::from(head),
        Line::from(Span::styled(
            format!("  {detail}"),
            Style::default().fg(theme::MUTED),
        )),
    ])
}

/// `(34.1%)` — a row's share of the column it is part of, dim beside the name.
fn share_cell(part: u64, whole: u64) -> Cell<'static> {
    let share = if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    };
    Cell::from(Line::from(Span::styled(
        format!("({share:.1}%)"),
        Style::default().fg(theme::MUTED),
    )))
}

/// Rows begin three lines into a panelled table: the top border from [`panel`],
/// the header, and the blank line [`header`] leaves under it. Change either of
/// those and this changes with them.
const ROWS_TOP: u16 = 3;

/// The clickable band of a table just rendered into `area`.
///
/// Takes the [`TableState`] *after* the render, because that is when ratatui has
/// written back the offset it scrolled to — which is the only thing that says
/// which row is drawn at the top of the band. `lines` is how tall the rows
/// actually were, which must come from the renderer rather than from the detail
/// setting: a narrow pane draws one-line rows whatever the setting says, and a
/// hit test that trusted the setting would be out by a factor of two.
fn row_hits(area: Rect, state: &TableState, lines: u16) -> Option<Rows> {
    let lines = lines.max(1);
    // One column of border either side, one row of border below.
    let top = area.y.saturating_add(ROWS_TOP);
    let bottom = area.y.saturating_add(area.height).saturating_sub(1);
    if area.width < 3 || bottom <= top {
        return None;
    }

    // Trimmed to whole rows. ratatui admits only rows that fit entirely, so with
    // two-line rows an odd band leaves its last line unpainted — and dividing by
    // `lines` would still map that line to a real row index, selecting something
    // the reader cannot see. Excluding it from the band makes `contains` reject
    // the click instead.
    let height = ((bottom - top) / lines) * lines;
    if height == 0 {
        return None;
    }

    Some(Rows {
        area: Rect {
            x: area.x.saturating_add(1),
            y: top,
            width: area.width.saturating_sub(2),
            height,
        },
        offset: state.offset(),
        lines_per_row: lines,
    })
}

/// Character-safe truncation with an ellipsis.
pub fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    text.chars().take(keep).collect::<String>() + "\u{2026}"
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "surface",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  tab / \u{2192} / l    next view"),
        Line::from("  shift-tab / \u{2190}  previous view"),
        Line::from("  1 \u{2013} 6           jump to a view"),
        Line::from("  j / k / \u{2191}\u{2193}      move the selection"),
        Line::from("  g / G           first / last row"),
        Line::from("  enter           switch pane, where a view has two"),
        Line::from("  d               token detail under each row"),
        Line::from("  u               Overview chart: spend or tokens"),
        Line::from("  [ / \u{5d}           move the chart cursor a bucket"),
        Line::from("  backspace       drop the chart cursor"),
        Line::from("  click           a view, a table row, or a pane"),
        Line::from("  wheel           move the selection under the pointer"),
        Line::from("  w               regroup charts by day, week or month"),
        Line::from("  ?               close this help"),
        Line::from("  q / esc         quit"),
        Line::from(""),
        Line::from(Span::styled(
            "  Everything stays on this machine. The only network",
            Style::default().fg(theme::DIM),
        )),
        Line::from(Span::styled(
            "  call is the model price table; --offline skips it.",
            Style::default().fg(theme::DIM),
        )),
    ];

    let w = 58u16.min(area.width.saturating_sub(4));
    let h = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(panel("help")),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::CostConfig;
    use crate::ledger::{Ledger, Tokens};
    use crate::scan::{tooling, Scan, Timings};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn tokens(input: u64, output: u64) -> Tokens {
        Tokens {
            input,
            output,
            cache_read: input * 10,
            cache_creation: 100,
            reasoning: 0,
            messages: 3,
        }
    }

    /// A scan with something in every section, so no view renders an empty case
    /// by accident. No price table, so every model is unpriced.
    /// A one-model price table, so money figures are real rather than `$0`.
    fn prices_for_a_model() -> crate::pricing::Prices {
        crate::pricing::Prices::parse(
            r#"{"claude-opus-5": {"input_cost_per_token": 0.000005,
                                  "output_cost_per_token": 0.000025,
                                  "cache_read_input_token_cost": 0.0000005,
                                  "cache_creation_input_token_cost": 0.00000625}}"#,
        )
        .expect("a one-model table parses")
    }

    fn populated() -> App {
        populated_with(crate::pricing::Prices::default())
    }

    /// The same scan, costed with the given table. Prices are applied when the
    /// `App` is built, so they cannot be assigned afterwards.
    fn populated_with(prices: crate::pricing::Prices) -> App {
        let mut ledger = Ledger {
            titles_enabled: true,
            ..Default::default()
        };
        for (day, model) in [
            ("2026-07-26", "claude-opus-5"),
            ("2026-07-27", "claude-opus-5"),
            ("2026-07-28", "gpt-5.6-sol"),
        ] {
            let t = tokens(1_000, 2_000);
            ledger.add(day, "claude_code", model, &t);
            ledger.add_project(day, "acme/widgets", model, &t);
            // Two sessions in the one project, so the breakdown has more than a
            // single row and the models column has something to list.
            for session in ["morning", "afternoon"] {
                ledger.add_session(day, session, model, &t);
                ledger.observe_session(
                    session,
                    "claude_code",
                    "acme/widgets",
                    Some(&format!("the {session} run")),
                );
            }
        }

        let detected: Vec<tooling::Detected> = tooling::AI_TOOLS
            .iter()
            .take(3)
            .map(|tool| tooling::Detected {
                tool,
                evidence: vec!["executable:/usr/local/bin/x".to_string()],
            })
            .collect();

        let scan = Scan {
            tools_summary: tooling::summarise(&detected),
            tools: detected,
            #[cfg(feature = "sqlite")]
            sites: crate::scan::sites::summarise(&[crate::scan::sites::Hit {
                domain: "claude.ai".into(),
                visits: 317,
                last_seen_unix: 1_785_100_000,
            }]),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };

        App::new(scan, Timings::default(), prices, CostConfig::default())
    }

    /// Every view at every plausible size. ratatui panics on a layout that does
    /// not fit, so this is the test that catches the most likely runtime failure.
    #[test]
    fn every_view_renders_at_every_plausible_size() {
        for (w, h) in [(200u16, 60u16), (120, 40), (80, 24), (60, 15), (40, 10)] {
            let mut app = populated();
            // Both row heights, both panes focused, and the chart cursor on and
            // off. The states multiply, and the one that used to be easy to leave
            // unexercised is "focused on a pane this size does not draw".
            for detail in [true, false] {
                app.detail = detail;
                for cursor in [false, true] {
                    for tab in Tab::ALL {
                        app.set_tab(*tab);
                        if cursor {
                            app.move_bucket(1);
                        }
                        for _ in 0..2 {
                            app.toggle_focus();
                            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                            terminal
                                .draw(|frame| {
                                    draw(frame, &app);
                                })
                                .unwrap_or_else(|e| {
                                    panic!(
                                        "{:?} at {w}x{h} detail={detail} cursor={cursor}: {e}",
                                        tab.title()
                                    )
                                });
                        }
                    }
                }
            }
        }
    }

    /// The layout tier the width gate picks, at each size the smoke test covers.
    /// Pinned because it is the whole behaviour of the gate, and a silent slide
    /// from side-by-side to dropped would look like the feature disappearing.
    #[test]
    fn the_layout_tier_matches_the_terminal_width() {
        let mut app = populated();
        app.set_tab(Tab::Projects);
        for (w, h, beside) in [
            (200u16, 60u16, true),
            (120, 40, true),
            (80, 24, false),
            (60, 15, false),
            (40, 10, false),
        ] {
            let out = rendered(&app, w, h);
            assert_eq!(
                out.contains("sessions in this project"),
                beside,
                "at {w}x{h} the breakdown should{} be drawn",
                if beside { "" } else { " not" }
            );
        }
    }

    #[test]
    fn every_view_renders_with_a_completely_empty_scan() {
        // The first run on a fresh machine with no AI installed at all.
        let scan = Scan {
            tools: Vec::new(),
            tools_summary: Default::default(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: Default::default(),
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );

        for tab in Tab::ALL {
            app.set_tab(*tab);
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| {
                    draw(frame, &app);
                })
                .unwrap_or_else(|e| panic!("{:?}: {e}", tab.title()));
        }
    }

    #[test]
    fn the_help_overlay_renders_over_every_view() {
        let mut app = populated();
        app.show_help = true;
        for tab in Tab::ALL {
            app.set_tab(*tab);
            app.show_help = true;
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| {
                    draw(frame, &app);
                })
                .unwrap();
        }
    }

    #[test]
    fn a_failed_section_is_named_in_the_footer() {
        let mut app = populated();
        app.scan.failed.push("usage");

        let out = rendered(&app, 120, 20);
        assert!(
            out.contains("FAILED"),
            "a panicking section must be visible"
        );
        assert!(out.contains("usage"));
    }

    #[test]
    fn an_unpriced_model_is_never_shown_as_free() {
        // The one arithmetic lie this tool could tell.
        let mut app = populated();
        app.set_tab(Tab::Cost);
        // Default `Prices` is empty, so every model is unpriced.
        assert!(app.prices.is_empty());

        let out = rendered(&app, 120, 30);
        // With no price table at all, Cost explains itself rather than showing
        // a column of $0.00.
        assert!(out.contains("nothing can be costed"));
        assert!(!out.contains("$0.00"));
    }

    /// Rendered buffer as one string, for the assertions below.
    fn rendered(app: &App, w: u16, h: u16) -> String {
        frame_of(app, w, h).0
    }

    /// The buffer and the click targets that came with it.
    fn frame_of(app: &App, w: u16, h: u16) -> (String, Hits) {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut hits = Hits::default();
        terminal.draw(|frame| hits = draw(frame, app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        (text, hits)
    }

    #[test]
    fn the_overview_charts_one_unit_and_switches_between_them() {
        // A table of our own, so the test does not depend on which models the
        // built-in fallback happens to carry.
        let prices = crate::pricing::Prices::parse(
            r#"{"claude-opus-5": {"input_cost_per_token": 0.000005,
                                  "output_cost_per_token": 0.000025,
                                  "cache_read_input_token_cost": 0.0000005,
                                  "cache_creation_input_token_cost": 0.00000625}}"#,
        )
        .expect("a one-model table parses");
        let mut app = populated_with(prices);
        app.set_tab(Tab::Overview);

        // One chart, in the unit the reader picked. The two used to be drawn
        // together and were the same data twice — same buckets, same series, same
        // silhouette, and the legend printed for both. Asserted on the key hint
        // rather than the word "tokens", which the spend title also carries: it
        // names the unit the key would switch *to*.
        let spend = rendered(&app, 120, 40);
        assert!(spend.contains("estimated spend"), "spend by default");
        assert!(spend.contains("[u] tokens"), "and says what u would give");
        // Priced usage, so the chart carries a real figure rather than $0.
        assert!(app.total_usd() > 0.0);

        app.toggle_unit();
        let tokens = rendered(&app, 120, 40);
        assert!(tokens.contains("[u] spend"), "u swapped the unit");
        assert!(
            !tokens.contains("estimated spend"),
            "and the spend chart is gone, not stacked under it"
        );
    }

    #[test]
    fn the_overview_never_shows_cost_as_zero_without_prices() {
        // Same rule as the Cost view: no price table means no chart, because a
        // flat $0 series would read as free.
        let mut app = populated();
        app.set_tab(Tab::Overview);
        assert!(app.prices.is_empty());

        let out = rendered(&app, 120, 40);
        assert!(out.contains("nothing can be costed"));
        assert!(!out.contains("$0.00"));
    }

    /// The SPEND card prices seats, not tokens: nothing known shows `–` and
    /// says how to fix it, a detected plan is `≈` an estimate at list price,
    /// and a tool without any figure makes the total `≥` a floor.
    #[test]
    fn the_spend_card_prices_seats_not_tokens() {
        // populated(): no [cost.subscriptions] and no detected plan.
        let mut app = populated();
        app.set_tab(Tab::Overview);
        let out = rendered(&app, 150, 40);
        assert!(
            out.contains("TOKEN COST"),
            "the API arithmetic keeps a card"
        );
        assert!(out.contains("no seat price known"));
        assert!(!out.contains("/mo"), "no figure is invented");
    }

    #[test]
    fn a_detected_plan_prices_the_spend_card_as_an_estimate() {
        let mut ledger = Ledger::default();
        ledger.add("2026-07-26", "codex", "gpt-5.6", &tokens(1_000, 2_000));

        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            // As `scan::run` would have merged it, from either source.
            plans: std::collections::BTreeMap::from([(
                "codex".to_string(),
                crate::scan::plans::DetectedPlan {
                    plan: "team".to_string(),
                    source: crate::scan::plans::PlanSource::Transcript,
                },
            )]),
            failed: Vec::new(),
            demo: false,
        };
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        app.set_tab(Tab::Overview);

        let out = rendered(&app, 150, 40);
        assert!(
            out.contains("\u{2248}$25.00/mo"),
            "ChatGPT Business's monthly list price, flagged an estimate"
        );
        assert!(out.contains("1 plan(s) detected"));
    }

    #[test]
    fn a_tool_without_any_figure_makes_the_spend_card_a_floor() {
        let mut ledger = Ledger::default();
        ledger.add(
            "2026-07-26",
            "claude_code",
            "claude-opus-5",
            &tokens(1_000, 2_000),
        );
        // Usage from a tool with no configured price and no named plan.
        ledger.add(
            "2026-07-26",
            "opencode",
            "big-pickle",
            &tokens(1_000, 2_000),
        );

        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let mut cost = CostConfig::default();
        cost.subscriptions.insert("claude_code".into(), 150.0);
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            cost,
        );
        app.set_tab(Tab::Overview);

        let out = rendered(&app, 150, 40);
        assert!(
            out.contains("\u{2265}$150.00/mo"),
            "a configured seat, floored by the tool beside it"
        );
        assert!(
            !out.contains("\u{2248}$150.00"),
            "configured is not an estimate"
        );
        assert!(out.contains("1 tool(s) unpriced"));
    }

    /// The Tools view names the plan each tool is signed into, as the raw
    /// slug the tool wrote — and a tool naming none shows a dash, not a guess.
    #[test]
    fn the_tools_view_names_the_plan_a_tool_is_on() {
        let detected: Vec<tooling::Detected> = tooling::AI_TOOLS
            .iter()
            .take(3)
            .map(|tool| tooling::Detected {
                tool,
                evidence: vec!["config:~/.x".to_string()],
            })
            .collect();
        let scan = Scan {
            tools_summary: tooling::summarise(&detected),
            tools: detected,
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: Default::default(),
            plans: std::collections::BTreeMap::from([(
                "claude_code".to_string(),
                crate::scan::plans::DetectedPlan {
                    plan: "team_tier_1".to_string(),
                    source: crate::scan::plans::PlanSource::Account,
                },
            )]),
            failed: Vec::new(),
            demo: false,
        };
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        app.set_tab(Tab::Tools);

        let out = rendered(&app, 150, 30);
        assert!(out.contains("PLAN"), "the plan column header");
        assert!(out.contains("team_tier_1"), "the slug the tool wrote");
        assert!(out.contains("$/MO"), "the seat price column header");
        assert!(
            out.contains("\u{2248}$30.00"),
            "the plan's list price, marked an estimate"
        );
    }

    /// A configured seat price shows plain — the estimate marker is what
    /// tells the reader which figures are worth correcting in config.
    #[test]
    fn a_configured_seat_price_shows_plain_on_the_tools_view() {
        let detected: Vec<tooling::Detected> = tooling::AI_TOOLS
            .iter()
            .take(1)
            .map(|tool| tooling::Detected {
                tool,
                evidence: vec!["config:~/.x".to_string()],
            })
            .collect();
        let scan = Scan {
            tools_summary: tooling::summarise(&detected),
            tools: detected,
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: Default::default(),
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let mut cost = CostConfig::default();
        cost.subscriptions.insert("claude_code".into(), 100.0);
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            cost,
        );
        app.set_tab(Tab::Tools);

        let out = rendered(&app, 150, 30);
        assert!(out.contains("$100.00"), "the configured figure");
        assert!(
            !out.contains("\u{2248}$100.00"),
            "configured is not an estimate"
        );
    }

    #[test]
    fn the_projects_view_lists_every_attributed_repository() {
        let mut app = populated();
        app.set_tab(Tab::Projects);
        assert_eq!(app.row_count(), 1, "one repository in the fixture");

        // Narrow enough that the breakdown stands down, so the table keeps every
        // column it has. Beside the breakdown it sheds MESSAGES and LAST USED.
        let out = rendered(&app, 110, 30);
        assert!(out.contains("acme/widgets"), "the repo slug is a row");
        assert!(out.contains("PROJECT") && out.contains("LAST USED"));
        assert!(out.contains("1 project over 30 days"));

        // Beside the breakdown, the three figures a ranking is read for survive.
        let beside = rendered(&app, 150, 30);
        assert!(beside.contains("acme/widgets"));
        for column in ["PROJECT", "TOKENS", "COST"] {
            assert!(beside.contains(column), "the narrow table dropped {column}");
        }
    }

    #[test]
    fn the_projects_chart_follows_the_selection() {
        let mut ledger = Ledger::default();
        let t = tokens(1_000, 2_000);
        ledger.add("2026-07-26", "claude_code", "claude-opus-5", &t);
        ledger.add_project("2026-07-26", "acme/widgets", "claude-opus-5", &t);
        ledger.add("2026-07-27", "claude_code", "claude-opus-5", &t);
        ledger.add_project("2026-07-27", "other/thing", "claude-opus-5", &t);

        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        app.set_tab(Tab::Projects);

        // Both repos are rows, and only the selected one titles the chart. Each
        // project's chart spans every day any project was active — the idle day
        // is a zero column, not a missing one, or two bars a fortnight apart
        // would sit side by side as if they were consecutive.
        let first = app.selected_project().unwrap().repo.clone();
        app.next_row();
        let second = app.selected_project().unwrap().repo.clone();
        assert_ne!(first, second);
        for repo in [&first, &second] {
            let buckets = app.project_buckets(repo);
            assert_eq!(buckets.len(), 2, "both days, padded");
            assert_eq!(
                buckets.iter().filter(|b| b.total > 0).count(),
                1,
                "{repo} was active on one of them"
            );
        }

        let out = rendered(&app, 120, 30);
        assert!(
            out.contains(&second),
            "the chart names the selected project"
        );
    }

    #[test]
    fn the_projects_view_explains_itself_with_no_attribution() {
        // Tool usage with no repository under it at all.
        let mut ledger = Ledger::default();
        ledger.add("2026-07-28", "claude_code", "claude-opus-5", &tokens(1, 2));
        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        app.set_tab(Tab::Projects);
        assert_eq!(app.row_count(), 0);

        let out = rendered(&app, 100, 20);
        assert!(out.contains("No repository-attributed usage"));
    }

    /// The sidebar is on screen in every view, so every entry has to be
    /// reachable at the narrowest width the layout claims to support — where it
    /// is collapsed to the digits.
    #[test]
    fn every_view_is_listed_in_the_sidebar_at_both_widths() {
        let mut app = populated();
        app.scan.demo = true;

        for (w, expect_titles) in [(80u16, false), (110, true)] {
            let (_, hits) = frame_of(&app, w, 24);
            for tab in Tab::ALL {
                assert!(
                    (0..24).any(|row| hits.tab_at(1, row) == Some(*tab)),
                    "{} is not clickable at {w} columns",
                    tab.title()
                );
            }
            let text = rendered(&app, w, 24);
            assert_eq!(
                text.contains("Projects"),
                expect_titles,
                "at {w} columns the sidebar should{} spell out titles",
                if expect_titles { "" } else { " not" }
            );
        }
    }

    /// The hit band and the drawn entry have to be the same row, or a click
    /// lands one view over. Checked against the rendered glyphs.
    #[test]
    fn a_view_is_clickable_exactly_where_its_entry_is_drawn() {
        const W: u16 = 110;
        const H: u16 = 24;
        let app = populated();
        let (text, hits) = frame_of(&app, W, H);
        let cells: Vec<char> = text.chars().collect();

        for (i, tab) in Tab::ALL.iter().enumerate() {
            let row = (0..H)
                .find(|row| hits.tab_at(1, *row) == Some(*tab))
                .unwrap_or_else(|| panic!("{} has no clickable band", tab.title()));
            let drawn: String = cells
                [row as usize * W as usize..row as usize * W as usize + NAV_WIDE as usize]
                .iter()
                .collect();
            assert!(
                drawn.contains(tab.title()),
                "the band for {} is row {row}, which draws {drawn:?}",
                tab.title()
            );
            assert!(
                drawn.contains(&(i + 1).to_string()),
                "row {row} should carry the digit that jumps to it: {drawn:?}"
            );
        }
    }

    /// The mark heads the sidebar; it is a logo, not a seventh view.
    #[test]
    fn the_mark_is_not_a_navigation_target() {
        let app = populated();
        let (_, hits) = frame_of(&app, 110, 24);
        for column in 0..NAV_WIDE {
            assert_eq!(
                hits.tab_at(column, 0),
                None,
                "column {column} of the mark row"
            );
        }
    }

    /// Nothing in the body is a view, however far down the pointer is.
    #[test]
    fn a_view_is_only_clickable_inside_the_sidebar() {
        let app = populated();
        let (_, hits) = frame_of(&app, 110, 24);
        assert!(hits.tab_at(1, 2).is_some(), "the sidebar itself still hits");
        for row in 0..24 {
            assert_eq!(
                hits.tab_at(NAV_WIDE + 5, row),
                None,
                "a body column claimed a view on row {row}"
            );
        }
    }

    /// The body now starts at the top of the frame and to the right of the
    /// sidebar, so both axes of the table band moved.
    #[test]
    fn a_click_picks_the_row_under_the_pointer() {
        let mut app = populated();
        app.set_tab(Tab::Tools);
        assert!(app.row_count() >= 3, "fixture needs three tools");

        let (_, hits) = frame_of(&app, 120, 30);
        // The body opens at y=0; ROWS_TOP is the border, the header and its
        // margin. Its first column is the sidebar width plus the panel border.
        let first = ROWS_TOP;
        let column = NAV_WIDE + 1;
        assert_eq!(hits.row_at(column, first), Some(0));
        assert_eq!(hits.row_at(column, first + 2), Some(2));
        // The header and the panel border are not rows.
        assert_eq!(hits.row_at(column, first - 1), None);
        // Neither is anything left of the body.
        assert_eq!(hits.row_at(NAV_WIDE - 1, first), None);
    }

    /// The offset ratatui scrolled to is the whole reason [`row_hits`] reads the
    /// state back after rendering: on a scrolled table the top visible line is
    /// not row 0, and a click there must not select row 0.
    #[test]
    fn a_click_on_a_scrolled_table_resolves_to_the_visible_row() {
        let mut app = populated();
        app.set_tab(Tab::Tools);
        app.last_row();
        let last = app.row_count() - 1;

        // Height 7 is the shortest frame the layout accepts, and it leaves the
        // table two rows for three tools — so it has to be scrolled.
        let (_, hits) = frame_of(&app, 120, 7);
        let column = NAV_WIDE + 1;
        assert_eq!(
            hits.row_at(column, ROWS_TOP + 1),
            Some(last),
            "the bottom visible line is the selected row"
        );
        assert_eq!(
            hits.row_at(column, ROWS_TOP),
            Some(last - 1),
            "the line above it is the row before, not row 0"
        );
    }

    /// A click on the empty space under a short table still resolves to a row
    /// index — it is `App` that rejects one past the end, so that a stray click
    /// does not snap the selection to the last row.
    #[test]
    fn a_click_past_the_last_row_leaves_the_selection_alone() {
        let mut app = populated();
        app.set_tab(Tab::Tools);
        let last = app.row_count() - 1;
        app.select_row(last);

        let (_, hits) = frame_of(&app, 120, 30);
        let empty = hits
            .row_at(NAV_WIDE + 1, ROWS_TOP + app.row_count() as u16 + 2)
            .expect("the band extends past the last row on a tall frame");
        assert!(empty > last, "that line is below the data");

        app.select_row(empty);
        assert_eq!(app.selected, last, "a click on empty space selects nothing");
    }

    #[test]
    fn a_view_without_a_table_has_no_clickable_rows() {
        let mut app = populated();
        app.set_tab(Tab::Overview);
        let (_, hits) = frame_of(&app, 120, 30);
        for row in 0..30 {
            assert_eq!(
                hits.row_at(NAV_WIDE + 10, row),
                None,
                "row {row} claimed a table row"
            );
        }
    }

    /// Every column the breakdown promises, on the project the selection is on.
    #[test]
    fn the_projects_view_breaks_the_selection_down_by_session() {
        let mut app = populated();
        app.set_tab(Tab::Projects);
        let repo = app
            .selected_project()
            .expect("the fixture has a project")
            .repo
            .clone();
        let sessions = app.sessions_in(&repo);
        assert_eq!(sessions.len(), 2, "the fixture has two sessions in it");

        // Wide, so the detail line has room for the whole story. The tool, the
        // day, the token total, the message count and the model list all moved off
        // the header row onto the detail line, which is what fits them beside a
        // projects table.
        let out = rendered(&app, 210, 30);
        for column in ["SESSION", "COST"] {
            assert!(out.contains(column), "the breakdown is missing {column}");
        }
        assert!(out.contains("2 sessions in this project"));
        assert!(out.contains("the morning run"), "a session title");
        assert!(out.contains("claude_code"), "the tool, on the detail line");
        assert!(out.contains("2026-07-28"), "the day, on the detail line");
        assert!(
            out.contains("claude-opus-5"),
            "the models, on the detail line"
        );
        assert!(out.contains("msg"), "the message count, on the detail line");

        // Narrower, the detail line is cut from the right — so the order it is
        // written in is a priority order. The model list goes last because half a
        // model name still reads as one, and half a date does not.
        let tight = rendered(&app, 150, 30);
        assert!(tight.contains("claude_code"), "the tool survives the cut");
        assert!(tight.contains("2026-07-28"), "and so does the day");
        assert!(
            tight.contains('\u{2026}'),
            "and the line says it was cut rather than just stopping"
        );
    }

    /// The breakdown follows the selection, because that is what the chart above
    /// it already does.
    #[test]
    fn the_breakdown_only_shows_sessions_of_the_selected_project() {
        let mut app = populated();
        app.set_tab(Tab::Projects);
        let repo = app.selected_project().unwrap().repo.clone();

        assert!(
            app.sessions_in(&repo).iter().all(|s| s.repo == repo),
            "a session from another project leaked into the breakdown"
        );
        assert!(
            app.sessions_in("nobody/nothing").is_empty(),
            "a project with no sessions gets no rows"
        );
    }

    /// A session total is costed exactly as the project row above it is, or the
    /// breakdown would not add up to the thing it breaks down.
    #[test]
    fn session_totals_agree_with_the_project_they_sit_under() {
        let mut app = populated();
        app.set_tab(Tab::Projects);
        let project = app.selected_project().expect("a project").clone();
        let sessions = app.sessions_in(&project.repo);
        assert!(!sessions.is_empty());

        let tokens: u64 = sessions.iter().map(|s| s.tokens.total()).sum();
        let messages: u64 = sessions.iter().map(|s| s.tokens.messages).sum();
        // The fixture books the same tokens to the project once and to each of
        // its two sessions, so the sessions double the project on purpose. What
        // must hold is that the two are in step, not that they are equal.
        assert_eq!(
            tokens,
            project.tokens * sessions.len() as u64,
            "session tokens are not the project's, per session"
        );
        assert_eq!(messages, project.messages * sessions.len() as u64);
    }

    /// A short terminal keeps the project table whole rather than shrinking it
    /// to make room for a breakdown.
    #[test]
    fn the_breakdown_stands_down_on_a_narrow_terminal() {
        let mut app = populated();
        app.set_tab(Tab::Projects);

        // Width is the binding constraint now that the pane is beside the table
        // rather than under it: a horizontal pane costs no rows at all, so a
        // short frame keeps it and a narrow one cannot.
        assert!(
            rendered(&app, 150, 18).contains("sessions in this project"),
            "a wide but short frame still shows the breakdown"
        );
        assert!(
            !rendered(&app, 100, 40).contains("sessions in this project"),
            "a narrow frame gives the whole width to the project table"
        );
    }

    /// The invariant the move is *for*: the two panes share a row, they are not
    /// stacked. A buffer grep cannot say that, so this asserts on the bands.
    #[test]
    fn the_breakdown_sits_beside_the_table_not_under_it() {
        let mut app = populated();
        app.set_tab(Tab::Projects);
        let (_, hits) = frame_of(&app, 150, 30);

        let table = hits.rows.expect("the project table has a band");
        let pane = hits.sessions.expect("the breakdown has a band");
        assert_eq!(table.area.y, pane.area.y, "the two panes start on one row");
        assert!(
            pane.area.x > table.area.x + table.area.width,
            "the breakdown is to the right of the table, not overlapping it"
        );
    }

    /// Neither band may ever answer for a point inside the other, or a click
    /// moves the cursor the reader was not pointing at.
    #[test]
    fn the_two_bands_never_overlap() {
        let mut app = populated();
        app.set_tab(Tab::Projects);
        for (w, h) in [(150u16, 30u16), (200, 60), (120, 40)] {
            let (_, hits) = frame_of(&app, w, h);
            for row in 0..h {
                for column in 0..w {
                    assert!(
                        !(hits.row_at(column, row).is_some()
                            && hits.session_row_at(column, row).is_some()),
                        "both bands claim ({column},{row}) at {w}x{h}"
                    );
                }
            }
        }
    }

    /// Pins `ROWS_TOP` and `lines_per_row` against the drawn buffer at once: the
    /// line a click resolves to row 0 must be the line carrying row 0's name, and
    /// the line under it must carry row 0's detail. Nothing else checks that the
    /// arithmetic and the renderer agree.
    #[test]
    fn a_two_line_row_is_clickable_exactly_where_it_is_drawn() {
        const W: u16 = 120;
        let mut app = populated();
        app.set_tab(Tab::Usage);
        assert!(app.detail, "the detail line is on by default");
        assert!(app.row_count() >= 2, "fixture needs two models");

        let (text, hits) = frame_of(&app, W, 30);
        let cells: Vec<char> = text.chars().collect();
        let line_of = |row: u16| -> String {
            cells[row as usize * W as usize..(row as usize + 1) * W as usize]
                .iter()
                .collect()
        };

        let first = (0..30)
            .find(|row| hits.row_at(NAV_WIDE + 2, *row) == Some(0))
            .expect("some line resolves to row 0");
        let head = line_of(first);
        assert!(
            theme::SERIES
                .iter()
                .any(|(texture, _)| head.contains(texture)),
            "row 0's first line should carry its series texture: {head:?}"
        );
        assert!(
            line_of(first + 1).contains("In:"),
            "row 0's second line should carry the detail: {:?}",
            line_of(first + 1)
        );
        // And the line below *that* is already the next row.
        assert_eq!(hits.row_at(NAV_WIDE + 2, first + 1), Some(0));
        assert_eq!(hits.row_at(NAV_WIDE + 2, first + 2), Some(1));
    }

    /// Every line of a tall row resolves to that row, and the remainder line a
    /// two-line table leaves unpainted resolves to nothing — clicking blank
    /// space must not select a row the reader cannot see.
    #[test]
    fn every_line_of_a_tall_row_selects_the_same_row() {
        let mut app = populated();
        app.set_tab(Tab::Usage);

        // Height chosen so the band is odd and one trailing line is left over.
        let (_, hits) = frame_of(&app, 120, 23);
        let column = NAV_WIDE + 2;
        let seen: Vec<Option<usize>> = (0..23).map(|row| hits.row_at(column, row)).collect();

        let hit: Vec<usize> = seen.iter().flatten().copied().collect();
        assert!(!hit.is_empty(), "some lines are clickable");
        for index in 0..=*hit.iter().max().unwrap() {
            let count = hit.iter().filter(|i| **i == index).count();
            assert_eq!(
                count, DETAIL_LINES as usize,
                "row {index} should claim exactly {DETAIL_LINES} lines, claimed {count}"
            );
        }
        // Monotone: the band never jumps backwards.
        assert!(hit.windows(2).all(|w| w[1] >= w[0]), "band is not ordered");
    }

    /// The flat mode is the old table exactly, so the one-line hit arithmetic is
    /// unchanged — this is what lets the Tools click tests stand.
    #[test]
    fn turning_the_detail_line_off_restores_one_line_rows() {
        let mut app = populated();
        app.set_tab(Tab::Usage);
        app.toggle_detail();
        assert!(!app.detail);

        let (text, hits) = frame_of(&app, 120, 30);
        assert!(text.contains("CACHE READ"), "the numeric columns come back");
        let column = NAV_WIDE + 2;
        let first = (0..30)
            .find(|row| hits.row_at(column, *row) == Some(0))
            .expect("some line resolves to row 0");
        assert_eq!(hits.row_at(column, first + 1), Some(1), "one line per row");
    }

    /// The figures on the detail line are the ones that were previously summed
    /// into TOTAL with no column of their own.
    #[test]
    fn the_detail_line_carries_the_tokens_that_were_invisible() {
        let mut app = populated();
        app.set_tab(Tab::Usage);
        let out = rendered(&app, 120, 30);
        for label in ["In:", "Out:", "CR:", "CW:"] {
            assert!(out.contains(label), "the detail line is missing {label}");
        }
        assert!(out.contains("SHARE"), "the share column");
    }

    /// Two repos with different session counts, so a cursor that survived a
    /// project move would be pointing into the wrong list.
    fn two_projects_with_sessions() -> App {
        let mut ledger = Ledger {
            titles_enabled: true,
            ..Default::default()
        };
        let t = tokens(1_000, 2_000);
        // acme/widgets: three sessions. other/thing: one.
        for session in ["a", "b", "c"] {
            ledger.add("2026-07-26", "claude_code", "claude-opus-5", &t);
            ledger.add_project("2026-07-26", "acme/widgets", "claude-opus-5", &t);
            ledger.add_session("2026-07-26", session, "claude-opus-5", &t);
            ledger.observe_session(session, "claude_code", "acme/widgets", Some(session));
        }
        ledger.add("2026-07-27", "claude_code", "claude-opus-5", &t);
        ledger.add_project("2026-07-27", "other/thing", "claude-opus-5", &t);
        ledger.add_session("2026-07-27", "z", "claude-opus-5", &t);
        ledger.observe_session("z", "claude_code", "other/thing", Some("z"));

        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        app.set_tab(Tab::Projects);
        app
    }

    /// The reset that stops the second pane pointing at a stale session. ratatui
    /// clamps an out-of-range selection silently, so getting this wrong shows up
    /// as the wrong row highlighted rather than as a crash.
    #[test]
    fn the_session_cursor_never_survives_a_move_of_the_project_cursor() {
        let mut app = two_projects_with_sessions();
        let big = app
            .repos()
            .iter()
            .position(|r| r.repo == "acme/widgets")
            .expect("the three-session repo is a row");
        app.select_row(big);
        assert_eq!(app.session_count(), 3);

        app.toggle_focus();
        app.last_row();
        assert_eq!(app.session_selected(), 2, "on the last of three");

        // Moving the project cursor replaces the pane's contents entirely.
        app.select_row(if big == 0 { 1 } else { 0 });
        assert_eq!(
            app.session_selected(),
            0,
            "the session cursor went back to the top of the new list"
        );
        assert!(
            app.session_selected() < app.session_count().max(1),
            "and is inside it"
        );
    }

    /// Focus decides which list j/k walks, and moving one must not move the other.
    #[test]
    fn focus_decides_which_pane_the_keys_drive() {
        let mut app = two_projects_with_sessions();
        let big = app
            .repos()
            .iter()
            .position(|r| r.repo == "acme/widgets")
            .unwrap();
        app.select_row(big);

        let project = app.selected;
        app.toggle_focus();
        app.next_row();
        assert_eq!(app.selected, project, "the project cursor stayed put");
        assert_eq!(app.session_selected(), 1, "the session cursor moved");

        app.toggle_focus();
        app.next_row();
        assert_ne!(app.selected, project, "the project cursor moves again");
    }

    /// Focus can never be left on a pane the view is not drawing.
    #[test]
    fn focus_will_not_move_into_a_pane_with_nothing_in_it() {
        let mut app = two_projects_with_sessions();
        let small = app
            .repos()
            .iter()
            .position(|r| r.repo == "other/thing")
            .unwrap();
        app.select_row(small);

        // Switching view resets focus, or the next view takes j/k for a pane it
        // does not have.
        app.toggle_focus();
        app.set_tab(Tab::Tools);
        assert_eq!(app.focus(), crate::app::Pane::Primary);
        assert_eq!(app.session_count(), 0, "Tools has no second pane");

        // And the toggle itself declines rather than stranding the keys.
        app.toggle_focus();
        assert_eq!(app.focus(), crate::app::Pane::Primary);
    }

    /// The gridline glyph only appears on tick rows, and only where no bar is —
    /// filling the gap between bars would run thirty buckets into one ribbon.
    #[test]
    fn a_chart_draws_gridlines_on_its_tick_rows() {
        let mut app = populated();
        app.set_tab(Tab::Usage);
        let out = rendered(&app, 140, 40);
        assert!(
            out.contains('\u{b7}'),
            "a tall chart should carry gridline dots"
        );
        // More than the two marks the old axis drew: a mid tick has a value on it.
        assert!(out.contains("tokens by model"), "the chart is there at all");
    }

    /// The cursor changes what the title answers for, which is the whole point of
    /// it — the bucket's own total and split rather than the window's.
    #[test]
    fn the_chart_cursor_reports_the_bucket_it_is_on() {
        let mut app = populated();
        app.set_tab(Tab::Usage);

        let before = rendered(&app, 140, 40);
        assert!(
            before.contains("buckets \u{b7}"),
            "the window title by default"
        );
        assert_eq!(app.bucket_back(), None);

        app.move_bucket(1);
        assert_eq!(
            app.bucket_back(),
            Some(0),
            "the first press lands on the newest"
        );
        let after = rendered(&app, 140, 40);
        assert!(
            after.contains("bucket \u{b7} [w] regroup"),
            "the title now names the bucket: {after:?}"
        );

        app.clear_bucket();
        assert_eq!(app.bucket_back(), None);
        assert!(rendered(&app, 140, 40).contains("buckets \u{b7}"));
    }

    /// Bold, not a fifth colour — the four series hues are the whole budget. A
    /// text dump cannot show this, so it is asserted against the cell styles.
    #[test]
    fn the_chart_cursor_marks_its_column_by_weight() {
        let mut app = populated();
        app.set_tab(Tab::Usage);
        app.move_bucket(1);

        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, &app);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        let bold = buffer
            .content()
            .iter()
            .filter(|cell| {
                cell.symbol() == "\u{2588}"
                    && cell.modifier.contains(ratatui::style::Modifier::BOLD)
            })
            .count();
        assert!(bold > 0, "the bucket under the cursor draws in bold");

        app.clear_bucket();
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, &app);
            })
            .unwrap();
        let none = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|cell| {
                cell.symbol() == "\u{2588}"
                    && cell.modifier.contains(ratatui::style::Modifier::BOLD)
            })
            .count();
        assert_eq!(none, 0, "with no cursor nothing is bold");
    }

    /// Regrouping rebuilds the buckets, so "three ago" would point at a different
    /// span of days. The cursor is dropped rather than moved silently.
    #[test]
    fn regrouping_and_switching_view_drop_the_chart_cursor() {
        let mut app = populated();
        app.set_tab(Tab::Usage);

        app.move_bucket(1);
        assert!(app.bucket_back().is_some());
        app.cycle_granularity();
        assert_eq!(app.bucket_back(), None, "regrouping drops it");

        app.move_bucket(1);
        assert!(app.bucket_back().is_some());
        app.set_tab(Tab::Cost);
        assert_eq!(app.bucket_back(), None, "so does changing view");
    }

    /// The cursor cannot be pushed off either end of the series.
    #[test]
    fn the_chart_cursor_stays_inside_the_series() {
        let mut app = populated();
        app.set_tab(Tab::Usage);
        // The series the Usage view actually charts, which is also what
        // `bucket_count` now measures.
        let count = app.model_token_buckets().len();
        assert!(count > 0);

        for _ in 0..count + 10 {
            app.move_bucket(1);
        }
        assert_eq!(app.bucket_back(), Some(count - 1), "clamped at the oldest");

        for _ in 0..count + 10 {
            app.move_bucket(-1);
        }
        assert_eq!(app.bucket_back(), Some(0), "and at the newest");
    }

    /// The property this palette exists for: a model is the same colour in the
    /// table as it is in the chart above it. They agree because both index the
    /// one list, and this asserts that they do rather than that they happen to.
    #[test]
    fn the_chart_and_the_table_agree_on_model_colour() {
        let mut app = populated();
        app.set_tab(Tab::Usage);

        let charted = app.model_names();
        assert!(!charted.is_empty(), "the fixture has models");

        for row in app.models() {
            let folded = app.fold_model(&row.model);
            let slot = charted
                .iter()
                .position(|name| *name == folded)
                .expect("a model is either named or folded into `other`");
            assert_eq!(
                model_tint(&app, &row.model),
                theme::model(slot),
                "{} is a different colour in the table than in the chart",
                row.model
            );
        }

        // And the chart names them: the legend is built from the same list.
        let out = rendered(&app, 160, 40);
        for name in charted {
            assert!(out.contains(name.as_str()), "the legend is missing {name}");
        }
    }

    /// Beyond the palette, models fold together rather than borrowing a colour
    /// that already means another model.
    #[test]
    fn models_past_the_palette_fold_into_other() {
        let mut ledger = Ledger::default();
        let tokens = tokens(1_000, 2_000);
        // One more model than there are named slots.
        for index in 0..crate::app::MODEL_SLOTS + 3 {
            ledger.add(
                "2026-07-26",
                "claude_code",
                &format!("model-{index:02}"),
                &tokens,
            );
        }
        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let mut app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        app.set_tab(Tab::Usage);

        let charted = app.model_names();
        assert_eq!(
            charted.len(),
            crate::app::MODEL_SLOTS + 1,
            "six named models and one `other`: {charted:?}"
        );
        assert_eq!(charted.last().unwrap(), crate::app::OTHER_MODELS);

        // Every folded model draws in the neutral, in the table as in the chart.
        let folded: Vec<&crate::app::ModelRow> = app
            .models()
            .iter()
            .filter(|row| !charted.contains(&row.model))
            .collect();
        assert!(!folded.is_empty(), "some models were folded");
        for row in &folded {
            assert_eq!(
                model_tint(&app, &row.model),
                theme::MODEL_OTHER,
                "{} should draw as `other`",
                row.model
            );
        }
    }

    /// The tool axis, which the Overview had lost entirely once its charts moved
    /// to models. Also the invariant that makes it trustworthy: the parts sum to
    /// the whole.
    #[test]
    fn spend_by_tool_and_model_each_account_for_the_whole_total() {
        let app = populated_with(prices_for_a_model());
        assert!(app.total_usd() > 0.0);

        let by_tool: f64 = app.spend_by_tool().iter().map(|(_, usd)| usd).sum();
        let by_model: f64 = app.spend_by_model().iter().map(|(_, usd, _)| usd).sum();
        for (label, sum) in [("tool", by_tool), ("model", by_model)] {
            assert!(
                (sum - app.total_usd()).abs() < 1e-9,
                "spend by {label} sums to {sum}, not {}",
                app.total_usd()
            );
        }
        // Biggest first, so a ranking can take the head and sum the tail.
        let tools = app.spend_by_tool();
        assert!(tools.windows(2).all(|w| w[0].1 >= w[1].1), "{tools:?}");
    }

    /// Distinct models, not the `(tool, model)` pairs the table lists. The fixture
    /// runs one model under one tool, so a second tool running the same model must
    /// not bump the count.
    #[test]
    fn the_token_card_counts_models_not_pairs() {
        let mut ledger = Ledger::default();
        let t = tokens(1_000, 2_000);
        ledger.add("2026-07-26", "claude_code", "claude-opus-5", &t);
        ledger.add("2026-07-26", "opencode", "claude-opus-5", &t);

        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: crate::scan::usage::Usage {
                ledger,
                window_days: 30,
                ..Default::default()
            },
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let app = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        assert_eq!(app.models().len(), 2, "two (tool, model) pairs");
        assert_eq!(app.distinct_models(), 1, "but one model");
    }

    /// Neither derived figure may be invented. A zero-day window is not a
    /// hypothetical — the empty-scan test runs with exactly that.
    #[test]
    fn the_rate_and_the_trend_decline_rather_than_divide_by_zero() {
        let scan = Scan {
            tools_summary: Default::default(),
            tools: Vec::new(),
            #[cfg(feature = "sqlite")]
            sites: Default::default(),
            usage: Default::default(),
            plans: Default::default(),
            failed: Vec::new(),
            demo: false,
        };
        let empty = App::new(
            scan,
            Timings::default(),
            crate::pricing::Prices::default(),
            CostConfig::default(),
        );
        assert_eq!(empty.scan.usage.window_days, 0, "the empty scan's window");
        assert_eq!(empty.daily_rate(), None, "no rate without a window");
        assert!(empty.spend_trend().is_none(), "and no trend without days");

        // The fixture has three active days, under the minimum for a half-window
        // comparison to say anything.
        let app = populated();
        assert!(app.daily_rate().is_some(), "a 30-day window does divide");
        assert!(
            app.spend_trend().is_none(),
            "three days is below TREND_MIN_DAYS"
        );
    }

    /// A ranking names the head and sums the tail, so the panel still accounts for
    /// everything even though it shows five rows.
    #[test]
    fn a_ranking_caps_its_rows_and_sums_the_rest() {
        let app = populated_with(prices_for_a_model());
        let out = rendered(&app, 165, 38);

        for panel in ["by model", "by tool", "by repository"] {
            assert!(out.contains(panel), "the {panel} ranking is drawn");
        }

        // The demo fixture is small; build a wide one to exercise the cap.
        let ranks: Vec<Rank> = (0..12)
            .map(|i| Rank {
                name: format!("thing-{i:02}"),
                usd: (12 - i) as f64,
                floor: false,
            })
            .collect();
        let shown = RANK_SHOWN.min(ranks.len());
        let tail: f64 = ranks[shown..].iter().map(|r| r.usd).sum();
        let head: f64 = ranks[..shown].iter().map(|r| r.usd).sum();
        let total: f64 = ranks.iter().map(|r| r.usd).sum();
        assert!(
            (head + tail - total).abs() < 1e-9,
            "the head and the summed tail are the whole total"
        );
        assert_eq!(ranks.len() - shown, 7, "seven fold into `+ N more`");
    }

    /// Which bands each size gets. The rankings give way first and the cards
    /// second; the chart is the one thing the page is never without, and it draws
    /// bars at every size the layout accepts.
    ///
    /// Rows are what these compete for, not columns — the bands are stacked, so a
    /// wide short terminal sheds exactly as much as a narrow short one.
    #[test]
    fn the_layout_sheds_bands_in_one_order() {
        let app = populated_with(prices_for_a_model());
        for (w, h, cards, ranks) in [
            (200u16, 60u16, true, true),
            (165, 38, true, true),
            (120, 40, true, true),
            (80, 24, true, true),
            (60, 15, false, false),
            (40, 10, false, false),
        ] {
            let out = rendered(&app, w, h);
            assert_eq!(out.contains("SPEND"), cards, "cards at {w}x{h}");
            assert_eq!(out.contains("by repository"), ranks, "rankings at {w}x{h}");
            // The chart survives every size, and draws rather than bailing out.
            assert!(out.contains("estimated"), "the chart at {w}x{h}");
            assert!(
                out.contains('\u{2588}'),
                "the chart drew no bars at {w}x{h}"
            );
        }
    }

    /// Every money surface explains itself rather than printing a column of
    /// zeros. The repository panel was the one that did not.
    #[test]
    fn a_ranking_explains_itself_with_no_price_table() {
        let app = populated();
        assert!(app.prices.is_empty());
        let out = rendered(&app, 165, 38);
        assert!(out.contains("nothing can be costed"));
        assert!(!out.contains("$0.00"), "and never a column of zeros");
    }

    #[test]
    fn truncate_is_character_safe() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactlyten", 10), "exactlyten");
        assert_eq!(truncate("abcdefghijk", 5), "abcd\u{2026}");
        // Multi-byte characters must not be cut mid-codepoint.
        assert_eq!(truncate("héllo wörld", 6), "héllo\u{2026}");
    }

    #[test]
    fn micro_dollars_round_trip_to_a_readable_amount() {
        assert_eq!(format_micro_usd(1_500_000), format_usd(1.5));
        assert_eq!(format_micro_usd(0), format_usd(0.0));
    }
}
