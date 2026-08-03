# The dashboard

`surface` with no arguments scans and opens a six-view terminal dashboard. This
page is what each view measures, column by column, and every key that moves you
through it.

```
 <■> surface     │
                 │
 1 Overview      │
 2 Tools         │
 3 Sites         │   the view you picked
 4 Usage         │
 5 Cost          │
 6 Projects      │
```

The sidebar down the left is the view list, headed by the mark; the number in
front of each title is the key that jumps to it, and clicking a row does the
same. Below about 90 columns it collapses to just the digits, which stay
clickable — the titles are the first thing worth giving up on a narrow terminal,
and the body needs the room more than the nav does.

## Keys

| Key | Does |
|---|---|
| <kbd>tab</kbd>, <kbd>→</kbd>, <kbd>l</kbd> | Next view |
| <kbd>shift-tab</kbd>, <kbd>←</kbd>, <kbd>h</kbd> | Previous view |
| <kbd>1</kbd>–<kbd>6</kbd> | Jump to a view |
| <kbd>j</kbd>/<kbd>k</kbd>, <kbd>↓</kbd>/<kbd>↑</kbd> | Move the selection |
| <kbd>g</kbd>/<kbd>G</kbd>, <kbd>home</kbd>/<kbd>end</kbd> | First / last row |
| <kbd>page up</kbd>/<kbd>page down</kbd> | Move ten rows |
| Mouse wheel | Move three rows |
| Click a view in the sidebar | Jump to it |
| Click a table row | Select it, and give its pane the keyboard |
| <kbd>enter</kbd> | Switch pane, on a view that has two |
| <kbd>d</kbd> | Token detail line under each row, on and off |
| <kbd>u</kbd> | Switch the Overview chart between spend and tokens |
| <kbd>[</kbd>/<kbd>]</kbd> | Move the chart cursor one bucket |
| <kbd>backspace</kbd> | Drop the chart cursor |
| <kbd>w</kbd> | Regroup the charts by day, week or month |
| <kbd>?</kbd> | Help overlay (any key closes it) |
| <kbd>q</kbd>, <kbd>esc</kbd>, <kbd>ctrl-c</kbd> | Quit |

Two deliberate details: an out-of-range digit does nothing rather than clamping
to the last view, and while the help overlay is open it swallows the next key —
so a stray press dismisses it instead of acting on the view behind it. A click
behaves the same way on all three counts: the empty space under a short table
selects nothing rather than snapping to the last row, a click on a chart or a
card does nothing at all, and a click with the help overlay open only closes it.

## Overview

Five cards, one chart, three rankings — down the page, not across it. Each card
is a headline figure with qualifier lines under it, and a card turns amber when
one of those is a caveat rather than a count.

| Card | Figure | Qualifiers |
|---|---|---|
| **SPEND** | What the seats actually cost, `$X/mo` | Where the figures came from (`N configured seat(s)`, `N plan(s) detected`), the saving against API rates, and `▲ N tool(s) unpriced` if any |
| **TOKEN COST** | The window's tokens at API list rates | `over N days · ≈ $X/day`, the period delta, and `▲ N model(s) unpriced` — or `at API list rates` when none are, because the figure is list-rate arithmetic, not a bill |
| **TOKENS** | Everything billable, cache included | Message count, distinct model count |
| **TOOLS** | AI tools detected | `▲ N autonomous`, vendor count |
| **AI SITES** | Distinct AI domains visited | Visit total, and `▲ N browser(s) unreadable` if any |

SPEND and TOKEN COST are different questions about the same tokens. TOKEN COST
is arithmetic — the window's usage priced per token at list rates. SPEND is
what is actually paid for the seats behind that usage: a figure from
`[cost.subscriptions]` is shown plainly; one from a plan the tool itself names
— its account file first (`~/.claude.json`, `~/.codex/auth.json`), its
transcripts as the fallback — is priced at that plan's list rate and marked
`≈` an estimate; a tool with usage but no figure makes the total `≥` a floor.
Knowing nothing, the card shows `–` and says how to configure it. Nothing is
fetched, nothing is guessed, and the plan's *name* is the only thing read from
those files — never the credentials that sit beside it.

### The rate and the delta

