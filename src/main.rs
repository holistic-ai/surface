//! surface — what AI runs here, and what it costs.
//!
//! A local scan of four things: the AI tools installed on this machine, the AI
//! sites this machine visits, the tokens those tools have spent, and what that
//! costs. Nothing is transmitted; the only network call is an optional model
//! price table, which `--offline` disables.

mod app;
#[cfg(feature = "sqlite")]
mod browser;
mod config;
mod demo;
mod format;
mod ledger;
mod paths;
mod pricing;
mod reason;
mod repo;
mod scan;
mod ui;

use std::io::{stdout, Stdout};
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, Tab};

#[derive(Parser, Debug)]
#[command(
    name = "surface",
    version,
    about = "What AI runs here, and what it costs.",
    long_about = None
)]
struct Cli {
    /// Print the scan as JSON and exit, instead of opening the dashboard.
    #[arg(long)]
    json: bool,

    /// Never fetch model prices; use the cache, then the built-in table.
    #[arg(long)]
    offline: bool,

    /// Print resolved paths and settings, then exit without scanning.
    #[arg(long)]
    check: bool,

    /// Open the dashboard on mock data, without scanning this machine.
    ///
    /// For demonstrations and bug reports: the tool names and domains are real,
    /// the counts are invented, and the dashboard says `DEMO` while it is on.
    #[arg(long)]
    demo: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let paths = paths::Paths::resolve()?;
    paths.ensure()?;
    let config = config::Config::load(&paths.config_file())?;

    if cli.check {
        return check(&paths, &config, cli.offline);
    }

    let prices = pricing::Prices::load(&paths.state_dir, !cli.offline);

    // The demo reads nothing: no transcripts, no history, no ledger. It is
    // still priced by the real table, so its dollar figures are what its
    // invented token counts would genuinely have cost.
    let (scan, timings) = if cli.demo {
        demo::scan()
    } else {
        scan::run(&config, &paths.state_dir)
    };

    if cli.json {
        return print_json(&scan, &timings, &prices);
    }

    let app = App::new(scan, timings, prices, config.cost);
    let mut terminal = setup()?;
    let result = run(&mut terminal, app);
    // Restore even if the loop failed, or the shell is left in raw mode.
    restore(&mut terminal)?;
    result
}

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn setup() -> Result<Tui> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

fn restore(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Poll rather than block, so a resize repaints without a keypress.
const TICK: Duration = Duration::from_millis(250);

/// Lines moved per wheel notch.
const WHEEL: i32 = 3;

fn run(terminal: &mut Tui, mut app: App) -> Result<()> {
    // Where the last frame put the tab titles and the table rows. The renderer
    // hands this back so a click is resolved against the geometry that was
    // actually drawn, including how far the table happened to be scrolled.
    let mut hits = ui::Hits::default();

    while !app.should_quit {
        terminal.draw(|frame| hits = ui::draw(frame, &app))?;

        if !event::poll(TICK)? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(&mut app, key),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    scroll(&mut app, &hits, WHEEL, mouse.column, mouse.row);
                }
                MouseEventKind::ScrollUp => {
                    scroll(&mut app, &hits, -WHEEL, mouse.column, mouse.row);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    handle_click(&mut app, &hits, mouse.column, mouse.row);
                }
                _ => {}
            },
            // Resize needs no handling: the next draw uses the new size.
            _ => {}
        }
    }
    Ok(())
}

/// A click on a tab title switches view; a click on a table row selects it.
///
/// Anything else is ignored on purpose — a click on a chart, a card or the
/// footer has no meaning, and guessing at one would move the selection out from
/// under the reader for no reason they could see.
fn handle_click(app: &mut App, hits: &ui::Hits, column: u16, row: u16) {
    // A status line is transient, the same as it is for a keypress.
    app.status_line = None;

    // Help swallows the click that dismisses it, exactly as it swallows a key,
    // so a click meant for the view behind it never lands twice.
    if app.show_help {
        app.show_help = false;
        return;
    }

    if let Some(tab) = hits.tab_at(column, row) {
        app.set_tab(tab);
        return;
    }

    // The sessions pane first: it sits inside the body, so asking the main table
    // first would let the wider band answer for a click that was not in it.
    if let Some(index) = hits.session_row_at(column, row) {
        app.select_session_row(index);
        return;
    }

    if let Some(index) = hits.row_at(column, row) {
        app.select_row(index);
    }
}

