# Memecoin Trading Bot — Research & Architecture

**Date:** 2026-08-27
**Status:** Research complete. No production trading code in this phase.
**Verdict:** **BUILD WITH CONDITIONS**

This document evaluates whether an automated system for finding and trading young memecoins can have positive expected value, and if so, how it should be built. It does not claim that any strategy is currently profitable. Numbers below are sourced from public 2026 data (Dune dashboards, launchpad analytics, academic papers, vendor docs). Where a claim is estimated rather than measured, it is labeled as such.

---

## 1. Executive summary

The memecoin market in 2026 is still large, still adversarial, and still dominated by Solana. It is not a market where “buy early, sell later” has a free lunch. The base rates are brutal:

- Pump.fun still issues tokens at industrial scale. A 39-day corpus covering **every** Pump.fun launch from 2026-06-05 to 2026-07-14 recorded **798,430 launches** and **5,689 graduations** — a **0.71%** graduation rate ([HuggingFace PumpFun Launch-to-Graduation Corpus](https://huggingface.co/datasets/Slinky21/Pumpfun_Memecoin_Corpus)). A Kaplan–Meier study of **832,941** launches from 2026-05-08 to 2026-06-10 found a **0.198%** 24-hour graduation rate ([arXiv:2607.02823](https://arxiv.org/html/2607.02823v3)).
- Among tokens that *do* migrate to a DEX, a Georgia Tech dataset (MELT) of **41,470** completed Pump.fun → DEX launches found **84.13% high-risk**. About **73%** of migrated tokens fell below **40% of migration price within 20 minutes**. Coordinated “bundle” accounts held **36.5% of supply** on average at migration ([arXiv:2602.13480](https://arxiv.org/abs/2602.13480)).
- A 7-month, 6.4-million-token Solana study found that a vast majority of memecoins exhibit rug-pull characteristics **within one hour of launch**, and that **XGBoost on the first 5 minutes of trading** can detect many of them ([arXiv:2608.20271](https://arxiv.org/abs/2608.20271)).
- Existing retail terminals (Axiom, Fomo, GMGN, Trojan, Photon remnants) already own discovery UX, copy-trading, and execution. Building another Telegram sniper is not an edge.
- Ultra-early sniping is structurally unattractive for a new team: Jito tip auctions, bundled insider supply, and launchpad anti-snipe mechanics have compressed sniper margins since 2024–2025.

What *is* researchable, and potentially valuable:

1. **Avoiding the left tail is more important than catching the right tail.** Academic work shows risk is detectable from early on-chain behavior. MELT’s risk model, used as a filter, reduced simulated post-migration losses by up to **34 percentage points** versus random selection.
2. **Confirmation after demand appears is more plausible than block-zero sniping.** Latency competition at creation is a capital-and-colocation arms race we should not enter in V1.
3. **Wallet intelligence and bundle detection are the most defensible proprietary signals**, because they require longitudinal data most retail terminals do not store for research.
4. **The system’s first job is measurement, not trading.** We can reconstruct token state at time T and measure T+10s … T+1h outcomes. Until that loop shows expectancy greater than fees, slippage, and failed-tx costs, no live capital should be deployed.

**Recommended V1 chain:** Solana.
**Recommended V1 strategy family:** confirmation momentum on Pump.fun / PumpSwap tokens that already show organic demand, gated by a hard risk filter, with aggressive exits. Not sniping.
**Recommended first action:** a historical experiment on the PumpFun corpus plus a 2–4 week live paper-trading collector. Live trading is a later phase, gated by statistics.

---

## 2. Is the idea viable?

**Viable as a research system: yes.**
**Viable as a profitable trading business: unknown, and currently unproven for us.**
**Viable as “snipe every new coin”: almost certainly no.**

### 2.1 What would have to be true

Positive expected value requires all of the following at once:

1. A filter that rejects most rugs/honeypots/bundles **before** we pay the spread.
2. At least one signal whose conditional return, after realistic costs, is positive out of sample.
3. An exit process that does not give back the entire winner on the next dump.
4. Position sizes small enough that we can actually exit into the available liquidity.
5. A trade frequency high enough that expectancy is not luck on 20 trades, but low enough that we are not paying Jito/priority/DEX fees on noise.

If any one of those fails, the system loses money even if win rate looks decent.

### 2.2 Why the naive version fails

The naive pipeline — detect creation, buy immediately, sell into the first pump — is the product that Axiom, Trojan, Banana Gun, and a thousand open-source repos already sell. Public evidence:

- Graduation is rare (0.2–0.7% depending on window).
- Most migrated tokens dump hard in minutes (MELT).
- Insiders systematically accumulate on the curve and unwind after migration. 98.7% of MELT create events co-occur with a developer buy in the same transaction.
- Sniper-user loss rates of 82–90% are commonly cited in 2026 industry writeups. Treat those as marketing-adjacent, but they are directionally consistent with the on-chain base rates.
- Competitive Jito tips on contested launches moved from ~0.001–0.005 SOL (2023) to ~0.008–0.04 SOL by mid-2025, and peak-hour 90%+ landing tips in 2026 are often 0.005–0.015 SOL. On a 0.1–0.3 SOL entry, the tip can be a large fraction of expected gross profit.

### 2.3 Why a *research-first* version might still have EV

The same papers that document how bad the market is also document that **the badness is structured**:

- High-risk launches have shorter curves, fewer holders, larger average buys, and more bundled supply.
- First-5-minute trading data has predictive content for rugs.
- Presence of Telegram (and especially all three socials) is associated with a large lift in graduation probability in one 2026 survival study (0.110% with no socials vs 1.485% with Telegram vs 1.919% with all three — association, not a trading signal).
- Risk filters can cut losses even when they cannot pick winners.

That is enough to justify building a **measurement and paper-trading system**. It is not enough to justify a live bot with meaningful capital.

### 2.4 Sniping vs slightly more mature tokens

| Universe | Typical age | Liquidity | Rug / dump risk | Competition | Researchability |
|---|---|---|---|---|---|
| Creation / first seconds | 0–10s | Tiny, bonding curve | Extreme; bundled supply; sniper wars | Highest | Poor: we cannot beat colocated searchers |
| Early curve confirmation | 30s–10 min | Still thin | High but *filterable* | High | Good: 15s snapshots exist |
| Near-graduation / graduation | minutes–hours | LP burned into PumpSwap | Dump-after-migrate is the modal outcome | Medium-high | Best discrete event |
| Post-graduation 15 min–few hours | minutes–hours | Real AMM, still small | Still high; 73% crash in 20 min | Medium | Good if we wait for demand |
| “Mature” memes (hours–days, $200k–$2M) | hours+ | Better exits | Lower structural rug, more P&D / narrative | Lower latency race, more copy-trade crowding | Good for later experiments |

**Conclusion:** trading *slightly more mature* tokens (confirmed curve demand, or post-graduation with surviving liquidity and holder growth) is statistically safer than sniping. Whether it is more profitable is an empirical question. Safety is not the same as EV: waiting reduces rugs but also reduces the left tail of *wins*. V1 should optimize **risk-adjusted expectancy after costs**, not “earliest possible fill.”

---

## 3. Recommended blockchain

**V1 chain: Solana.**
Not because it is fashionable. Because it is the only chain where all of these are true at once: memecoin density, cheap execution, structured launch lifecycle, real-time data, and historical corpora.

### 3.1 Comparison (as of late August 2026)

Figures below mix sources with different definitions (spot meme volume vs launchpad volume vs top-30 tokens). They are for ranking, not for precise TAM.

| Criterion | Solana | BNB Chain | Base | Ethereum | Robinhood Chain / other |
|---|---|---|---|---|---|
| Memecoin spot volume | Dominant. Blockworks/SolanaFloor: **~$5.2B** weekly meme volume, **~85%** of combined meme volume across SOL/BNB/ETH/Base/RH in the week of 2026-08-25. ~25% of Solana DEX volume. | Material but smaller. Same week: **~$412M** meme volume (~5.8% of BNB DEX). Four.meme had huge 180d launchpad volume earlier in 2026. | Small for memes. Same week: **~$31M** (~0.5% of Base DEX). Clanker / Virtuals / Zora matter for SocialFi, not for a first bot. | Established large-cap memes (PEPE, SPX, MOG). Fresh-launch meme volume tiny vs SOL. Weekly meme ~**$83M**. | RH chain / Pons appeared in 2026 launchpad fee boards. Too new, too poorly tooled for V1. |
| Launch cadence | Pump.fun + FOMO + LetsBonk + LaunchLab. Pump.fun lifetime: **~$214B** volume, **~1.31M** graduations, **~$347M** 30d avg daily volume, **~80M** unique wallets (Dune @geggonen, 2026-08-26). | Four.meme / Flap.sh. Competitive on launchpad *fees* in some 2025–2026 windows; fewer research datasets. | Clanker (Farcaster), Virtuals. Lower launch density. | Uniswap v2/v3 launches. Expensive, MEV-heavy. | Unproven. |
| Tx cost | Fractions of a cent + priority/Jito tips. Tips dominate, not base fees. | Low but not Solana-low; sandwiching still relevant. | Low vs ETH, higher than SOL. | Prohibitively high for sub-$100 memecoin round-trips. | Unknown. |
| Speed | ~400ms slots. Yellowstone gRPC / shreds for sub-second discovery. | ~3s blocks. Fine for confirmation, worse for sniping. | ~2s. | 12s + PBS/MEV. | Unknown. |
| DEX / launch infra | Pump.fun bonding curve → **PumpSwap** (since Mar 2025, program `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`). Raydium, Meteora DLMM, Jupiter routing (~90%+ of routed swaps). | PancakeSwap, Four.meme curve. | Uniswap, Aerodrome, Clanker. | Uniswap. | Thin. |
| Real-time data | Best in class: Helius LaserStream, Triton Yellowstone, Chainstack, RPC Fast shreds. PumpPortal WS. | Bitquery Kafka, standard EVM logs. | Standard EVM. | Standard EVM + private orderflow. | Weak. |
| Token-analysis tooling | RugCheck, Solana Tracker, Birdeye, DexScreener, GMGN, Jupiter Shield. | Weaker specialized meme tooling. | Moderate. | Etherscan + Token Sniffer; more contract-level rugs. | Weak. |
| MEV / bot competition | Highest. Jito Block Engine on **~95–98% of stake**. Bundle auctions. Bundled insider launches. | High sandwich risk on public mempool. | Moderate. | Extreme for sniping (Banana Pro-type first-block bots). | Unknown. |
| Rug style | Liquidity manipulation, bundles, creator dumps. Classic LP pull is *harder* after PumpSwap graduation because LP is burned. Soft rugs dominate. | Mix of contract and liquidity rugs. | Contract + social. | Contract backdoors, taxes, honeypots. | Unknown. |
| Historical data | Strongest: Dune `dex_solana.trades`, Bitquery archive since mid-2024, HuggingFace 39-day tick corpus, MELT, SolRugDetector. | Dune + Bitquery. Fewer public tick-level meme corpora. | Weaker. | Strong for bluechips, weak for 2026 micro-memes. | Almost none. |
| Infra cost | RPC/gRPC is the main opex. $50–$2k+/mo depending on ambition. | Cheaper nodes, more gas on volume. | Cheap nodes, less signal. | Gas destroys small-size EV. | Not relevant. |

### 3.2 Why Solana despite the competition

Choosing Solana is a **liquidity and research** decision, not a “fastest chain” decision.

1. **The launch funnel is standardized.** Create → bonding curve → (rarely) graduate → PumpSwap AMM with burned LP. That is a research object. BNB Four.meme is similar but has worse public tick data and less mature gRPC tooling for a small team.
2. **We can observe the entire lifecycle at second-level resolution.** The HuggingFace corpus already did this for 39 days. Replicating it on EVM would be slower and more expensive per event.
3. **Round-trip costs can be small enough that a modest edge survives.** On Ethereum, a failed snipe plus priority fee can erase a $50–$200 experiment. On Solana, the *economic* cost of being wrong is slippage and adverse selection, not gas — unless we overbid Jito.
4. **Safety tooling is Solana-native.** Mint/freeze authority, Token-2022 extensions, Pump.fun creator history, and RugCheck reports are first-class.
5. **Base and Ethereum fail the density test.** You cannot train a confirmation model on 31 million dollars a week of Base memes and expect it to generalize. Ethereum memes that matter are already mature; the remaining Uniswap launches are an MEV cage match.

### 3.3 Why not BNB Chain for V1

Four.meme’s 180-day volume was large in some 2026 Dune cuts (one dashboard attributed **$41.4B** to Four.meme vs **$14.8B** Pump.fun bonding + **$23.3B** PumpSwap over ~180d). That is a real market. We still reject it for V1 because:

- Solana has Yellowstone, LaserStream, and a Pump.fun-specific research corpus. BNB has logs and Kafka.
- Sandwiching on a public EVM mempool is a worse execution problem for a first bot than Jito tipping.
- Tooling (RugCheck-class APIs, wallet PnL, GMGN-like coverage) is thinner.
- We can add BNB as V2 once the Solana measurement loop works.

### 3.4 V2+ watchlist

- **BNB / Four.meme** if Solana confirmation EV is real and we want a second, less-saturated launchpad.
- **FOMO / LetsBonk / Raydium LaunchLab** on Solana as additional discovery sources in the same stack.
- **Base** only if Clanker/Virtuals volume returns to a level that supports statistical tests.

---

## 4. Market / competitor research

### 4.1 What exists in 2026

Retail and pro-sumer layer (not an exhaustive list):

| Product | What it is in 2026 | Strength | Weakness relative to our goal |
|---|---|---|---|
| **Axiom** | Dominant Solana web terminal. Dune @adam_tehc: lifetime fees **~$496M**. Mid-Aug 2026 tracked bot volume leader (~$80M+/day in one industry tracker). | Charts, speed, routing, volume, UX | Closed. Not a research ledger. Optimizes conversion, not expectancy. |
| **Photon** | Former leader; volume and fees collapsed through 2025–2026 (DefiLlama-linked reports: Q4 2024 peak fees vs Q2 2026 ~$1.7M). Some outlets describe a rebrand path into Axiom; treat branding as messy. | Historical execution DNA | Declining product; not a research platform |
| **Fomo** | Fast-growing mobile/SocialFi trading surface on Solana; competing with Pump.fun for attention. | Distribution, social graph | Not our architecture; we can *listen* to it |
| **GMGN** | Multi-chain web + Telegram. Smart-money, KOL, sniper/insider/bundle flags, copy trade, “AI agent.” Lifetime fees **~$127M**. | Best public wallet-intel UX | Signals are not ours; copy-trading crowding; no reproducible research store |
| **Trojan** | Telegram + web. Fast SOL execution, copy trade. Lifetime fees **~$225M**. | Mobile execution | Same as other TG bots |
| **BullX** | Halted trading **2026-06-01**; still offline as of Aug 2026. Lifetime fees **~$206M**. | Cautionary tale | Counterparty/product-risk of depending on a terminal |
| **Banana Gun / Banana Pro** | Multi-chain sniper; strong on ETH first-block | Latency stack | Exact thing we should not clone |
| **BonkBot / Maestro / Padre / Moonshot** | Niche TG/mobile | Convenience | No research loop |
| **Birdeye** | Solana-first analytics + Data API | Token/OHLCV/holders/security heuristics | Enrichment, not execution, not expectancy |
| **DexScreener** | Multi-chain pair pages, free API | Ubiquitous; good for enrichment | Too late for discovery; rate-limited |
| **Jupiter** | Default SOL aggregator; Ultra/Swap APIs; Shield; Beam landing | Best execution API for non-curve swaps | Not a scanner |
| **Pump.fun / PumpSwap** | The casino itself | Canonical lifecycle | We trade *on* it, we do not rebuild it |
| **RugCheck.xyz** | Default Solana safety report + API | Authorities, holders, LP, insider beta | Snapshot, not a time series; can lag or miss bundles |
| **Solana Tracker** | Data API + Datastream + risk object on every token | Pump.fun-native, WS, risk, holders | Paid; still not a strategy |
| **PumpPortal** | Unofficial Pump.fun/PumpSwap trade + WS | Fastest *easy* curve execution | 0.5–1% extra fee; Lightning path is operationally risky; no historical data |

Dune @adam_tehc “Trading Bots on Solana” (updated 2026-08-24) all-time fee table (rounded): Axiom $496M, Photon $443M, Trojan $225M, BullX $206M, GMGN $127M, BonkBot $113M. The category extracted **well over a billion dollars in fees**. That is evidence of *user demand for tools*, not of *user profits*.

### 4.2 What they already do well

- Sub-second discovery of new Pump.fun mints and migrations.
- One-click buy/sell with slippage, priority fee, and (sometimes) Jito.
- Holder concentration, mint/freeze flags, and “bundler %” as UI badges.
- Copy-trade of labeled wallets.
- Social overlays (Twitter CA detection, KOL calls).
- Charting from the first print.

### 4.3 What they do not do (our opening)

They optimize **time-to-click** for humans. They do not:

1. Persist a **point-in-time feature vector** for every decision (anti look-ahead).
2. Run **shadow fills** against the liquidity that actually existed.
3. Publish or even internally compute **expectancy after fees, tips, failures, and impact**.
4. Separate **RISK_SCORE** (hard gate) from **OPPORTUNITY_SCORE** (ranking).
5. Maintain a **wallet performance graph that excludes insiders and serial deployers**.
6. Systematically test **exit logic**, which is where most discretionary PnL dies.
7. Survive the operator going offline (BullX).

A custom bot that is “Photon but ours” has **negative** expected value as a product. A custom bot that is a **reproducible research engine with an optional execution adapter** can have value even if we never scale capital — because we will know, in weeks, whether an edge exists.

### 4.4 Potential sources of real edge (not guaranteed)

Ranked by how proprietary they could become:

1. **Bundle / Sybil clustering of supply** (MELT’s strongest unique feature). Retail UIs show top-10; they often miss Jito-co-signed multi-wallet buys and common funders.
2. **Creator longitudinal reputation** across thousands of launches (serial rug vs serial “fair” launcher).
3. **Non-insider smart-money graph** with strict filters (no same-slot creator cluster, no 15-second round trips, minimum unique tokens, purged walk-forward PnL).
4. **Exit microstructure**: detecting the start of coordinated selling *before* the 20-minute collapse, rather than a 2x take-profit.
5. **Regime detection**: memecoin EV is violently non-stationary (TRUMP weekend vs 2026 cooling vs FOMO vs Pump.fun BOOST). A model that knows “this week the casino is dead” is more valuable than a slightly better momentum coefficient.
6. **Not trading.** Capacity to sit in cash is an edge relative to terminals that nag users to click.

Social scraping, “AI agents,” and tweet-to-snipe are **not** recommended as V1 edge. They are already productized (GMGN SnipeX, etc.) and are contaminated by bots.

---

## 5. Proposed edge

The hypothesized edge is **not** “we are faster.” It is:

> **Conditional selection:** only trade young Solana memecoins whose *observable* holder, flow, and creator structure does not match the high-risk cluster, **and** whose short-horizon order flow shows demand from many unlinked wallets, **and** exit when that demand decays or insiders distribute.

Call this **Filtered Confirmation**, not sniping.

### 5.1 Three stacked hypotheses to test (in order)

**H1 — Risk is predictable.**
Given only information available at time T, we can rank tokens by dump/rug probability over the next 20–60 minutes better than chance. *Supported by MELT and arXiv:2608.20271. Must be replicated on 2026 mid-year data because regimes change.*

**H2 — Conditional on passing the risk gate, short-horizon returns are not a fair game.**
After costs, some combination of unique-buyer growth, buy/sell imbalance, and non-insider smart-money flow predicts T+5m / T+15m / T+60m markouts. *Unsupported until we run the first experiment.*

**H3 — Exits can capture a fraction of the right tail without sitting through the modal dump.**
Time stops, sell-pressure triggers, and creator-exit triggers beat fixed 2x/SL. *Unsupported until paper trading.*

If H1 fails, stop. If H1 works and H2 fails, we have a **loss-avoidance tool**, not a trading business. If H1 and H2 work and H3 fails, we will show paper profits that live trading will not realize.

### 5.2 What we explicitly are not claiming

- That Pump.fun as a whole has positive EV for takers.
- That smart-money copy trading works after GMGN made it public.
- That Twitter sentiment is alpha.
- That Rust + gRPC makes a late team competitive at slot-0.

---

## 6. Token discovery strategy

### 6.1 Discovery sources (V1)

Event-driven only. Polling DexScreener/Birdeye is a **backup enricher**, never the trigger.

| Source | Event | Latency | V1? |
|---|---|---|---|
| Yellowstone gRPC / Helius LaserStream, filter Pump.fun program | `create` / buy / sell on bonding curve | tens of ms after shred/processed | **Primary** |
| Same, PumpSwap program `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | graduation `migrate`, then AMM swaps | same | **Primary** |
| PumpPortal `subscribeNewToken` / `subscribeMigration` | convenience feed | typically <100ms behind gRPC in NY; no history | Dev / fallback |
| Raydium LaunchLab, Meteora DBC, LetsBonk, FOMO programs | competing launchpads | same stack | V1.5 if bandwidth allows |
| Jupiter new-route / token list | too late | seconds–minutes | Enrichment only |
| DexScreener / Birdeye trending | retail-visible | too late | Negative control (“would we have seen this anyway?”) |
| X/Telegram | CA posts, group bursts | seconds–minutes, noisy | **Not V1**. Revisit only if on-chain H2 is real |

### 6.2 Event-driven vs polling

Use **push** (gRPC) for detection and **pull** (RPC / DAS / RugCheck / tracker API) for hydration.

Polling at 1s cannot compete and will rate-limit us. Polling is acceptable for:

- holder snapshots every 5–15s on a **watchlist of tens of tokens**, not the entire chain
- creator history (cache)
- price/markout jobs

### 6.3 Discovery pipeline

```
gRPC tx stream
  ├─ classify: pump_create | pump_trade | pump_migrate | pumpswap_trade | other_pool
  ├─ write raw_event (append-only)
  └─ emit TokenSeen { mint, t, source, slot }

TokenSeen
  ├─ SafetyEngine.fast_path (authorities, program, Token-2022, creator cache)  → reject or continue
  ├─ hydrate: curve progress, unique traders, top holders (sampled)
  ├─ SignalEngine.update
  ├─ if RISK_SCORE > hard_max → drop (keep data)
  ├─ if OPPORTUNITY_SCORE > threshold → StrategyEngine.decide
  └─ persist snapshot with as_of_slot (the only legal features for a later decision)
```

**Reaction budget:**

- Hard rejects (mint/freeze, Token-2022 transfer hook, known serial rug creator): **< 200ms** after first event.
- Confirmation entries: **seconds**, not milliseconds. That is a feature. It keeps us out of the Jito war.
- Emergency exits: **same hot path as execution**, target next 1–2 slots.

### 6.4 Universe definition for V1

In:

- Pump.fun SPL tokens (not Token-2022 unless we have a proven sell path)
- Optionally PumpSwap graduates in the first 0–120 minutes

Out:

- Tokens with active mint or freeze authority
- Transfer-hook / permanent-delegate / confidential-transfer extensions until explicitly supported
- Quote assets, LST, bridged majors
- Anything first seen via DexScreener trending (too late; keep as control)

---

## 7. Safety system

### 7.1 Design principle

**RISK_SCORE is a gate. OPPORTUNITY_SCORE is a ranker.**
A token with excellent momentum and a freeze authority is a reject. Never average them into one “buy score.”

Scale (proposed):

- `RISK_SCORE` ∈ [0, 100], **higher = worse**. Default hard reject ≥ 70. Soft penalty 40–70.
- `OPPORTUNITY_SCORE` ∈ [0, 100], **higher = more interesting**. Only computed if risk < hard reject.

### 7.2 Failure modes and detectability

| Failure mode | Detectable programmatically? | How | Confidence |
|---|---|---|---|
| Mint authority active | **Yes** | Mint account `mint_authority` | High — hard reject |
| Freeze authority active | **Yes** | `freeze_authority` | High — hard reject |
| Token-2022 transfer hook / fee / permanent delegate | **Yes** | Parse extensions | High — V1 reject unless sell simulated |
| Hidden extra supply (unrevoked mint) | **Yes** if authority still set | Same as mint | High |
| Metadata mutable / impersonation ticker | **Partial** | Metaplex update authority; ticker collision search | Medium |
| LP not burned (non-PumpSwap pools) | **Yes** | LP mint holders; raydium/meteora vault owners | High |
| Liquidity removal | **Yes** (after the fact); **partial** as leading indicator | LP token movement, vault balance Δ | High after, medium before |
| Bundled supply / multi-wallet creator | **Partial** | Same-tx multi-buy, common funder, Jito bundle ID clustering (MELT method) | Medium–high, best custom edge |
| Insider / creator holdings | **Yes** | Creator ATA + cluster | High |
| Sniper wallet concentration | **Partial** | First-N slot buyers share | Medium (snipers can be us, or insiders) |
| Honeypot (cannot sell) | **Partial** | Simulate sell via Jupiter/Pump; Token-2022 hooks | High if simulation used **before every buy** |
| Fake volume / wash trading | **Partial** | Self-trade, ping-pong, repeated sizes, 2-sided same wallet in one tx (MELT: 21.4% of pre-migration txs were wash) | Medium |
| Artificial liquidity | **Partial** | LP vs reported mcap; curve virtual vs real SOL | Medium |
| Coordinated wallet clusters | **Partial** | Graph on funder + Jito + co-sign | Medium |
| Pump-and-dump / creator selling | **Yes** as it happens | Creator/cluster net flow | High for detection, late for prevention |
| Top-holder selling | **Yes** | Holder snapshot diffs | High |
| Market-maker / bot tape | **Partial** | Size regularity, timing | Low–medium |
| MEV sandwich on our tx | **Partial** | Worse fill than sim; Jito `dontfront` mitigates | Medium |
| Failed tx / expired blockhash | **Yes** | Landing tracker | High |
| Slippage / impact | **Yes** | Quote vs fill; curve formula is deterministic | High |
| Slow exit / RPC delay | **Yes** | Instrumentation | High |
| Liquidity disappearing on exit | **Partial** | Vault reserves in the same slot as exit attempt | Medium |
| Social bot comments / fake hype | **Unreliable** | Ignore in V1 | — |

### 7.3 Fast path vs slow path

**Fast path (must complete before any order):**

1. Token program = SPL Token (or known-good Token-2022 subset).
2. Mint authority = null.
3. Freeze authority = null.
4. No transfer hook / permanent delegate.
5. Simulate a small sell of a hypothetical position (or at least simulate buy then sell on curve).
6. Creator not on `serial_rug` list.
7. Top-1 holder (ex-curve/ex-pool) below threshold.
8. Bundled supply estimate below threshold (if cluster cache warm; else conservative default).

**Slow path (can trail by 1–3s, blocks size-up not first fill):**

- Full RugCheck report
- Solana Tracker `risk` object (snipers/insiders/bundlers/dev %)
- Birdeye security + holders
- Creator prior launches / prior rugged flag
- Holder Gini / top-10 / top-20
- Wash-trade fraction
- Unique-buyer vs unique-seller
- LP composition and SOL reserves

### 7.4 Data sources for safety

| Source | Use | Trust |
|---|---|---|
| Direct RPC / Geyser account data | Authorities, supply, ATAs, vaults | Highest |
| SimulateTransaction | Honeypot / hook | Highest for “can we sell” |
| RugCheck API `GET /v1/tokens/{mint}/report` | Packaged risks, score, top holders | High as input, never sole gate |
| Solana Tracker token `risk` | Bundler/sniper/dev percentages | High as input |
| Jupiter Shield | Extra warnings on routed tokens | Medium |
| Birdeye | Liquidity, trades, holders | Medium; CU-expensive |
| Helius DAS | Metadata, compressed stuff | Medium |
| DexScreener | Pair metadata | Low for safety, fine for display |
| Custom clustering | Bundles | Ours |

Do not block the hot path on Birdeye. Cache RugCheck; if it times out, fail **closed** on first trades of a mint (no trade) rather than fail open.

### 7.5 Suggested hard rejects (starting point — calibrate later)

- Mint or freeze authority present
- Transfer hook or permanent delegate
- Sell simulation fails
- Creator rugged ≥ 2 times in our history (once if last 30 days)
- Estimated bundled + creator supply ≥ 40%
- Top-10 (post-bundle merge) ≥ 50% excluding pool
- Token age < 15s **and** we are not in a dedicated experiment that allows it
- Available 2% depth < 3× intended notional (cannot exit)

These numbers are **priors**, not optimized thresholds.

---

## 8. Signal research

Classification uses 2026 public research plus market-microstructure reasoning. **None of these are proven profitable after costs in our hands.**

Legend: **HIGH VALUE** = prioritize measurement; **POSSIBLY USEFUL** = include in feature store; **LOW VALUE** = keep but do not act on; **UNRELIABLE** = do not use for decisions.

### 8.1 Momentum

| Signal | Class | Why |
|---|---|---|
| Unique-buyer acceleration (new wallets per 15s) | **HIGH VALUE** | Organic demand is many wallets, not one bundle looping. |
| Buy-count acceleration vs sell-count | **HIGH VALUE** | Simple, causal, on-chain. |
| Real SOL in curve / curve progress Δ | **HIGH VALUE** | Deterministic “how much runway to graduation.” |
| Volume acceleration | **POSSIBLY USEFUL** | Contaminated by wash trades (21%+ of pre-mig txs in MELT). Use *unique-wallet-adjusted* volume. |
| Trade frequency | **POSSIBLY USEFUL** | Same contamination. |
| Market-cap acceleration | **LOW VALUE** | Mechanical with buys on a curve; not independent of flow. |
| Liquidity acceleration post-graduation | **POSSIBLY USEFUL** | Real SOL adding to pool vs price going up on thin pool. |

### 8.2 Order flow

| Signal | Class | Why |
|---|---|---|
| Buy/sell notional ratio (ex-wash) | **HIGH VALUE** | |
| Unique buyers / unique sellers | **HIGH VALUE** | |
| Average buy size vs distribution | **POSSIBLY USEFUL** | Very large average buy is a *risk* signal (MELT high-risk pattern), not opportunity. |
| Sequential large buys from unlinked wallets | **POSSIBLY USEFUL** | Could be demand or a pack of snipers. |
| Whale buy from known non-insider | **POSSIBLY USEFUL** | Needs wallet engine first. |
| Repeated identical sizes | **HIGH VALUE as risk** | Wash / bot tape. |

### 8.3 Wallet quality

| Signal | Class | Why |
|---|---|---|
| Involvement of wallets with repeatable *non-insider* PnL | **HIGH VALUE** (after engine exists) | Most defensible alpha if we can de-insider the set. |
| “Smart money” lists from GMGN/Birdeye | **LOW VALUE** | Public, copied, often insiders. |
| Early buyer quality (did early buyers historically take profit vs rug) | **HIGH VALUE** | Directly related to dump risk. |
| Fresh wallets funded from CEX in last hour | **POSSIBLY USEFUL as risk** | Mix of retail and sybils. |
| Known sniper programs | **POSSIBLY USEFUL as risk** | High sniper % → we are late or in a bundle war. |

### 8.4 Distribution

| Signal | Class | Why |
|---|---|---|
| Holder count growth | **HIGH VALUE** | |
| Top-10 / top-20 Δ after bundle merge | **HIGH VALUE** | MELT: merge raises top-10 by **24pp** on high-risk vs **6pp** on low-risk. |
| Creator net position Δ | **HIGH VALUE** | Selling is an exit trigger, not just a filter. |
| Insider distribution into many fresh wallets | **HIGH VALUE as risk** | |
| Gini of holders | **POSSIBLY USEFUL** | Redundant with top-k if computed well. |

### 8.5 Creator behavior

| Signal | Class | Why |
|---|---|---|
| Creator prior launch count and rug rate | **HIGH VALUE** | |
| Creator still holding vs already distributing | **HIGH VALUE** | |
| Creator funding source (mixer / fresh / known factory) | **POSSIBLY USEFUL** | |
| Same-tx create+buy (98.7% in MELT) | **LOW VALUE alone** | Almost universal; not discriminative unless size is extreme. |
| Advertised Telegram / X / website | **POSSIBLY USEFUL** | 2026 survival study: socials associated with ~17× graduation rate vs none. Easy to fake. Use as weak prior, not a buy signal. |

### 8.6 Market structure

| Signal | Class | Why |
|---|---|---|
| Token age | **HIGH VALUE** | Regime of risk changes in minutes. |
| Bonding-curve % complete | **HIGH VALUE** | |
| Graduation / migrate event | **HIGH VALUE as event**, **UNRELIABLE as auto-buy** | Discrete; also the favorite dump window. Trade *around* it only with a plan. |
| Liquidity / mcap ratio (post-grad) | **HIGH VALUE** | Exit capacity. |
| Time of day / SOL trend / launchpad heat | **POSSIBLY USEFUL** | Regime features. MELT included SOL price and hour. |
| Time-to-graduation extremely short | **HIGH VALUE as risk** | High-risk tokens migrate faster with fewer holders (MELT). |

### 8.7 Social

| Signal | Class | Why |
|---|---|---|
| Raw mention count on X | **UNRELIABLE** | Bots, copy-paste CA spam, lag. |
| Telegram group size | **UNRELIABLE** | Bought members. |
| Presence of *any* official socials at create | **POSSIBLY USEFUL** | See graduation association; not a return predictor. |
| Specific KOL tweet | **UNRELIABLE for auto-trade** | Adverse selection; we become exit liquidity. |
| Search trends | **LOW VALUE** | Too slow for this horizon. |
| Sentiment NLP | **UNRELIABLE at launch** | No corpus of honest text. |

**V1 decision: do not include social signals in the trading model.** Store them later as an optional feature for research if H2 on-chain already works.

### 8.8 Additional signals not in the original brief

| Signal | Class | Why |
|---|---|---|
| **Bundle-merged holder concentration** | **HIGH VALUE** | MELT’s main finding. |
| **Wash-trade fraction** | **HIGH VALUE as risk** | 21% of pre-mig txs; 62.9% of high-return manipulated tokens had prior wash/LPI (Mongardini & Mei, arXiv:2507.01963). |
| **Can-sell simulation result** | **HIGH VALUE** | Binary. |
| **Slot-level failed buy ratio** (others failing) | **POSSIBLY USEFUL** | Congestion / trap. |
| **Priority-fee / tip distribution on the token** | **POSSIBLY USEFUL** | How hard bots are fighting. |
| **Name/ticker impersonation of a hot token** | **HIGH VALUE as risk** | |
| **Image reuse / metadata hash collision** | **POSSIBLY USEFUL as risk** | Factory deploys. |
| **Cross-token wallet overlap with recent rugs** | **HIGH VALUE** | |
| **Post-graduation min-price-ratio path** (for exits) | **HIGH VALUE** | 20-minute window is where most damage happens. |
| **Our own fill quality vs mid** | **HIGH VALUE** | Detects whether we are the exit liquidity. |
| **Launchpad identity** (Pump vs LetsBonk vs FOMO) | **POSSIBLY USEFUL** | Different bot ecology. |
| **BOOST / featured / livestream flags** if on-chain or API-visible | **POSSIBLY USEFUL** | Pump.fun product changes alter base rates. |

### 8.9 Practical feature set for the first experiment

Keep it small so we can actually test:

1. minutes_since_create
2. curve_progress
3. unique_buyers
4. unique_sellers
5. buy_sol / sell_sol (ex same-wallet round trip)
6. wash_fraction
7. creator_sol_sold
8. top10_pct_raw
9. top10_pct_bundled
10. holder_count
11. avg_buy_sol
12. creator_prior_rugs
13. failed_sell_sim (0/1)

---

## 9. Entry strategies

### Strategy A — Ultra-early sniper

Enter seconds after create.

- **Pros:** Best price if the token is real.
- **Cons:** We are last among professionals; first among rugs. Bundled supply already taken. Jito tip inflation. Anti-snipe (e.g. Clanker-style decaying tax on other chains; Pump.fun competition via bundles). Cannot run meaningful safety. **Likely negative EV for a new desk.**
- **V1:** Research-only observer. Do not execute.

### Strategy B — Confirmation momentum

Wait 30s–10 min for unique buyers, non-wash volume, and passing risk.

- **Pros:** Uses the 5-minute predictive window from arXiv:2608.20271. Aligns with H1/H2. Latency is a non-goal.
- **Cons:** We buy higher. Many tokens are already dumping. Need strict size vs depth.
- **V1:** **Primary candidate.**

### Strategy C — Graduation / migration

Trade the migrate event to PumpSwap.

- **Pros:** Discrete, liquid relative to curve start, LP burned (classic rug harder).
- **Cons:** MELT: this is *the* unwind window for insiders. 73% crash in 20 minutes. Buying the migrate print is often buying the top of the insider bid.
- **Better variant:** fade only if unique post-mig buyers expand *and* creator cluster is not selling *and* 2–5 minutes of post-mig tape is two-sided. Or trade *pre*-grad only if organic.
- **V1:** Observe all migrations; paper-trade 2–3 rule variants. No live default-on.

### Strategy D — Smart-money confirmation

Enter when historically successful non-insider wallets buy.

- **Pros:** Could be the durable edge.
- **Cons:** Engine does not exist yet; public lists are poisoned; copy-trade crowding; latency to their fill.
- **V1:** Build the ledger. Do not trade it until the wallet graph has walk-forward proof.

### Strategy E — Breakout from early consolidation

- **Pros:** Classic microstructure; might apply post-grad on the few survivors.
- **Cons:** On a 10-minute-old coin, “consolidation” is usually a pause before dump or a wash tape.
- **V1:** Feature only.

### Strategy F — Hybrid scoring

Safety gate + momentum + (later) wallet quality + liquidity + distribution.

- **This is the intended production policy**, but **not** the first experiment. First prove each component’s markout.

### Recommendation (risk-adjusted EV prior)

**Best prior: Strategy B with Strategy F’s risk gate, plus a small experimental sleeve on a conservative Strategy C variant.**

Do not combine all strategies into one live bot. Run them as **labeled policies** in the same paper-trading harness so we can kill losers.

Expected qualitative ranking *before* our tests:

1. B (confirmation) — only plausible V1
2. F (hybrid) — once B has data
3. D (smart money) — V2
4. C (graduation) — dangerous default, interesting conditional
5. E — later
6. A — observer only

---

## 10. Exit strategies

Exits matter more than entries. MELT’s 20-minute post-migration collapse is the modal path. Sniper studies of extractive wallets show **>55% fully exited in <1 minute** and **~85% within 5 minutes**. We should think like them on the way out, even if we refuse to snipe on the way in.

### 10.1 Do not rely on fixed TP/SL alone

Fixed +100% / −20% will:

- get run over by a 90% rug (SL never fills at −20% if liquidity vanished)
- sell winners at +100% that then 10x (acceptable) **or**
- never hit TP and ride a 73% median-style crash

### 10.2 Exit engine: three layers

**INITIAL_EXIT** (planned, always armed at entry)

- Time stop: default **8–15 minutes** on curve; **10–20 minutes** post-grad. Calibrate.
- Partial scale-out: e.g. 30% at +40%, 30% at +80%, runner with trail (numbers are placeholders).
- Volatility trail: ATR/realized-vol on 5s prints, not candle ATR from DexScreener 1m (too slow).

**PARTIAL_EXIT** (degrading but not emergency)

- Unique-buyer growth ≤ 0 for N consecutive 15s buckets
- Buy/sell ratio flips < 1 for N buckets
- Top holders start distributing (cluster sell > X% of supply)
- Wash fraction spikes (bots painting)

**EMERGENCY_EXIT** (market-sell, high slippage tolerance, Jito, skip extra simulation if we just simulated)

- Creator/cluster sells
- Vault SOL drops beyond threshold
- Sell simulation starts failing
- Freeze/mint authority *re-enabled* (should be impossible if revoked; still watch)
- Our position > Y% of remaining pool
- RPC/gRPC stale > Z ms **and** price already moving against us (flatten)
- Daily loss limit / kill switch

### 10.3 Compared tactics

| Tactic | Use |
|---|---|
| Fixed stop | Floor only; **not** the main exit. On curve, a −15% stop may be noise. |
| Fixed take profit | Only as a *scale-out*, never 100% of position. |
| Trailing stop | Runner only, after a green scale-out. |
| Vol-based trail | Preferred over fixed %. |
| Momentum decay | Core INITIAL/PARTIAL. |
| Sell-pressure detection | Core. |
| Whale/creator exit | Emergency. |
| Liquidity deterioration | Emergency. |
| Abnormal holder selling | Partial then emergency. |
| Max holding time | Mandatory. Memes are not investments. |
| Dynamic reduction | If depth falls, shrink even if thesis intact. |

### 10.4 Execution of exits

Exits should be **more aggressive on fees than entries**. Paying an extra 0.002 SOL tip to get out of a melting 0.3 SOL position is rational. V1: always have a precomputed sell transaction template (blockhash refresh loop) for open positions.

---

## 11. Smart-money research

### 11.1 Why it might work

Memecoin PnL is extremely skewed. A small set of wallets repeatedly:

- avoid obvious rugs
- size in after demand
- exit in under a few minutes

If those wallets are **not** the creator cluster, copying *their entry conditions* (not blindly copying size and time) could encode a strategy we would have written ourselves.

### 11.2 Why it often fails

- The leaderboard *is* the insider set (create+snipe bundles).
- Public copy-trade makes their next fill worse, then ours worse.
- Realized PnL without transfer-in of tokens from the mint is misleading.
- One 100x pays for 200 rugs and produces a “smart” wallet that never repeats.

### 11.3 WALLET_SCORE (design)

For each wallet, rolling 14d / 45d / 180d:

- realized PnL, ROI, win rate, trade count
- median hold time, p90 hold time
- token age at entry (prefer wallets that buy *after* 30s, not at slot 0 — those are snipers/insiders)
- unique tokens, unique days (repeatability)
- max drawdown of the wallet’s equity curve
- % of profits from top 1 trade (concentration penalty)
- overlap with creator/funder/Jito clusters (**insider_score**)
- % of round-trips < 15s (extractive sniper, maybe not copyable)
- funding source entropy

**Reject from the copy set if:**

- insider_score high
- profit concentration > 80% in one token
- mostly slot-0 buys
- fewer than N independent tokens
- equity curve is one spike

**Cluster of consistently successful non-insiders:** this is the research prize. Use community detection on co-held tokens *after* stripping creator clusters. If a community of 20–50 wallets repeatedly shows up on tokens that later have positive T+15m markouts *and* they are not funded together, that community is a feature.

### 11.4 V1 vs V2

- V1: ingest every trade we see into `wallet_activity`. Compute daily batch scores. **No live copy.**
- V1.5: paper-trade “wallet_quality ≥ q” as a binary feature inside Strategy B.
- V2: optional small copy sleeve with latency budget of seconds, not ms.

---

## 12. Execution architecture

### 12.1 Language

**Hybrid.**

| Layer | Language | Why |
|---|---|---|
| gRPC ingest, decode, hot risk, order state, tx send | **Rust** | Yellowstone clients, zero-copy, predictable latency, no GC pauses on emergency exits |
| Research, features, backtests, ML | **Python** | Polars/DuckDB, sklearn/XGBoost, notebooks |
| API, dashboard | **TypeScript** (Next.js or similar) | Fast UI |
| Glue / jobs | Python or TS | |

Do not write the event loop in Python. Do not write the research layer in Rust in V1.

Go is acceptable instead of Rust if the team is stronger in Go; Rust is the better default for Solana tx + Yellowstone.

### 12.2 Execution venues

| Path | When | Ease | Speed | Reliability | Cost |
|---|---|---|---|---|---|
| **Self-built Pump.fun buy/sell ix** (Anchor IDL, local sign, send via RPC + Jito) | Curve trades | Medium | High | High if tested | Network + tips only |
| **Self-built PumpSwap ix** | Post-grad | Medium | High | High | Same |
| **Jupiter Swap API v1 / Swap V2** (quote + swap-instructions) | Multi-venue, Raydium/Meteora/PumpSwap | Easy | Medium | High | Jupiter + venue fees. Docs (Aug 2026) state Ultra is superseded by Swap V2 — **verify at implementation time**. |
| **Jupiter Ultra / Beam** | If still supported | Easiest landing | High landing (vendor claims 50–400ms, 0–1 block) | Vendor dependency | Vendor economics |
| **Jupiter Shield** | Pre-trade warnings | Easy | n/a | Advisory | Free with API key |
| **PumpPortal Local** (`/api/trade-local`) | Prototype only | Easy | Medium | Medium | **0.5% extra** |
| **PumpPortal Lightning** | Avoid | Easy | Fast | **They sign**; 1% fee; unaudited third party | Unacceptable for production capital |
| Direct Raydium | Only if Jupiter route is worse | Harder | High | Medium | Custom bugs |

**V1 recommendation:**

- Curve: **own instructions**, sign locally, send to 2 RPCs + Jito `sendTransaction` / bundle.
- AMM: **Jupiter swap-instructions** so we can attach compute budget + Jito tip ourselves, with a PumpSwap-direct fallback if Jupiter has not indexed the pool yet (common in the first seconds after migrate).
- Never put the trading key in a third-party Lightning API.

### 12.3 RPC / streaming

| Component | V1 choice |
|---|---|
| Primary gRPC | Helius LaserStream on **Business ($499/mo)** — mainnet gRPC, up to 10 connections, 24h replay |
| Secondary RPC | Helius or Triton / Chainstack for send + simulate |
| Optional third send path | Jito Block Engine + `jitodontfront` account on swaps |
| Dev fallback WS | PumpPortal newToken/migration (free) |
| Hydration APIs | RugCheck + Solana Tracker (start with RugCheck + own RPC; add Tracker if bundle fields save time) |

Colocate the bot in **the same region as the gRPC endpoint** (typically NY / Frankfurt / Tokyo depending on provider). Do not run this from a laptop on residential Wi-Fi.

### 12.4 Transaction construction

Every send:

1. Fresh blockhash (or durable nonce later; not V1)
2. Compute budget IU + price from recent percentiles
3. Simulate (skip only on emergency exit if last sim < 200ms old)
4. Set slippage from **quoted impact + buffer**, not a global 50%
5. Dual-send: staked RPC (SWQoS) and Jito
6. Record `execution_attempts` until confirmed or expired

---

## 13. MEV considerations

### 13.1 How we get hurt

- **Front-run / back-run / sandwich:** harder on Solana than Ethereum after Jito’s 2024 mempool shutdown, **not zero**. Searchers still see flow via leaders, shreds, and public RPCs. Thin PumpSwap pools are sandwichable.
- **Priority fee / tip auction:** we overpay or we do not land.
- **Stale quotes:** curve moves every buy; Jupiter quote from 300ms ago is wrong.
- **Failed or expired txs:** we pay nothing on Jito fail (good) but miss exits (bad). Base-fee paths can still burn CU.
- **Bundle atomicity used against us:** insiders create+buy+buy in one bundle; we see the mint after they own the curve.

### 13.2 What V1 actually needs

Do **not** build a searcher or colocated shred-processor in V1.

Need:

- Jito send + dynamic tip from `https://bundles.jito.wtf/api/v1/bundles/tip_floor` (50th percentile for normal, 75–95th for emergency exit)
- `jitodontfront` on sizeable AMM swaps
- Tight slippage on entries, **looser on emergency exits**
- Simulate before send
- Position size so a sandwich cannot take a material fraction of bankroll
- No public “pending buy” leaks (don’t log to Discord before send)

Do **not** need in V1:

- Own block engine
- Shredstream ($800–$1,000/IP/mo on Helius) until paper-trading shows we lose because we are 200ms late **on confirmation entries**. Confirmation entries should not care.
- BAM TEE orderflow unless vendor integration is trivial later

If the first live experiment is confirmation at T+2 minutes, **latency is not the bottleneck**. Adverse selection and rugs are.

---

## 14. Data sources and APIs

### 14.1 Recommended stack

| Role | Provider | Notes |
|---|---|---|
| Chain ingest | Helius LaserStream gRPC | Primary |
| RPC simulate/send | Helius + one backup (Triton/Chainstack/Corvus) | |
| Landing | Jito Block Engine | |
| AMM quotes / ix | Jupiter Swap API + Shield | API key from portal.jup.ag |
| Curve ix | Official Pump.fun / PumpSwap IDL (build ourselves) | |
| Safety report | RugCheck API | `api.rugcheck.xyz` |
| Optional enrichment | Solana Tracker Data API | Risk/bundlers/snipers; €50–€397/mo |
| Optional OHLCV/holders | Birdeye | Start **Lite/Starter ($39–$99)**; Premium $199 if WS needed. Easy to overspend CUs. |
| Display | DexScreener free API | Rate limit ~60/min; never hot path |
| Historical research | HuggingFace PumpFun corpus + Dune + Bitquery archive | See §15 |
| Metadata | Helius DAS / Metaplex | |

### 14.2 Providers to avoid as single points of failure

- PumpPortal Lightning (key handling + 1% fee)
- Any custodial TG bot wallet
- Birdeye as the only price source (CU cost + lag vs gRPC trades)

---

## 15. Historical-data plan

### 15.1 What we need to reconstruct

For each candidate event time T:

- token metadata, authorities, creator
- curve progress / pool reserves
- last N trades
- holder sketch (at least top-k + count)
- our feature vector **using only data with slot ≤ T**
- then markouts at T+10s, +30s, +1m, +5m, +15m, +30m, +1h in **executable price** (curve formula or pool reserves), not DexScreener candles

### 15.2 What already exists

| Dataset | Coverage | Grain | Use |
|---|---|---|---|
| **Slinky21/Pumpfun_Memecoin_Corpus** | 2026-06-05 → 2026-07-14, **798,430** launches, 33.6M trades, 26.9M 15s snapshots, 5.7k grads | Token, trade, snapshot, wallet, labeled post-grad outcomes | **First experiment.** CC BY 4.0. Read `KNOWN_ISSUES.md` — quality issues are documented, not fatal. |
| **MELT** (arXiv:2602.13480) | 2024-12-01 → 2025-03-01, 41,470 *migrated* tokens, 200M+ txs, 122 features, bundle traces | Behavioral traces | Method for bundle clustering; regime is older (TRUMP cycle). |
| **SolRugDetector** (arXiv:2603.24625) | Labeled rugs; 100k tokens H1 2025, 76k flagged | Rug taxonomy | Safety labels, not returns. |
| **Catching the Rug** (arXiv:2608.20271) | 6.4M tokens, 7 months | First-5-min features | Confirms H1 is plausible. |
| **Bitquery** | DEXTradeByTokens archive since ~mid-2024; realtime ~7d; Kafka; parquet dumps | Trades | Paid; good backfill |
| **Dune** `dex_solana.trades` | Broad, delayed | Trade | Cheap exploration, not tick-perfect |
| **Birdeye OHLCV** | Minutes | Candles | **Insufficient** for this problem |
| RPC ledger | Full in theory | Raw | Impractical to backfill ourselves at the start |

### 15.3 Recommendation

1. **Phase 1 experiment on the HuggingFace corpus** (already tick-like). Do not wait to build a perfect indexer.
2. **In parallel, start our own prospective collector** (gRPC → Postgres/Parquet). Prospective data is the only unbiased live distribution.
3. Use Bitquery/Dune to sanity-check counts, not as the execution simulator.
4. Do not expect to reconstruct August 2024 Pump.fun at 1-second fidelity cheaply. For V1, **mid-2026 + live forward** is enough.

### 15.4 Costs (order of magnitude)

- HuggingFace corpus: free download (~6.7 GB)
- Dune: free tier for exploration; paid if API-heavy
- Bitquery: typically hundreds of USD/mo if we rely on it for streaming; optional
- Our collector: included in RPC bill

---

## 16. Backtesting methodology

### 16.1 Why candles lie here

A 1-minute OHLC on a token that lives 8 minutes:

- hides intra-minute 50% swings
- assumes we traded at close
- ignores that we could not sell because the only bids were the wash loop
- ignores failed txs
- survivorship: DexScreener lists tokens that existed long enough to be scraped

**Use event-level simulation.** State is curve reserves or AMM x/y. Price is the execution of a trade of size q against that state.

### 16.2 Required realism

For each simulated decision at T:

- Features from `as_of_slot ≤ T` only
- Entry delay: **confirmation policy 2–10s** (parameter); sniper policy 400–800ms (observer)
- Slippage: apply the bonding-curve or constant-product impact of our size **plus** extra bps for jitter
- Fees: Pump.fun curve ~1.25% total (verify current tiered fee at implementation); PumpSwap ~0.25%+creator; Jupiter; Jito tip; priority fee
- Failures: randomly fail entries/exits with empirically estimated land rates (start 85% entry, 90% exit; replace with our paper-trade stats)
- Liquidity constraint: cannot sell more than `min(position, f * pool_base)` 
- No fill if sell simulation would have failed
- Graduation: if we hold through migrate, switch pricing model; include the halt window

### 16.3 Anti-leakage

- Walk-forward by **time**, not random tokens (regimes).
- Purged CV with embargo (token that graduates at t should not sit in train if test starts at t+2 min — use hours/days of embargo).
- Creator and wallet features may only use **their history before T**.
- Outcome labels for training H1 (rug vs not) must not include post-T trades in features.
- Do not tune 40 hyperparameters on 5,689 graduates.

### 16.4 Metrics for research (not vanity)

Primary: **expectancy per trade after costs**, **profit factor**, **max DD**, **CVaR / worst 5% trades**, **trades/day**.
Secondary: win rate, payoff ratio.
Always slice by age, mcap, liquidity, risk bucket, source, hour.

---

## 17. Paper-trading methodology

Shadow mode uses the **same code path** as live until `execution_engine.send`.

```
discover → safety → signals → decide →
  build_tx → simulate →
  paper_fill = apply(impact(state_T), fees, modeled_fail) →
  position_manager (same exits) →
  outcome
```

Rules:

- Fill price = deterministic curve/AMM execution at **decision slot + latency slots**, not the next Birdeye print.
- If live liquidity cannot absorb size, **partial or zero fill**.
- Record the full feature snapshot with the decision id.
- Never look at “mark-to-market using later high.”
- Run ≥ **2 weeks** and ≥ **200 paper trades** (or 4 weeks if frequency is low) before discussing live.
- Compare paper vs a **would-have-been DexScreener candle fill** to quantify how much fantasy PnL candles would have added.

---

## 18. Database architecture

Postgres (or Postgres + Timescale) as system of record. Parquet on object storage for research dumps. Append-only event tables. Updates only on current *state* tables.

### 18.1 Principles

- Every decision row has `as_of_slot`, `as_of_time`, `feature_snapshot_id`.
- Raw events are immutable.
- Live and simulated trades share a schema; `mode` enum: `sim`, `paper`, `live`.
- Do not store private keys. Do not store seed phrases. Wallet pubkey only.

### 18.2 Tables (logical)

**tokens**
`mint PK, name, symbol, uri, program, decimals, created_slot, created_at, creator, launchpad, migrated_at, migrated_slot, pool, status`

**token_snapshots**
`id, mint, as_of_slot, as_of_time, curve_progress, real_sol, virtual_sol, price, mcap_est, unique_buyers, unique_sellers, holder_count, volume_buy_sol, volume_sell_sol, wash_fraction, top10_pct, top10_pct_bundled, creator_pct, source`

**pools**
`address, mint, dex, created_slot, quote_mint, lp_mint, lp_burned`

**liquidity_snapshots**
`id, pool, as_of_slot, base_reserve, quote_reserve, lp_supply`

**trades** (market tape, not ours)
`sig, slot, time, mint, pool, wallet, side, sol_amount, token_amount, price, is_wash, is_bundle, source`

**wallet_activity**
`wallet, mint, first_seen, n_buys, n_sells, realized_pnl_sol, ...` (can be rollup)

**holders**
`mint, as_of_slot, wallet, amount, rank` (store top-k + totals, not always full 10k holders)

**creator_history**
`creator, mint, launched_at, rugged_flag, graduated_flag, creator_pnl_sol`

**signals**
`id, mint, as_of_slot, opportunity_score, model_version`

**signal_components**
`signal_id, name, value, weight`

**risk_assessments**
`id, mint, as_of_slot, risk_score, hard_reject, reasons jsonb, rugcheck_raw jsonb`

**trade_decisions**
`id, time, slot, mint, strategy, mode, risk_id, signal_id, snapshot_id, action (enter/skip/exit), reason, intended_sol, intended_slippage_bps`

**simulated_trades / live_trades**
`id, decision_id, mode, side, requested_sol, filled_sol, filled_tokens, fee_sol, tip_sol, impact_bps, sig, success, error`

**positions**
`id, mint, mode, qty, avg_px, opened_at, status, strategy`

**position_events**
`position_id, time, type (open, partial, trail, emergency, time_stop), decision_id`

**execution_attempts**
`id, trade_id, sent_at, path (rpc|jito), tip, cu_price, landed_slot, error`

**outcomes**
`position_id, pnl_sol, pnl_pct, hold_seconds, mae, mfe, exit_reason, markouts jsonb`

Indexes: `(mint, as_of_slot)`, `(wallet, time)`, `(created_at)`, `gin(reasons)`.

Retention: raw gRPC could be huge. Keep **full raw for 14–30 days**, downsample to snapshots forever, keep all decisions/trades forever.

---

## 19. System architecture

### 19.1 V1 shape: modular monolith, two runtimes

Do not start with 12 microservices. Two deployable units:

1. **`engine` (Rust)** — ingest, safety fast path, strategy, execution, position manager
2. **`research` (Python)** — batch features, wallet scores, backtests, model training
3. **`web` (TS)** — dashboard + read API

Postgres is the bus. Redis optional for hot token state if Postgres is enough (it likely is at V1 size).

```
                    Yellowstone / LaserStream
                              │
                              ▼
                     ┌─────────────────┐
                     │  Discovery      │
                     │  (decode txs)   │
                     └────────┬────────┘
                              │
                              ▼
                     ┌─────────────────┐     RugCheck / RPC
                     │  Token Safety   │◄────────────────────
                     └────────┬────────┘
                              │
              ┌───────────────┼────────────────┐
              ▼               ▼                ▼
        Signal Engine   Wallet Intel*    Risk Engine
              │               │                │
              └───────────────┼────────────────┘
                              ▼
                       Strategy Engine
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
              Execution            Paper Fills
              (Jito/Jup/Pump)      (same models)
                    │
                    ▼
              Position Manager ──► Outcome Tracker
                              │
                              ▼
                         PostgreSQL
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
                 Dashboard           Research jobs
```

\*Wallet intel can be batch (Python → tables) in V1; Rust only reads scores.

### 19.2 Later split (only if needed)

Separate `execution` if we want a tiny binary with keys on a locked box. That is a **security** split, worth doing before live, not before paper.

### 19.3 Observability

- Prometheus + Grafana: ingest lag, slot delay, decide-to-send ms, land rate, tip paid, open positions, daily PnL
- Pager on: ingest lag > 2s, land rate < 50%, daily loss limit, unknown error rate
- Structured logs **without** secrets or full signed tx bytes

---

## 20. Dashboard design

### Overview

- Equity curve (paper vs live, clearly labeled)
- Open positions with unrealized, age, risk
- Today: trades, win rate, expectancy, fees/tips, failed txs
- Drawdown from peak
- Ingest health (slot lag)

### Scanner

Table of new tokens (last 2h): mint, age, launchpad, curve %, liq, mcap, unique buyers, RISK, OPP, flags, action (why skipped).

Color by hard-reject vs traded vs watching.

### Trades

Each row: entry/exit px and time, strategy, mode, PnL, **reason_in**, **reason_out**, snapshot link.

### Token detail

Price (from our prints), volume, liq, holders, creator, risk flags over time, signal components, tape, our decisions overlaid. Link Solscan / pump.fun.

### Research

- Signal bucket vs next-hour markout
- Strategy comparison
- PnL distribution (log scale)
- Calibration plot: predicted dump probability vs realized
- Wallet-score decay

Build the dashboard as soon as paper trades exist. It is how we notice look-ahead bugs.

---

## 21. Security architecture

### 21.1 Capital segregation

```
Treasury (hardware wallet / multisig)  ──manual──►  Hot trading wallet
        never on the bot host                      bot signer
                                                   funded to: max_daily_loss + open_risk + fees
```

Rules:

- Hot wallet balance ≤ configured **operating capital** (e.g. 5–10% of total, hard cap in SOL).
- Sweep profits to treasury on a schedule (manual or script from a **different** key that can only transfer to treasury).
- Bot key cannot be the treasury.

### 21.2 Key handling

- Keys in OS keyring / encrypted file / cloud KMS — **never git, never Docker image, never logs**.
- `.env` in `.gitignore`; production uses systemd credentials or AWS/GCP secret manager.
- DB compromise must not reveal keys (they are not in DB).
- Separate `read` RPC key vs `send` if vendor supports it.
- Paper mode **refuses to load the live signer**.

### 21.3 Runtime controls

- Kill switch: env/file/HTTP authenticated endpoint → cancel all, halt entries, optional market-sell all paper/live.
- Max daily loss (SOL and %), max position, max concurrent, max token age sleeve.
- If drawdown from session start > X: flatten and lock until human reset.
- Allowlist spend: program ids (Pump, PumpSwap, Jupiter, compute budget, Jito tip accounts) — reject unexpected ix.
- Withdrawals: hot wallet should **not** need to withdraw to arbitrary addresses in normal operation. Optional: on-chain timelock or only-transfer-to-treasury instruction policy (Squads/native if we graduate to that).

### 21.4 AppSec

- Dashboard behind auth; no public “buy” buttons talking to the engine without the same policy engine.
- Rate-limit any control API.
- Do not paste CAs from Discord into auto-buy without the safety engine.

---

## 22. Infrastructure costs

USD, 2026 list prices, approximate. Tips/fees scale with trading and are **not** in the infra rows.

### 22.1 Research prototype (H1/H2 on corpus + tiny collector)

| Item | Low | Recommended |
|---|---|---|
| HuggingFace corpus / Dune | $0 | $0–$50 |
| Helius Developer | $49 | $49 |
| RugCheck | $0 | $0 |
| Single VPS (NYC/FRA, 4 vCPU) | $20 | $40 |
| Postgres on same box | $0 | $0 |
| **Total infra** | **~$70** | **~$90–$150** |

### 22.2 Paper-trading production (always-on gRPC, dashboard)

| Item | Low | Recommended | High performance |
|---|---|---|---|
| Helius | $49 (WS only, not enough) | **Business $499** (gRPC) | Professional $999 + 5TB $400 |
| Backup RPC | $0 | $49–$160 (Chainstack/Corvus starter) | $325–$400 |
| Solana Tracker | $0 | Advanced €50 ≈ $55 | Premium €397 ≈ $430 |
| Birdeye | $0 | Lite $39 | Premium $199 |
| Server (8 vCPU, 32GB, same region as gRPC) | $40 | $80–$150 | $200+ colo |
| Managed Postgres | $0 (self) | $30–$80 | $150 |
| Monitoring | $0 | $20 | $50 |
| Domain/auth | $10 | $10 | $10 |
| **Total infra / mo** | **~$100–$200 (inadequate ingest)** | **~$750–$1,100** | **~$2,000–$3,000** |

### 22.3 Small live bot

Recommended paper stack **plus**:

- Jito tips + priority: **highly variable**. A 20-trade/day confirmation bot at 0.002 SOL tip both sides ≈ 0.08 SOL/day in tips. At $100/SOL that is ~$240/mo — **illustrative only**.
- DEX/launchpad fees: 0.25–1.25% round trip dominates tips at larger size.
- Still **no** shredstream unless metrics demand it.

### 22.4 Serious low-latency live

| Item | Cost |
|---|---|
| Helius Pro + data add-on or Triton/RPC Fast Aperture | $1,000–$3,000 |
| Shredstream | $800–$1,000 / IP |
| Colo / dedicated | $500–$2,000 |
| Second region failover | +50–100% |
| Birdeye Business | $499+ |
| On-call | human cost |
| **Total** | **$4k–$10k+/mo before trading fees** |

**Do not buy this until confirmation paper-trading is positive.** It would be spending on the wrong bottleneck.

### 22.5 Configurations named

- **LOW COST:** corpus research + Helius $49 + VPS. No gRPC mainnet. Not for paper fidelity.
- **RECOMMENDED:** Helius Business + backup RPC + RugCheck + optional Tracker €50 + $100 server + Postgres. **This is the first serious spend.**
- **HIGH PERFORMANCE:** only after live edge is real and landing/lag is the measured constraint.

---

## 23. Expected-value analysis

### 23.1 Preconditions for +EV

Let \(p\) be win rate, \(W\) average winner %, \(L\) average loser %, \(c\) round-trip cost % (fees+tip+impact+fail drag).

Need:

\[
p(W - c) - (1-p)(L + c) > 0
\]

Worked example (hypothetical, **not a forecast**):

- \(p = 0.35\), \(W = 0.40\), \(L = 0.25\), \(c = 0.03\)
- EV = \(0.35 \times 0.37 - 0.65 \times 0.28 = 0.1295 - 0.182 = -0.0525\) → **loses 5.3% per trade**

Even a “35% win rate, 40% avg win, 25% avg loss” **dies to 3% costs**. Costs are not a footnote.

Curve round-trip at ~1.25% each way is already ~2.5% before tips and impact. Thin-pool impact of a 0.2 SOL trade can be several percent. Failed entries that we retry into a worse price add more.

### 23.2 How fees destroy a theoretical edge

| Cost | Typical | Effect |
|---|---|---|
| Pump.fun curve fee | ~1.25% in + 1.25% out (confirm live tiers) | Hard floor |
| PumpSwap | ~0.25%+ | Better; still 2 sides |
| Jito tip | 0.001–0.02 SOL | Brutal on 0.05 SOL size; fine on 1 SOL if land rate needs it |
| Impact | 1–15%+ on tiny pools | Main killer |
| Fail / retry | 5–20% of sends | Missed exits worse than missed entries |
| Third-party bot fee | 0.5–1% extra | Why we will not use Lightning |

**Rule:** if modeled \(c\) ≥ half of modeled \(W\), skip the token even if OPP is high.

### 23.3 Bankroll scenarios (illustrative, not promises)

Assume a **hypothetical** post-cost EV of **+0.8% per trade** (optimistic for a filtered confirmation system that is actually working) and **8 trades/day**, 20 trading days: 160 trades/month, +128% *if independent, no DD, no regime break* — which will not happen. More honest framing:

| Bankroll | Max / trade (1% risk, capped by depth) | Practical cap | What goes wrong at this size |
|---|---|---|---|
| $1,000 | $10 | Often **depth-capped below $10** on young coins | Fees/tips eat EV; one 80% rug is −$8; variance dominates; **statistically meaningless** |
| $5,000 | $50 | Depth often $20–$80 | Barely enough to learn; still high variance |
| $10,000 | $100 | Depth still binds on curve | First size where costs *might* not dominate if we only trade deeper names |
| $50,000 | $500 | **Cannot** put $500 into a $8k curve token without moving it | Forced into more mature names; original edge may vanish |

**Scalability:** memecoin liquidity is the wall. A strategy that makes 2% on $30 notional does not make 2% on $3,000. Capacity of a confirmation-on-young-memes strategy is likely **low five figures of equity**, not a fund. That is acceptable if the goal is a small automated desk, not a VC-scale quant shop.

### 23.4 $1k specifically

At $1,000, treat live as **instrumented tuition**, not a profit center. Infra at recommended level (~$800/mo) **cannot** be paid by a $1k bankroll. Either:

- keep infra at low-cost until paper is proven, or
- accept that this is an R&D project funded separately from trading capital.

### 23.5 Honest base rate

If we traded random Pump.fun graduates at migration, MELT suggests expected markouts over the next hour are **sharply negative**. Our entire thesis is **selection**. Without selection, EV is negative. With selection, EV is an empirical unknown.

---

## 24. Development roadmap

Do not advance a phase because the software “works.” Advance because the **success criteria** hit and **failure criteria** did not.

### PHASE 0 — Research (this document)

- **Objective:** Decide chain, architecture, first experiment, kill tests.
- **Implementation:** This file + go / no-go review.
- **Tests:** Sources cited; numbers dated.
- **Success:** Stakeholder accepts BUILD WITH CONDITIONS and the first experiment design.
- **Failure:** Desire to skip to live sniping anyway → do not staff the project.

### PHASE 1 — Data collector + schema

- **Objective:** Persist every Pump.fun create/trade/migrate and PumpSwap trade we see, plus periodic snapshots.
- **Implementation:** Rust gRPC ingest, Postgres schema in §18, disk Parquet dump.
- **Tests:** Re-parse a known mint; snapshot count vs trade count sanity; slot lag histogram.
- **Success:** ≥ 7 continuous days, lag p95 < 1s, no silent gaps > 30s.
- **Failure:** Cannot keep up on Business gRPC; data holes → fix infra before any model.

### PHASE 2 — Token discovery + hydration

- **Objective:** TokenSeen → metadata + curve state within 200ms (local) / 1s (RPC).
- **Implementation:** Program filters, DAS/metadata, curve account decode.
- **Tests:** Golden tx fixtures for create, buy, migrate.
- **Success:** 99% of creates assigned a mint and creator; hydration errors alerted.
- **Failure:** Missing migrations.

### PHASE 3 — Safety engine

- **Objective:** Hard rejects + RISK_SCORE with reasons; sell simulation.
- **Implementation:** Authorities, extensions, RugCheck, creator cache, simulate sell.
- **Tests:** Known freeze-mint tokens reject; Pump.fun standard SPL passes; hook tokens reject.
- **Success:** 0 known honeypots would have been bought on replay of last 7 days; false-reject rate measured (not yet optimized).
- **Failure:** Sell simulation unreliable → no live, keep paper with extra caution.

### PHASE 4 — Signal engine (features only)

- **Objective:** Write the §8.9 vector every 15s for watchlist tokens.
- **Implementation:** From our tape, not Birdeye.
- **Tests:** Feature at T does not change when recomputed with data after T (replay).
- **Success:** Look-ahead tests pass; features match manual Solscan sample.
- **Failure:** Leakage in tests → do not train.

### PHASE 5 — Historical evaluation (FIRST EXPERIMENT lives here)

- **Objective:** Test H1 and H2 on HuggingFace corpus + our first collected week.
- **Implementation:** DuckDB/Polars event simulator with fees/impact/latency. Simple rules + logistic/XGBoost for dump prediction. No live.
- **Tests:** Purged time split; cost model unit tests on curve math.
- **Success (H1):** Out-of-sample AUPRC meaningfully above prevalence; high-risk decile dump rate >> low-risk decile.
- **Success (H2):** After costs and 3s delay, top opportunity decile **among risk-passed tokens** has positive mean T+15m markout **and** the result is not one week / 30 tokens.
- **Failure:** H1 dead → **stop or pivot to purely manual research**. H1 live H2 dead → **no trading bot**; optional “rug warning” product (out of scope unless we want it).

### PHASE 6 — Paper trading

- **Objective:** Same decisions as live, no sends.
- **Implementation:** Strategy B + risk gate + exit engine. Dashboard.
- **Tests:** Paper fill matches replay of reserves at T+delay.
- **Success:** ≥ 200 trades or 4 weeks; post-cost expectancy > 0 in **two separate calendar weeks**; max DD within pre-set bound (e.g. < 25% of paper equity); land-rate model not fantasy.
- **Failure:** Persistent negative expectancy → change strategy or **kill**. Do not “go live smaller.”

### PHASE 7 — Strategy optimization (still paper)

- **Objective:** Calibrate thresholds, exits, size. Add wallet feature if data supports it.
- **Implementation:** Walk-forward; freeze a policy.
- **Success:** Improvement holds on a **held-out week not used for tuning**.
- **Failure:** Overfit carousel (every week a new rule) → freeze or kill.

### PHASE 8 — Small-capital live experiment

- **Objective:** Measure **implementation shortfall** vs paper (slippage, fails, latency).
- **Implementation:** Hot wallet with tight caps. Kill switch. Same policy as paper.
- **Tests:** Every live trade has a paper twin; diff explained.
- **Success:** 2–4 weeks; live vs paper gap understood; live post-cost EV not significantly worse than paper; no security incident.
- **Failure:** Live much worse (adverse selection) → back to paper or kill. Software bugs that lose money → halt.

### PHASE 9 — Production optimization

- **Objective:** Reliability, key isolation, maybe second launchpad, maybe wallet sleeve.
- **Not:** 10× size. Size only with depth.
- **Success:** Months of monitored +EV after costs, capacity study.
- **Failure:** Regime shift to zero — shrink to zero size (feature, not shame).

---

## 25. Kill criteria

Abandon or freeze the **trading** project (collector can remain as a hobby) if any of these hit:

1. **H1 fails** on corpus + 2 weeks live data: dump prediction ≈ random after authorities-only filter.
2. **H2 fails:** risk-filtered confirmation markouts ≤ 0 after realistic costs on walk-forward.
3. Modeled round-trip **c ≥ expected gross edge**.
4. Paper PnL **negative for 4 consecutive weeks** after one freeze of the rule set.
5. Live implementation shortfall **erases paper edge** for 2 weeks.
6. Max DD on paper or live exceeds **precommitted** bound (suggest 30% of allocated trading capital).
7. Profitability exists only on **n < 100** trades or a **single** regime week (e.g. one viral day).
8. Edge exists only on size **< $15** where fees dominate any realistic bankroll.
9. We cannot detect bundled supply well enough, and unfiltered left tail is unbounded.
10. Pump.fun / Solana structure changes (new anti-bot, fee explosion, gRPC unaffordable) so the experiment cannot be run honestly.
11. Security incident involving the hot key.
12. The team decides the goal is “be Axiom” rather than “measure EV.”

Killing the project is a successful use of this document.

---

## 26. Final recommendation

### Verdict: **BUILD WITH CONDITIONS**

Build a **Solana research and paper-trading system** aimed at **filtered confirmation** of young Pump.fun / PumpSwap tokens. Do not build a sniper. Do not fund a live bot until H1 and H2 clear. Do not spend on shreds/colocation until paper shows a latency problem.

Conditions:

1. Phase 5 experiment is mandatory and public-to-the-team (write up numbers).
2. Live capital starts as a **measurement sleeve**, segregated, capped.
3. Infra spend jumps to Helius Business only when the collector needs gRPC, not before.
4. Kill criteria are written in the runbook, not this markdown only.

---

## RECOMMENDED V1

A single Solana engine that:

1. Streams Pump.fun + PumpSwap via Yellowstone/LaserStream.
2. Records raw events and 15-second snapshots.
3. Hard-rejects mint/freeze/hooks/failed sell-sim/serial-rug creators/extreme bundled supply.
4. Computes RISK_SCORE and OPPORTUNITY_SCORE separately.
5. Paper-trades **Strategy B (confirmation momentum)** with time-boxed, flow-based, creator-exit-aware exits.
6. Exposes a dashboard of skips, paper fills, and markouts.
7. Does **not** auto-copy wallets, **not** snipe creates, **not** scrape Twitter.

Optional observer: log what a slot-0 sniper *would* have done, to prove it is worse.

---

## TECH STACK

| Piece | Choice |
|---|---|
| Hot engine | **Rust** (`yellowstone-grpc-client`, `solana-*` / `agave` crates) |
| Research | **Python 3.12 + Polars + DuckDB + scikit-learn / XGBoost** |
| Dashboard | **TypeScript + Next.js** |
| DB | **PostgreSQL 16** (+ optional Timescale) |
| Deploy | Linux VPS in gRPC region, Docker Compose is enough for V1 |
| Secrets | env/KMS, not git |
| IaC | defer |

---

## DATA PROVIDERS

**Must:**

- Helius (RPC + LaserStream)
- Jito Block Engine + tip_floor
- Jupiter Swap API + Shield
- Pump.fun / PumpSwap program IDLs (self-built ix)
- RugCheck API
- Direct chain state (authorities, vaults)

**Should:**

- Second RPC vendor
- HuggingFace PumpFun corpus (research)
- DexScreener (UI links, control)

**Nice:**

- Solana Tracker (bundler/sniper fields)
- Birdeye (start cheap, watch CUs)
- Dune / Bitquery for cross-checks

**Avoid for production signing:**

- PumpPortal Lightning
- Custodial Telegram bots

---

## FIRST EXPERIMENT

**Name:** *Filtered Confirmation Markouts on Pump.fun (mid-2026 corpus)*

**Data:** `Slinky21/Pumpfun_Memecoin_Corpus` (2026-06-05 to 2026-07-14), after applying `KNOWN_ISSUES.md` filters. Optionally join post-grad snapshots.

**Question 1 (H1):** Using only information from the first 2, 5, and 10 minutes after create (and, separately, first 2 minutes after migrate), can a simple model (logistic regression, then XGBoost) rank dump/rug/death outcomes better than (a) random, (b) “reject if mint/freeze”, (c) RugCheck-like authority rules only?

**Question 2 (H2):** Among tokens that pass a **frozen** risk rule (authorities + top10_bundled < X + unique_buyers ≥ Y), does a confirmation rule (unique buyers accelerating, wash_fraction < Z, creator not selling) have **positive mean executable return** at T+5m and T+15m after:

- 1.25%+1.25% curve fees (or actual fee schedule)
- 3 second entry delay
- 1% extra impact
- 0.002 SOL tip each way
- 10% entry fail / 5% exit fail

**Question 3:** Same as Q2 but **entry at migrate** vs **entry at T+3m post-migrate if not dumping**. This answers sniping-vs-mature.

**Protocol:**

- Split by calendar week, train on early weeks, test on later weeks (regime-aware).
- Report expectancy, CVaR, n, and a bootstrap CI.
- Pre-register X, Y, Z as a small grid, pick on train, **one** test evaluation.
- Write results into `experiments/EXP001_filtered_confirmation.md` before any live discussion.

**Pass:** H1 ranking works **and** H2 lower CI on T+15m post-cost mean is ≥ 0 on test weeks, n ≥ 300 risk-passed tokens (or all graduates if smaller, with that limitation disclosed).

**Fail:** proceed to kill or to a narrower research question (e.g. creator-reputation only as a warning tool).

---

## Appendix A — Key 2026 sources

On-chain / market:

- Dune Pump.fun analytics (@geggonen): ~$214B lifetime volume, ~1.31M graduations, ~$347M 30d avg daily volume (retrieved 2026-08-26)
- SolanaFloor / Blockworks (2026-08-26): Solana ~$5.2B weekly meme volume, ~85% share vs BNB/ETH/Base/RH
- MemeFees / DefiLlama launchpad fees (2026-08-27): pump.fun ~38% of 24h launchpad fees
- Dune @adam_tehc trading bots: Axiom/Photon/Trojan/BullX/GMGN fee history
- HuggingFace Slinky21/Pumpfun_Memecoin_Corpus (Jun–Jul 2026)

Academic:

- Hu et al., MELT, arXiv:2602.13480 (May 2026 v2)
- Li et al., Catching the Rug, arXiv:2608.20271 (Aug 2026)
- SolRugDetector, arXiv:2603.24625 (Mar 2026)
- Pump.fun graduation survival, arXiv:2607.02823 (Aug 2026 revision)
- Mongardini & Mei, meme manipulations, arXiv:2507.01963

Infra docs:

- Helius plans (Business $499, LaserStream gRPC; Professional $999; shreds ~$800–$1,000/IP)
- Jupiter Developers: Ultra vs Swap V2 (verify at build time); Beam landing claims
- Jito: bundles, tip_floor, `jitodontfront`
- RugCheck API, Solana Tracker risk object, PumpPortal fees (0.5%/1%)
- Chainstack: Pump.fun now migrates to PumpSwap (since 2025-03-20), program `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`

---

## Appendix B — Explicit non-goals for V1

- Multi-chain
- Telegram user bot
- Token creation / bundling (illegal or unethical depending on jurisdiction; also not our business)
- Guaranteed profit dashboards
- LLM “agent that apes”
- Training a transformer on 1-minute candles
- Competing with Axiom on UX

---

*End of research document. No production trading code should be written until Phase 5 results are reviewed.*