Two derived figures on the TOKEN COST card, and both decline to appear rather
than mislead:

- **`≈ $X/day`** divides the total by the window. Per day rather than projected to
  a month, because the default window *is* a month — a monthly projection came out
  equal to the total and read like a bug. It divides by the *configured* window,
  not by the days that saw usage, so three days of history in a thirty-day window
  reads low. That is the same span the line beside it already claims.
- **`▲ 42% vs prior 15 days`** compares the window's second half against its
  first, split on the date halfway between the first and last active day — not on
  the row index, since days with no usage are simply absent from the series.
  Below six active days it is not shown at all: a half-window delta over three
  days swings on single days.

### One chart, two units

Below the cards, **one** chart at the full width of the page, stacked by model.
<kbd>u</kbd> switches it between **estimated spend** and **tokens**, and the title
names the unit the key would give you.

It used to be two charts, one per unit, stacked in a column half this wide. They
were the same data: same buckets, same series, and — because cost is roughly
proportional to tokens within a model — the same silhouette, with the legend
printed underneath both. One chart at full width has room for its bars and needs
its legend once.

The spend title carries `▲ a floor` while any model is unpriced, and with no price
table at all the chart is replaced by a one-line explanation rather than a flat
`$0` series, which would read as free. In that state the title says `[u] for
tokens`, since tokens are still chartable.

### Three rankings

Under the chart, the three axes the scan knows, ranked side by side and equally
wide: **by model**, **by tool**, **by repository**. None of them is the answer on
its own — a model is expensive, a tool ran it, a repository asked for it, and
which of those you can act on depends on why you opened the dashboard.

Each names its top five and folds the rest into `+ N more` carrying their summed
spend, so the panel still accounts for the whole total. A figure shows `≥` when an
unpriced model sits under it. `(unattributed)` is an ordinary row in the
repository ranking, for work outside a git repository with an `origin` remote.