/// The wheel scrolls the pane under the pointer, falling back to the focused one.
///
/// Pointer-targeted because the click already is, and two mouse gestures on one
/// frame that disagree about what they act on read as a bug. Wheeling a pane does
/// *not* move focus, though clicking does — a glance at a list should not take
/// the keyboard away from the one you were working in.
fn scroll(app: &mut App, hits: &ui::Hits, delta: i32, column: u16, row: u16) {
    if app.show_help {
        app.show_help = false;
        return;
    }
    if hits.over_sessions(column, row) {
        app.scroll_pane(app::Pane::Sessions, delta);
        return;
    }
    app.scroll(delta);
}

fn handle_key(app: &mut App, key: event::KeyEvent) {
    // A status line is transient: any key clears it.
    app.status_line = None;

    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        app.should_quit = true;
        return;
    }

    // While help is open it swallows the next key, so a stray press dismisses it
    // rather than acting on the view behind it.
    if app.show_help {
        app.show_help = false;
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('w') => app.cycle_granularity(),
        KeyCode::Char('d') => app.toggle_detail(),
        KeyCode::Char('u') => app.toggle_unit(),
        KeyCode::Enter => app.toggle_focus(),
        KeyCode::Char('[') => app.move_bucket(1),
        KeyCode::Char(']') => app.move_bucket(-1),
        KeyCode::Backspace => app.clear_bucket(),
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.next_tab(),
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.prev_tab(),
        KeyCode::Char('j') | KeyCode::Down => app.next_row(),
        KeyCode::Char('k') | KeyCode::Up => app.prev_row(),
        KeyCode::Char('g') | KeyCode::Home => app.first_row(),
        KeyCode::Char('G') | KeyCode::End => app.last_row(),
        KeyCode::PageDown => app.scroll(10),
        KeyCode::PageUp => app.scroll(-10),
        // Out-of-range digits are ignored rather than clamped: pressing 9 with
        // six views should do nothing, not jump to the last one.
        KeyCode::Char(c @ '1'..='9') => {
            let index = c as usize - '1' as usize;
            if let Some(tab) = Tab::ALL.get(index) {
                app.set_tab(*tab);
            }
        }
        _ => {}
    }
}

fn check(paths: &paths::Paths, config: &config::Config, offline: bool) -> Result<()> {
    let prices = pricing::Prices::load(&paths.state_dir, !offline);

    println!("state dir   {}", paths.state_dir.display());
    println!("config dir  {}", paths.config_dir.display());
    println!(
        "config      {}",
        if paths.config_file().exists() {
            "loaded"
        } else {
            "defaults (no file)"
        }
    );
    println!(
        "ledger      {}",
        ledger::ledger_path(&paths.state_dir).display()
    );
    println!(
        "prices      {} models{}",
        prices.len(),
        if prices.is_builtin() {
            " (built in; no cache, no network)".to_string()
        } else {
            match prices.age() {
                Some(age) => format!(", cached {} ago", format::human_duration(age.as_secs())),
                None => String::new(),
            }
        }
    );
    println!(
        "sites       {}",
        if !scan::Scan::sites_compiled_in() {
            "not compiled in (built without the sqlite feature)".to_string()
        } else if config.web.scan_history {
            format!("{} day lookback", config.web.history_lookback_days)
        } else {
            "disabled in config".to_string()
        }
    );
    println!(
        "usage       {}",
        if config.usage.scan {
            format!("{} day window", config.usage.window_days)
        } else {
            "disabled in config".to_string()
        }
    );
    Ok(())
}

