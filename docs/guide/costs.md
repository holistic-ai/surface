# Costs

Where the money figures come from, and — more importantly — the four ways they
can be wrong. A cost dashboard that does not tell you its own error bars is worse
than no dashboard.

## Where the rates come from

[LiteLLM's public price table](https://github.com/BerriAI/litellm), fetched once
a day at most and cached in the state directory. Three sources, in order:

1. The **cached table**, if it is less than 24 hours old.
2. The **built-in table** compiled into the binary — so a first run with no
   network still costs correctly instead of showing an empty view. The Cost panel
   says `built-in price table` when this is what you are looking at, because
   those rates are as old as the release.
3. The **network**, when the cache is stale and `--offline` was not passed.

A stale cache beats no prices, and the built-in table beats both when neither is
available.

## Prices are applied when read, not when counted

The scan records token counts and model ids; nothing else. Pricing happens at
read time, which has one consequence worth knowing: **a newer price table
re-prices your whole history**. Rates moved last week? The next scan reflects
that across every day in the window, retroactively, for free.

The alternative — baking a rate in at count time — is wrong the moment prices
move, with no way to correct it afterwards.

## Unpriced is not free

A model missing from the price table shows as `▲ unpriced`, never `$0.00`, and a
total with unpriced models beneath it is marked `≥` because it is a floor rather
than a figure. The Cost panel spells it out:

```
┌ $1,284.02 over 30 days · ▲ a floor: 2 model(s) have no price ─┐
```

In `--json`, an unpriced cost is `{"state": "unpriced", "usd": null}`. It never
serialises as `0.0` — a total that silently omits a third of the usage is worse
than no total, and this is the one arithmetic lie surface could plausibly tell.
There is a test whose only job is to make sure it does not.

Unpriced usually means a model too new for the table, a provider-specific alias,
or a model id the tool wrote in a form the table does not use. Running once
without `--offline` fixes the first case.

## Local models are free, and say so

A locally-run model shows as `local`, not `$0.00`. Both would compute to zero,
but they mean different things: one genuinely costs nothing, the other costs an
unknown amount. Collapsing them would destroy the only distinction that matters.

## These are API list rates

surface prices tokens as if you paid per token at list price. If you are on a
flat subscription, that number answers *"what would this usage have cost on the
API"* — which is a useful question, but not your bill.

Put what you actually pay in `[cost.subscriptions]` and the Cost view compares
the two:

```toml
[cost.subscriptions]
claude_code = 100.0
```

```
┌ subscription vs pay-per-token ─────────────────────────────────────┐
│TOOL          SUBSCRIPTION  SAME AT API RATES                       │
│claude_code   $100.00       $340.12          subscription saves $240 │
└────────────────────────────────────────────────────────────────────┘
```

Without an entry, a tool gets no subscription row. surface reads no account state
— no plan, no billing API, no session token — so it cannot know what you pay and
will not guess. Where a figure comes from a published list price rather than your
config, it is suffixed `est`.

## How each token kind is billed

| Kind | Billed at |
|---|---|
| `input` | Input rate |
| `output` | Output rate |
| `cache_read` | Cache-read rate |
| `cache_creation` | Cache-write rate |
| `reasoning` | Output rate |

Cache reads at cache rates and reasoning at output rates is how the providers
that distinguish them do it. This is also why the six token counters are never
merged before pricing: one blended number cannot be priced correctly.

## The four ways to be wrong

A summary, because these are the ones to check before quoting a figure at anyone:

1. **`≥` on a total** — unpriced models underneath it. The real number is higher.
2. **`built-in price table`** — the rates are as old as the binary. Run once
   online.
3. **`(unattributed)` is large** — the spend is real, but not tied to a
   repository, so per-repo ranking under-reports.
4. **You are on a subscription** — the figure is an API-equivalent, not a bill,
   until you configure `[cost.subscriptions]`.

## See also

- [Configuration](configuration.md#cost) — the `[cost]` table.
- [The dashboard](dashboard.md#cost) — every column in the Cost view.
- [Core concepts](../getting-started/concepts.md#prices-at-read-time) — why
  pricing is a read-time operation.