The repository ranking used to be the whole right-hand column and took every row
it was given, so a sixty-row terminal listed fifty repositories of which the last
twenty had spent under a dollar. The [Projects](#projects) view is where the full
list lives.

The view sheds bands as the terminal shrinks, and always in the same order: the
rankings go first, then the cards, leaving the chart — which is the one thing the
page cannot be without.

## Tools

One row per detected tool.

| Column | Meaning |
|---|---|
| **TOOL** | Product name |
| **VENDOR** | Who ships it |
| **KIND** | `assistant`, `coding agent`, `autonomous agent`, `editor`, `extension`, `local runtime` |
| **CAN ACT** | Whether it can execute code or take actions on this machine |
| **PLAN** | The subscription plan the tool is signed into, as the raw slug it wrote (`default_claude_max_5x`, `team`) — from its account file, or its transcripts as the fallback. `–` when it names none |
| **$/MO** | What that seat costs per month: plain from `[cost.subscriptions]`, `≈` at the plan's list price. The `≈` is the invitation — a figure that looks wrong is fixed with one config line |
| **FOUND BY** | The evidence that produced the detection |

**CAN ACT** is the column worth reading first. A chat window that can only talk
is not the same exposure as an agent holding a shell, and it is flagged with a
glyph *and* a word so the distinction survives a monochrome terminal.

**FOUND BY** distinguishes a leftover config directory from a running process.
Detection never executes anything — see [Detection, not
judgement](../getting-started/concepts.md#detection-not-judgement) for the five
channels.

## Sites

One row per AI domain, from browser history.

| Column | Meaning |
|---|---|
| **DOMAIN** | The registrable host, e.g. `chatgpt.com` |
| **VENDOR** | Who runs it |
| **KIND** | `assistant`, `agent`, `modelapi`, `devtool`, `modelhub` |
| **VISITS** | The browser's **lifetime** visit count, with a bar |
| **LAST SEEN** | Date of the most recent visit, or `unknown` |

The panel title carries the scan's own scope: how many domains, how many browser
profiles were read, the lookback window, and — when relevant —
`▲ unreadable: Safari`.

!!! note "Visits are lifetime, the lookback only filters"
    `history_lookback_days` decides *which* domains are listed: a domain must
    have been visited that recently to appear. The count next to it is still the
    browser's own lifetime total, not a count within the window.

Two states this view reports rather than faking:

- **Switched off** — `scan_history = false`, so it says so instead of showing an
  empty list.
- **Not compiled in** — the binary was built without the `sqlite` feature, which
  browser history needs. See
  [Installation](../getting-started/installation.md#without-sqlite).

## Usage

A stacked token chart over a per-model table. One row per `(tool, model)` pair
within the window.

| Column | Meaning |
|---|---|
| **TOOL** | `claude_code`, `codex`, `opencode`, … |
| **MODEL** | Model id as the tool recorded it |
| **INPUT** | Prompt tokens |
| **OUTPUT** | Completion tokens |
| **CACHE READ** | Tokens served from cache, billed at cache rates |
| **MESSAGES** | Records counted, after deduplication |
| **TOTAL** | Everything billable added up, with a bar |

Every kind of token is on the detail line under each row, which is what
<kbd>d</kbd> toggles:

```
● claude-sonnet-4-5  claude_code                    (20.6%)   1,581      297M ████
  In: 62.4M · Out: 6.6M · CR: 198M · CW: 23.2M · R: 7.2M
```

`CW` is cache creation and `R` is reasoning. Both are billed — reasoning at the
output rate — and both are inside **TOTAL**, so before they had a line of their
own the visible columns did not add up to the total beside them.

**The chart above this table is stacked by model, not by tool, and a model is the
same colour in both.** That is the point of keying them the same way: a segment in
the chart and a row in the table are visibly one thing. The swatch beside each
row is the very swatch the legend gives that model.

Colours come from the brand's blue ramp, refitted for a dark terminal. As given it
is ten steps of one hue for a light page, so each step keeps its hue and its
saturation and only its lightness is re-solved, spread across 4.5:1–13:1 — against
a dark background three of the originals measure 1.0–1.2:1, which is invisible.

A single-hue ramp is the right shape here for two reasons that are easy to miss.
These series are **ranked** — slot one is the largest spender — so a sequential
scale encodes rank as well as identity, which is what a sequential scale is for.
And it is the *more* robust choice for colour-vision deficiency than a set of
distinct hues, because every form of dichromacy preserves lightness and none
preserves hue.

The slot order is deliberately **not** the ramp's order. Stacked segments touch,
and slots stack in series order, so a ramp laid out as a gradient would put every
bar's least distinguishable pair against each other. Interleaving the light and
dark halves alternates a vivid blue with a near-grey at each boundary, so adjacent
segments differ in lightness *and* chroma.

Six models are named; everything past them folds into **other**, drawn in a
neutral grey from the two-tone ramp rather than from the model palette, so a
fold-together bucket cannot be mistaken for one more model. The six are the six
largest by tokens, because this list is also the chart's legend.

Elsewhere colour and texture divide the work: a **colour** always means a model,
and a **texture** — `█ ▓ ▒ ░` — always means a tool. The Overview and Projects
charts stack by tool and so use textures; the sessions pane marks each row with
its tool's texture. Nothing is ever both.

With <kbd>d</kbd> off, the table goes back to one line per row with `INPUT`,
`OUTPUT` and `CACHE READ` as columns — fewer figures, twice as many rows on
screen.

## Cost

The same rows, priced: a spend chart, the per-model cost table, and a
subscription comparison when one is configured.

| Column | Meaning |
|---|---|
| **TOOL** | Tool id |
| **MODEL** | Model id |
| **TOKENS** | Total billable tokens |
| **COST** | `$12.40`, `local`, or `▲ unpriced` — followed by a bar |

The COST cell carries the *state*, not just a number, so `unpriced` can never be
mistaken for free. The panel title carries the same warning at the top level:

```
┌ $1,284.02 over 30 days · ▲ a floor: 2 model(s) have no price ─┐
```

It also says `built-in price table` when the rates came from the table compiled
into the binary, because those are as old as the release. And when there are no
prices at all — a first run with `--offline` and no cache — the view says *"No
model prices are available, so nothing can be costed"* rather than rendering a
column of `$0.00`.

### Subscription comparison

If you told surface what you actually pay, it compares that against what the
same tokens would have cost at API rates:

| Column | Meaning |
|---|---|
| **TOOL** | Tool id |
| **SUBSCRIPTION** | Your monthly cost, suffixed `est` if it came from a list price |
| **SAME AT API RATES** | What that usage would have cost per token |
| | `subscription saves $240.00`, or `▲ API would be $88.00 cheaper` |

A tool with no `[cost.subscriptions]` entry and no plan named in its own
transcripts gets no row: surface reads no account state, so a plan it is not
told about — by you or by the transcript — is never guessed. See
[Configuration](configuration.md#cost) and [Costs](costs.md).

## Projects

One row per repository, most expensive first. Where the Overview's *spend by
repository* panel is a ranking, this view is the whole picture: the table plus a
chart of whichever project the selection is on, so <kbd>j</kbd>/<kbd>k</kbd>
walks projects and the chart follows.

| Column | Meaning |
|---|---|
| **PROJECT** | Repository slug, or `(unattributed)` |
| **TOKENS** | Total billable tokens in the window |
| **MESSAGES** | Records counted, after deduplication |
| **COST** | `$12.40`, or `≥$12.40` when unpriced models sit under it |
| **LAST USED** | Last day this project saw any usage |
| **TOOLS** | The tools whose sessions ran here, each behind its series texture |

```
┌ acme/platform · 30 buckets · 568M total · peak 67.4M · [w] regroup ──────────┐
│ 67.4M ····························································░░·······  │
│                                                                    ░░        │
│ 33.7M ······░░···················░░·························▒▒▒▒▒··░░·······  │
│       ░░░░░░▒▒░░░░░░░░░░░░░░░░░░  ▒▒▒▒▒▒▒▒▓▓▓▓▓▓▓▓▓▓▓▓  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓       │
│     0 ████████████████████████████████████████████████████████████████       │
│       2026-06-29  2026-07-05  2026-07-11  2026-07-17  2026-07-23             │
│       █ claude_code  ▓ codex  ▒ ollama  ░ opencode                           │
└──────────────────────────────────────────────────────────────────────────────┘
┌ 16 projects over 30 days · ▲ 1 ≥ a floor ───┐┌ 72 sessions in this project ══╗
│PROJECT         TOKENS COST     TOOLS        ││● invoice rounding bug   $56.17│
│                                             ││  claude_code · 2026-07-28 · 3…│
│acme/platform   568M   $628.71  █ claude_cod…││● flaky checkout test    $32.80│
│acme/webapp     200M   $268.07  ░ opencode   ││  opencode · 2026-07-27 · 34.8…│
│acme/mobile-app 123M   $183.33  ▓ codex      ││● cache keys collide u…  $30.22│
│(unattributed)  125M   $150.56  █ claude_cod…││  claude_code · 2026-07-26 · 1…│
└─────────────────────────────────────────────┘╚═══════════════════════════════╝
```

Attribution is by the working directory a session ran in, resolved to its git
`origin` slug — never a path, and never a branch. Work outside a repository lands
in `(unattributed)`, which is a row like any other rather than a discard — see
[Repository
attribution](../getting-started/concepts.md#repository-attribution).

The chart carries one series, the project itself: a repository runs more models
than there are series slots, and a repeated texture would claim two models were
the same one. It spans every day *any* project was active, so an idle stretch is a run
of zero columns rather than a gap that squeezes two distant bars together.

### Sessions in the selected project

Beside the table, a breakdown of the project the selection is on — because a
repository total answers "how much" and not "doing what". A session is the unit a
person actually recognises: one run of one tool in one checkout.

| Field | Meaning |
|---|---|
| **SESSION** | The session's title where the tool records one, otherwise a short form of its key |
| **COST** | `$1.40`, or `≥$1.40` when unpriced models sit under it |
| detail line | The tool, the last day it ran, total tokens, message count, and every model it used |

```
┌ 16 projects over 30 days · ▲ 1 ≥ a floor ───┐┌ 72 sessions in this project ══╗
│PROJECT              TOKENS     COST         ││● invoice rounding bug   $56.17│
│                                             ││  claude_code · 2026-07-28 · 3…│
│acme/platform        568M       $628.71  ████││● flaky checkout test    $32.80│
│acme/webapp          200M       $268.07  ██  ││  opencode · 2026-07-27 · 34.8…│
└─────────────────────────────────────────────┘╚═══════════════════════════════╝
                                                ^ brighter border = has the keys
```

It scrolls, and it is selectable in its own right — a busy repository has
hundreds of sessions and all of them are reachable. <kbd>enter</kbd> moves the
keyboard between the two panes and the focused one brightens its border;
<kbd>j</kbd>/<kbd>k</kbd> then walk whichever has it. A click selects a row *and*
gives that pane the keyboard. The wheel scrolls whichever pane the pointer is
over, without moving focus — glancing at a list should not take the keyboard away
from the one you were working in.

Moving the project cursor rebuilds the breakdown, so the session cursor goes back
to the top rather than being clamped: row 3 of one repository's sessions is a
different session in another's, and the list is sorted costliest-first.

The detail line is what makes the pane fit beside the table. It is cut from the
right, so what it is written in is a priority order — the tool and the day survive
a narrow pane, and the model list, which the [Usage](#usage) view lists in full
anyway, is the first thing to go.

Below about 96 columns of body the breakdown is not drawn at all and the ranking
takes the whole width, keeping every column it has. Width is the constraint, not
height: a pane beside the table costs no rows, so a short terminal still shows it.

Session keys are one-way hashes, the same ones the ledger stores. They identify a
session across scans without being able to reach the transcript it came from —
see [Privacy](privacy.md). Titles appear only where the tool records one and
title collection is enabled.

A machine with no repository-attributed usage gets an explanation instead of an
empty table.

## Charts

Every chart is stacked by series, with a scale gutter and a legend carrying
per-series totals. Two properties they hold to:

- **Every chart with a palette is keyed by model.** Overview, Usage and Cost all
  stack by model and use the same colours, so a model is one colour wherever you
  meet it. Projects is the exception: it charts the selected repository, which is
  a single series.

    On the Overview this costs rows. Naming up to seven models takes a legend of
    two or three lines out of the plot, so the two charts appear only when the
    column can afford both — below that you get the token chart at full height
    rather than two charts too short to read. The spend figure is on a card above
    it either way, and the Cost view charts spend in full.

    In a panel under about 70 columns the legend cuts the model names to fit and
    keeps the totals, on the grounds that a shortened name is still a hint and a
    missing figure is nothing. Read the full names in the Usage table.

- **Texture means tool; colour means model.** The dashboard itself is two colours —
  Ink and Paper, the same ramp as this site — so tools are told apart by texture
  rather than hue: `█ ▓ ▒ ░`, heaviest at the bottom of a stack. Each one's grey
  compensates its own density, because `░` inks a quarter of its cell and in the
  same grey as `█` would read a quarter as strongly. Only the Projects chart,
  its table's TOOLS column and the sessions pane use textures now; every chart
  with a palette is keyed by model.

    Models get real colour, from the brand's blue ramp, because a model is the one
    thing a reader needs to trace between a chart and a table. Six are named and
    the rest fold into a neutral **other**. See [Usage](#usage) for how the ramp is
    refitted and why its slots are interleaved rather than laid out as a
    gradient.

- **A series keeps its slot** between scans, assigned by identity in a stable
  order rather than by rank — except the model list, which is ranked by tokens
  precisely because it doubles as the chart's legend.

- Amber and green stay the only other hues, for warnings and for money.
- **Regrouping never changes the total.** <kbd>w</kbd> cycles day → week →
  month; weeks are ISO weeks, so a week never straddles two labels. There is a
  test whose whole job is that the grand total survives every granularity.

## The footer

```
 scanned in 395ms · read 12.4 KB                     [tab] view  [jk] move  [w] regroup  [?] help  [q] quit
```

Scan duration, bytes read from transcripts, and the keys. Two things appear only
when they happen:

- **`FAILED: usage`** — that section panicked and was skipped. The rest of the
  scan still ran; this is the failure isolation described in [Core
  concepts](../getting-started/concepts.md#the-scan).
- **`ledger not saved`** — the ledger could not be written. Costs incrementality
  on the next run, not correctness now.

A transient status line (`grouped by week`) replaces the footer until the next
keypress.