/// The scan as JSON.
///
/// Assembled here rather than derived on [`scan::Scan`], because the shape a
/// person wants to pipe into `jq` is not the shape the views want to read.
fn print_json(scan: &scan::Scan, timings: &scan::Timings, prices: &pricing::Prices) -> Result<()> {
    use serde_json::json;

    let ledger = &scan.usage.ledger;

    let days: Vec<_> = ledger
        .rows()
        .into_iter()
        .map(|(day, tool, model, t)| {
            json!({
                "date": day,
                "tool": tool,
                "model": model,
                "input": t.input,
                "output": t.output,
                "cache_read": t.cache_read,
                "cache_creation": t.cache_creation,
                "reasoning": t.reasoning,
                "messages": t.messages,
                "cost": cost_json(&prices.cost(&model, &t)),
            })
        })
        .collect();

    let by_repo: Vec<_> = ledger
        .by_project()
        .into_iter()
        .map(|(repo, models)| {
            let mut total = ledger::Tokens::default();
            let mut usd = 0.0;
            let mut unpriced = 0usize;
            for (model, t) in &models {
                total.add(t);
                let cost = prices.cost(model, t);
                usd += cost.usd();
                if cost.is_unpriced() {
                    unpriced += 1;
                }
            }
            json!({
                "repo": repo,
                "tokens": total.total(),
                "messages": total.messages,
                "cost_usd": usd,
                // A total with unpriced models under it is a floor, not a figure.
                "unpriced_models": unpriced,
            })
        })
        .collect();

    let payload = json!({
        "surface": env!("CARGO_PKG_VERSION"),
        // Present and false on a real scan, so a consumer can tell the
        // difference without having to know the flag exists.
        "demo": scan.demo,
        "timings_ms": timings,
        "failed_sections": scan.failed,
        "tools": {
            "detected": scan.tools_summary.detected,
            "autonomous": scan.tools_summary.autonomous,
            "vendors": scan.tools_summary.vendors,
            "list": scan.tools.iter().map(|d| json!({
                "id": d.tool.id,
                "name": d.tool.name,
                "vendor": d.tool.vendor,
                "kind": d.tool.kind,
                "autonomous": d.tool.autonomous,
                "evidence": d.evidence,
            })).collect::<Vec<_>>(),
        },
        "sites": sites_json(scan),
        "usage": {
            "window_days": scan.usage.window_days,
            "disabled": scan.usage.disabled,
            "tools_read": scan.usage.tools_read,
            "sources_read": scan.usage.sources_read,
            "bytes_read": scan.usage.bytes_read,
            "unreadable": scan.usage.unreadable,
            "duplicates_skipped": ledger.duplicates_skipped,
            "totals_by_tool": ledger.totals_by_tool().into_iter().map(|(tool, t)| (tool, json!({
                "input": t.input,
                "output": t.output,
                "cache_read": t.cache_read,
                "cache_creation": t.cache_creation,
                "reasoning": t.reasoning,
                "messages": t.messages,
                "total": t.total(),
            }))).collect::<serde_json::Map<_, _>>(),
            "days": days,
            "by_repo": by_repo,
        },
        "prices": {
            "models": prices.len(),
            "age_secs": prices.age().map(|a| a.as_secs()),
        },
    });

    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

/// Cost as three distinguishable states rather than a number.
///
/// `Unpriced` must not serialise as `0.0`: a model missing from the table costs
/// an unknown amount, and rendering that as free is the one arithmetic lie this
/// tool could plausibly tell.
fn cost_json(cost: &pricing::Cost) -> serde_json::Value {
    use serde_json::json;
    match cost {
        pricing::Cost::Known(usd) => json!({"state": "known", "usd": usd}),
        pricing::Cost::Local => json!({"state": "local", "usd": 0.0}),
        pricing::Cost::Unpriced => json!({"state": "unpriced", "usd": null}),
    }
}

#[cfg(feature = "sqlite")]
fn sites_json(scan: &scan::Scan) -> serde_json::Value {
    serde_json::to_value(&scan.sites).unwrap_or(serde_json::Value::Null)
}

#[cfg(not(feature = "sqlite"))]
fn sites_json(_scan: &scan::Scan) -> serde_json::Value {
    serde_json::json!({
        "compiled_in": false,
        "reason": crate::reason::TOOL_UNAVAILABLE,
        "detail": "built without the sqlite feature",
    })
}
