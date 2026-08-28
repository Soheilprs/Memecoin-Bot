# Memecoin Trading Bot — Research V2

**Date:** 2026-08-27
**Supersedes (does not replace):** `MEMECOIN_BOT_RESEARCH.md`
**Status:** Second research/architecture phase. No production trading code.
**Verdict:** **BUILD WITH CONDITIONS**

This revision keeps V1’s core thesis — *discover early, do not automatically buy early, gate on security, measure before live capital* — and drops the Solana-only assumption.

---

## 1. What changed from V1

| V1 | V2 |
|---|---|
| One chain: Solana | Three first-class chains: **Solana, Base, Robinhood Chain** |
| Robinhood Chain treated as too early | Independently verified live (chain ID **4663**, public RPC serving blocks, Pons V2 factory has bytecode, launches firing now) |
| Base dismissed as too small for a first bot | **BUILD** as the EVM security lab and a real (smaller) launch surface |
| Discovery was Solana gRPC only | Multi-chain discovery adapters → one `TokenDiscovered` event |
| Security was Solana authorities + RugCheck | Split: **Solana on-chain safety** vs **EVM static + state + simulation** |
| Edge hypothesized as filtered confirmation on Pump.fun | Same strategy family, but **chain/launchpad is an experimental factor** (H5) |
| Rust Solana engine | **Rust multi-chain engine** (`yellowstone` + `alloy`) + Python research + TS dashboard |
| First experiment = HuggingFace Pump.fun corpus | Keep that, **and** start prospective collectors on all three chains immediately |

**What did not change**

- Ultra-early sniping is still not the V1 execution strategy.
- RISK_SCORE and OPPORTUNITY_SCORE stay separate.
- Paper trading and statistical gates still block live capital.
- We still do not clone Axiom / GMGN / Telegram snipers.

**The new principle (explicit)**

Finding a token at T+0 and buying at T+0 are different products. V2 optimizes the first. The second remains an experiment.

---

## 2. Current Solana ecosystem (August 2026)

Solana remains the densest, best-tooled memecoin market. Launchpad fee share (MemeFees / DefiLlama, 2026-08-27): **SOL ~47% of tracked 24h launchpad fees**, Pump.fun alone **37.8%** ($1.95M fees / $63.6M vol 24h). FOMO is a real #3 globally ($665k fees 24h). Weekly meme *spot* volume was still ~$5.2B on Solana vs ~$31M on Base (SolanaFloor / Blockworks, week of 2026-08-25).

### 2.1 Launchpads that matter

| Pad | Role | Graduation | V1 monitor? |
|---|---|---|---|
| **Pump.fun** | Dominant factory. Bonding curve → PumpSwap since 2025-03-20 | ~85 SOL real reserves historically (~$69k mcap headline; SOL-denominated) | **Yes — primary** |
| **PumpSwap** | Native AMM for graduates. LP burned | n/a (post-grad) | **Yes** |
| **FOMO** | Social / mobile launchpad, large fee share | Needs verification of on-chain program vs in-app routing | **Yes, after program ID verified** |
| **LetsBonk.fun** | Runs **on Raydium LaunchLab** (`LanMV9sAd7wArD4vJFi2qD4…`) distinguished by config account `FfYek5vEz23cMkWsdJwG2oa6EphsvXSHrGpdALN4g6W1` | LaunchLab migrate_to_amm / cpswap | **Yes** |
| **Raydium LaunchLab** | Generic launchpad program | migrate_to_amm / migrate_to_cpswap | **Yes** |
| **Meteora DBC** | Dynamic bonding curve used by Bags, Jupiter Studio, others | `migrate_meteora_damm` / `migration_damm_v2` | **Yes** |
| **Jupiter Studio** | Meteora DBC + account suffix `jups` | Same DBC migrate | **Yes (filter)** |
| **Bags** | Meteora DBC + Bags program account | Same | V1.5 |
| **Moonshot / Moonit** | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | `migrateFunds` | Optional |
| **Heaven** | `HEAVENoP2qxoeuF8Dj2oT1GHEnu49U5mJYkdeC8BAX2o` | pool create | Optional |
| Direct Raydium AMM / CPMM / CLMM, Meteora DLMM | Non-pad pool creates | n/a | Watch, not primary |

### 2.2 Launch funnel (Pump.fun — best measured)

From V1 sources, still the only statistically honest Solana funnel:

```
~20,500 tokens / day          (798,430 / 39d, Jun 5–Jul 14 2026 corpus)
        ↓
~0.2–0.7% graduate            (window-dependent)
        ↓
of graduates, ~84% high-risk  (MELT, older window, migrated-only)
        ↓
~73% of graduates drop
   below 40% of migrate px
   within 20 minutes
```

Useful lifetime of a random Pump.fun token is **minutes**. Confirmation strategy lives in the 30s–15m band, not in “hold for the narrative.”

### 2.3 Data / execution (unchanged in kind)

Yellowstone / Helius LaserStream is still the right ingest. Jupiter for AMM, self-built Pump ix for the curve, Jito for landing. See V1 §§12–14.

---

## 3. Current Base ecosystem (August 2026)

Base is **not** a Solana-scale meme casino. MemeFees: Base **~2.6%** of 24h launchpad fees (~$118k vs SOL $2.1M and RHC $1.86M). SolanaFloor: Base meme spot **~$31M / week**. That is enough to collect and to train an EVM security engine. It is probably **not** enough to be the P&L engine.

What Base *is* good for:

- Mature RPC, traces, Anvil forks, Tenderly, Basescan verification.
- **Known factories with template tokens** (Clanker, Zora, Flaunch). Contract risk collapses to “is this the template?” plus creator/holder risk.
- Same EVM wallet set that later appears on Robinhood Chain.

### 3.1 Launch mechanisms

| Pad | Mechanism | Factory / key contract (Base) | Verified this session? |
|---|---|---|---|
| **Clanker v4** | `deployToken()` → ClankerToken + Uniswap v4 hook + LP locker. 100B supply. Extensions ≤90%. MEV/sniper modules. | Factory `0xE85A59c628F7d27878ACeB4bf3b35733630083a9` | **Yes** (bytecode 12375) |
| Clanker v3.1 | Older Uniswap v3 LP locker path | `0x2A787b2362021cC3eEa3C24C4748a6cD5B687382` | **Yes** (bytecode 17351) |
| **Zora Coins** | Content/creator/trend coins via ZoraFactory → Uniswap v4 + Zora hook. Factory is a **proxy** (Zora team can upgrade the factory; deployed coins claimed immutable). | `0x777777751622c0d3258f214F9DF38E35BF45baF3` | **Yes** (proxy-sized code 130) |
| **Flaunch** | Uniswap v4 PositionManager, fair launch then Progressive Bid Wall | `0x516af52d0c629b5e378da4dc64ecb0744ce10109` | **Yes** (bytecode 9760) |
| **Doppler** | Market-curve / Uniswap v4 position tooling used under Zora and Bankr “Pure Markets” | Multiple deployers historically (`0x16f5acb6…`, `0xf0b5141d…` in Dune maps; Bankr Doppler deployer `0xD59cE43E53D69F190E15d9822Fb4540dCcc91178`) | **Partial — treat as family, enumerate live** |
| **Bankr** | Launches *through* Clanker v4 and Doppler, not always its own token impl | Historical deployer `0x2112b8456AC07c15fA31ddf3Bf713E77716fF3F9` returned **empty code** on 2026-08-27 | **Needs re-map** |
| **Virtuals** | AI-agent tokens, bonding curve | Curve `0x1A540088125d00dD3990f9dA45CA0859af4d3B01`; older deployers in Dune `0xc169a240…`, `0x71b8efc8…` | Curve **yes** (code 1167) |
| **Uniswap Liquidity Launchpad / CCA** | Official Uniswap pad | LiquidityLauncher `0x00004c4ccc709Ef590F7C81102C0689F0263D4e9`; LBPStrategy Base `0x34385dD739FE5464892BF0bA4CC42492804dA000` | Launcher **yes** (code 3747) |
| Uniswap V3 | `PoolCreated` | Factory `0x33128a8fC17869897dcE68Ed026d694621f6FDfD` | **Yes** |
| Uniswap V4 | `Initialize` on PoolManager | `0x498581fF718922c3f8e6A244956aF099B2652b2b` | **Yes** |
| Uniswap V2 | `PairCreated` | Commonly cited `0x8909Dc15e40173Ff4699343b6eB8132d548eF197` returned **empty code** | **NEEDS VERIFICATION** |
| Aerodrome | Base MetaDEX; Ignition launchers exist | PoolFactory `0x420DD381b31aEf6683db6B902084cB0FFECe40Da` | **Yes** (code 3516) |
| Raw ERC-20 | `CREATE`/`CREATE2` | No factory | Catch via new-contract + then `PoolCreated`/`Initialize` |

Clanker v4 event (from source): `TokenCreated` with `msgSender`, `tokenAddress`, `tokenAdmin`, metadata, `poolHook`, etc.

### 3.2 Base funnel (order-of-magnitude)

We do **not** have a Pump.fun-quality tick corpus for Base. Honest estimate:

- Launch *rate* is far below Solana/RH. Clanker 24h fees ~$10k vs Pump $1.95M.
- Many Zora coins are **content coins**, not tradable memes with meme-trader flow.
- Clanker/Zora/Flaunch tokens are usually **not classic honeypots**; the failure mode is **creator dump / thin LP / social fake volume**.
- Useful sample for H1/H2 on Base will take **longer calendar time** than Solana. That is acceptable because Base is the contract-analyzer proving ground.

---

## 4. Current Robinhood Chain ecosystem (August 2026)

**This is the largest V1 correction.**

### 4.1 Independently verified (this session, public RPC `https://rpc.mainnet.chain.robinhood.com`)

| Fact | Result |
|---|---|
| `eth_chainId` | `0x1237` = **4663** |
| Latest block (sample) | ~47,289,024 |
| Block time | **~101 ms** (5000 blocks = 506s) |
| `eth_gasPrice` | `0x21c8ed0` ≈ 0.035 gwei (tiny) |
| Pons V2 factory bytecode | **24,177 bytes** at `0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e` |
| `TokenLaunched` last ~33 min (20,000 blocks) | **83 events ≈ 149 launches / hour ≈ 3,600 / day** |
| `LaunchSwept` same window | **0** |
| `PoolGraduated` same window | **0** |
| Pons V1 `TokenLaunched` same window | **0** (V1 quiet now) |
| Uniswap LiquidityLauncher logs same 5k-block slice | 5 (Pools.trade still alive, not at Aug 5 mania) |

So: **Pons V2 is a high-frequency launch factory with almost no graduations in a random half-hour.** That is a launchpad, not a liquid market. It is still a first-class *discovery* chain.

### 4.2 What the chain is

- Arbitrum Orbit L2, ETH gas, mainnet **2026-07-01**.
- **No public mempool** (centralized sequencer). Sniping via pending-tx feed does not exist. Sequencer feed: `wss://feed.mainnet.chain.robinhood.com` (Robinhood docs).
- Uniswap v2/v3/v4 and UniswapX live from day one.
- Tokenized stocks (AAPL, NVDA, TSLA, …) and USDG exist as quote assets. Pons V2 can quote those.
- Explorers: `robinhoodchain.blockscout.com` and `explorer.mainnet.chain.robinhood.com` (docs disagree on hostname; both Blockscout-class).
- Alchemy is the **officially recommended** RPC (`robinhood-mainnet.g.alchemy.com`). Public RPC is rate-limited and already painful at 11.6M daily tx claims.

### 4.3 Launchpads — Pons is not the whole chain

Dune `@0x_emerson` 30d token-create attribution (treat **counts as upper bounds**; “Other / Unlabeled” is mostly spam factories):

| Pad | 30d tokens (dashboard) | Avg/day | Factory (Dune map) | Bytecode on 2026-08-27 |
|---|---|---|---|---|
| Other / unlabeled | 1,367,749 | 45,592 | 60k factories | spam |
| Flap | 478,257 | 15,942 | `0x26605f322f7ff986f381bb9a6e3f5dab0beaeb09` | **yes** (2840) |
| Pons (map; mixed V1/V2) | 114,332 | 3,811 | V1 `0xa5aab3f0c6eeadf30ef1d3eb997108e976351feb`; V2 `0x7ed598bc…` | **yes** |
| Trench | 93,522 | 3,117 | `0x2ecfb98bce4f3616115e4a2a7a2379af388dfbaa` | **yes** (758) |
| Bankr / Long.xyz | 48,009 | 1,600 | `0x1b37d3a72082029c44b35b604ea473617580b69a` | **yes** (1912) |
| Virtuals | 9,454 | 315 | `0x43e4c17b15365596caae8e7d00e42bc8e988c2d4` | **yes** (1167) |
| Clanker | 1,135 | 38 | `0xd3f2cc1731b7fd17f28798835c2e02f0a1839a94` | **yes** (12070) |
| Pools.trade (Uniswap) | mania on Aug 5–6 (10.5k then 11.6k / day) | bursty | LiquidityLauncher `0x0000FffFBE8efE702c8703aE3477FF5dE3d319C0` | **yes** (4127) |

AltStreet (Jul 3–19): **150,924** distinct tokens in 17 days (~8,900/day); **50.2% had zero swaps**. NOXA.fun printed 60k pools then **halted**. Pons opened ~Jul 13 and hit 15,401 pools in a day. That is a spam-and-die market.

MemeFees 24h: **Pons $20.4M vol / $1.48M fees (28.7% global launchpad fee share)**. Fee share can be high even when graduation is rare, because curve trading + snipe tax is the product.

### 4.4 How Pons actually works (V2) — Bitquery + verified contracts

This is the important on-chain object.

**Lifecycle**

1. Factory or router mints **1,000,000,000** (18 decimals) into a **per-token bonding curve**.
2. Traders `CurveBuy` / `CurveSell` against that curve. 1% curve fee typical (`curveFeeBps` per launch). Creator tax extra, capped by factory.
3. **Snipe tax** on early buys: at current factory settings, **9900 bps in second 0, 618 bps second 1, 19 bps second 2, then 0** (`snipeTaxStartBps=9900`, `snipeTaxSeconds=3`). Creators can pre-exempt wallets (`SnipeTaxExempted`).
4. Graduation threshold: **4.2 ETH** for native-quoted launches (other quote assets have their own threshold).
5. Permissionless two-phase graduate: `graduate` (`LaunchSwept` / `CurveCompleted`) then `createGraduatedPool` (`PoolGraduated`). Gap of seconds–minutes where the token is **swept but not tradeable**.
6. Uniswap v4 pool: **fee=0, tickSpacing=200, hooks=PonsV2MemeHook**. Real fees taken by the hook. LP **permanently locked**. Extra **4/49 of supply** locked in `PonsV2LaunchLocker`.

**Supply split (fixed)**

| Slice | Share |
|---|---|
| Sold on curve | 5/7 |
| Seeds v4 pool | 10/49 |
| Permanently locked | 4/49 |

**Pons V1** (`0xa5aab3f0…`) is a **different protocol**: no curve, Uniswap **V3** pool at launch, still deployed historically, quiet in our 33-minute sample. A Pons feed that only watches V2 **misses V1**.

**Pools.trade** is the opposite design: Uniswap v4 from block one, no graduation, hook `0x0`, fee 2500.

### 4.5 Is RH first-class?

**Yes for discovery and measurement. Not yet for live trading capital.**

Reasons to include it:

- Launch density comparable to Pump.fun (~thousands/day on Pons V2 alone; tens of thousands if you count Flap/Trench spam).
- Structured events (`TokenLaunched`, `CurveBuy`, `LaunchSwept`) — we do not need DexScreener.
- No public mempool → sniper war is a sequencer-proximity game, which we are **not** playing. Confirmation strategy fits.
- Same EVM security stack as Base.
- Possible **information advantage**: tooling is immature vs Solana; GMGN/Axiom coverage is thinner.

Reasons not to trade it live yet:

- 50%+ tokens never swap (early-chain study).
- Graduation in our sample: **0 / 83** in ~33 minutes. Almost all “markets” die on the curve.
- Historical depth is ~8 weeks. Non-stationary (NOXA halt, Pools.trade fee war, Pons V2).
- Public RPC will not carry a production collector. Alchemy/QuickNode is mandatory.
- Tokenized-stock quote pairs add complexity and probably not meme-trader flow.

---

## 5. Launch frequency comparison

| Metric | Solana | Base | Robinhood Chain |
|---|---|---|---|
| New tokens / hour (order of mag.) | Pump.fun ~800–900/hr in mid-2026 corpus; plus FOMO/LaunchLab | Tens, not hundreds, across Clanker/Zora/Flaunch | **Pons V2 ~150/hr (measured 2026-08-27)**; Flap/Trench can dwarf that |
| New tokens / day | Pump.fun ~20k; chain much higher with all pads | Hundreds–low thousands | Pons ~3.6k measured; Dune 30d Pons ~3.8k; all pads >> 10k |
| Meaningful liquidity | Rare; graduation 0.2–0.7% | Higher *fraction* of Clanker/Zora launches have locked LP by design | Pons graduation **very rare** at 4.2 ETH; V1/Pools.trade have instant pools, usually dust |
| Survive 15 min with flow | Small % | Unknown; collect | Unknown; collect. AltStreet: 50% never swap **at all** |
| Rug / scam style | Bundles, dumps, Token-2022 | Template + creator dump; plus raw ERC-20 honeypots | Mix: template pads (safer contracts) + unlabeled ERC-20 factories (hostile) |
| Tx cost | Tips dominate | Low (gas ~0.006 gwei in sample) | Very low (~0.035 gwei) |
| Block / slot | ~400 ms | ~2 s (+ Flashblocks) | **~100 ms** |
| MEV | Jito auctions | Public mempool + builder; Flashblocks | **No public mempool**; sequencer privileged |
| Historical data | Best | Good (Basescan, Dune) | Weak; chain born Jul 2026 |
| Real-time data | Best (gRPC) | Alchemy/QN WS `logs` | Alchemy WS + official sequencer feed |
| Bot competition | Extreme | Medium | Rising, less mature terminals |

**Funnel sketches (illustrative, labeled)**

Solana Pump.fun (corpus-backed):

```
100,000 launched
    → ~700 graduate (0.7% optimistic)
    → ~110 not high-risk by MELT-like label (~16% of grads)
    → tens pass confirmation
    → single-digit trade candidates / 100k
```

Robinhood Pons V2 (measured rate + graduation 0 in 33 min; **do not treat 0 as the true rate**, only as “graduation is rare”):

```
3,600 / day launched
    → almost all never hit 4.2 ETH
    → a thin set of curve-active names
    → fewer still graduate
    → security + confirmation leaves a handful
```

Base Clanker/Zora:

```
low hundreds / day of *real* pad tokens
    → most have locked LP (contract-safe-ish)
    → still mostly dead socially
    → confirmation set is small but cleaner
```

---

## 6. New-token discovery architecture

```
                 MULTI-CHAIN DISCOVERY
        ┌──────────────┬──────────────┬──────────────┐
        │   SOLANA     │     BASE     │ ROBINHOOD    │
        │ Yellowstone  │ eth_subscribe│ eth_subscribe│
        │ program txs  │ logs + heads │ logs + heads │
        └──────┬───────┴──────┬───────┴──────┬───────┘
               │              │              │
               ▼              ▼              ▼
          chain adapters (decode to TokenDiscovered)
               └──────────────┬──────────────┘
                              ▼
                    NORMALIZED TOKEN EVENT
                              ▼
                      SECURITY ENGINE
                       (chain-specific)
                              ▼
                         DATA ENGINE
                              ▼
                      OPPORTUNITY ENGINE
                              ▼
                       STRATEGY ENGINE
```

**Do not use DexScreener trending as a trigger.** Use it as a *negative control*: “would retail have seen this yet?”

**Mempool:** not for V1 confirmation strategy. Base pending txs help snipers, not us. RH has no public mempool. Skip.

---

## 7. Exact launchpads / factories / events to monitor

Addresses below are **checksum-insensitive in EVM**. Solana addresses are case-sensitive.

### 7.1 Solana — VERIFIED (multiple independent catalogs)

| Program | Address | Watch |
|---|---|---|
| Pump.fun curve | `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P` | `create`, `create_v2`, `buy`, `sell`, `migrate` |
| PumpSwap AMM | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | pool create, swap |
| Pump Fees | `pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ` | fee config |
| Raydium LaunchLab | `LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj` | `initialize_v2`, migrate_*; LetsBonk if accounts include `FfYek5vEz23cMkWsdJwG2oa6EphsvXSHrGpdALN4g6W1` |
| Meteora DBC | `dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN` | `initialize_virtual_pool_with_spl_token`, migrate_* |
| Raydium AMM v4 | `675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8` | `initialize2` |
| Raydium CPMM | `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C` | pool create |
| Raydium CLMM | `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK` | pool create |
| Meteora DLMM | `LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo` | **note:** some catalogs differ on last chars; confirm against official IDL before coding |
| Meteora DAMM v2 | `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG` | |
| Moonshot | `MoonCVVNZFSYkqNXP6bxHLPL6QQJiMagDL3qcqUQTrG` | `migrateFunds`, `tokenMint` |
| Boop | `boop8hVGQGqehUK2iVEMEnMrL5RbjywRzHKBmBE7ry4` | `graduate` |
| Heaven | `HEAVENoP2qxoeuF8Dj2oT1GHEnu49U5mJYkdeC8BAX2o` | `create_standard_liquidity_pool` |

**NEEDS VERIFICATION before coding:** FOMO on-chain program ID (large fee share; do not guess). Meteora DLMM suffix variants. Pump `migrate` vs `complete` instruction name on current IDL.

**Transport:** Helius LaserStream / Yellowstone `transactions` filter on those program IDs. Account subscribe on bonding-curve PDAs for watchlist.

### 7.2 Base — VERIFIED unless noted

| Source | Address | Event / method |
|---|---|---|
| Clanker v4 factory | `0xE85A59c628F7d27878ACeB4bf3b35733630083a9` | `TokenCreated` / `deployToken` |
| Clanker v3.1 | `0x2A787b2362021cC3eEa3C24C4748a6cD5B687382` | older TokenCreated |
| ZoraFactory (proxy) | `0x777777751622c0d3258f214F9DF38E35BF45baF3` | `deploy`, `deployCreatorCoin`, `deployTrendCoin` + CoinCreated-style logs (**confirm topic0 from current impl**) |
| Flaunch | `0x516af52d0c629b5e378da4dc64ecb0744ce10109` | Flaunch-specific create |
| Uniswap v3 factory | `0x33128a8fC17869897dcE68Ed026d694621f6FDfD` | `PoolCreated(token0,token1,fee,tickSpacing,pool)` |
| Uniswap v4 PoolManager | `0x498581fF718922c3f8e6A244956aF099B2652b2b` | `Initialize` |
| Uniswap LiquidityLauncher | `0x00004c4ccc709Ef590F7C81102C0689F0263D4e9` | launch events |
| Aerodrome PoolFactory | `0x420DD381b31aEf6683db6B902084cB0FFECe40Da` | `PoolCreated` |
| Virtuals bonding curve | `0x1A540088125d00dD3990f9dA45CA0859af4d3B01` | curve trades / creates |
| WETH | `0x4200000000000000000000000000000000000006` | quote |

**NEEDS VERIFICATION:** Uniswap V2 factory on Base (cited `0x8909Dc15…` empty). Bankr current deployer (old one empty). Doppler live factory set. Clanker TokenCreated topic0 from latest ABI.

**Transport:** Alchemy/QuickNode `eth_subscribe("logs")` on the factory list. `newHeads` only as a heartbeat. Contract-creation traces: **not V1** (expensive); catch raw ERC-20 when a new pool references an unknown token, then backfill `eth_getCode` + creator tx.

### 7.3 Robinhood Chain — VERIFIED this session unless noted

| Role | Address | Events |
|---|---|---|
| **PonsV2LaunchFactory** | `0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e` | `TokenLaunched` topic0 `8d4aad4953d0ca700d468f3753aa14432d1b35b43ec6409f051fb6aa43a89607`; `LaunchSwept`; `PoolGraduated` |
| **PonsV2LaunchAndBuy** | `0xe33e9e479df8802cb0866d5d05258bec4cf62948` | `Launched`; selectors `launchAndBuy=0xf85f8e41` |
| Factory launch selectors | | `launchToken=0xf35abbcf`, `0xa72101af` |
| **PonsV2MemeHook** | `0xe5e702641ea86f4ae6cc3cdaed2b886f976be044` | `PoolRegistered` topic0 `01bf263a…` |
| **PonsV2LaunchLocker** | `0x267444d099b10fb5ed7c3cc7b7c767adca574952` | lock events |
| Graduation executor | `0xc7819b64a1daecd7ec19856d026cb14efbd89046` | `GraduationDustSwept` |
| Bonding curve | **one per token** (mint receiver) | `CurveBuy` `ec36bf57…`, `CurveSell` `8113d738…`, `SnipeTaxCharged` |
| Uniswap v4 PoolManager | `0x8366a39cc670b4001a1121b8f6a443a643e40951` | `Initialize` (filter hooks == Pons hook) |
| **Pons V1 factory** | `0xa5aab3f0c6eeadf30ef1d3eb997108e976351feb` | `TokenLaunched` topic0 `db51ea9a…` |
| Pools.trade LiquidityLauncher | `0x0000FffFBE8efE702c8703aE3477FF5dE3d319C0` | Uniswap Liquidity Launchpad v3.2 |
| Flap portal | `0x26605f322f7ff986f381bb9a6e3f5dab0beaeb09` | TokenCreated (decode ABI) |
| Trench | `0x2ecfb98bce4f3616115e4a2a7a2379af388dfbaa` | |
| Bankr/Long | `0x1b37d3a72082029c44b35b604ea473617580b69a` | |
| Virtuals RH | `0x43e4c17b15365596caae8e7d00e42bc8e988c2d4` | |
| Clanker RH (Dune) | `0xd3f2cc1731b7fd17f28798835c2e02f0a1839a94` | **not** the Base CREATE2 factory |
| Virtuals RH curve | `0xd4cCBFA37e2f35611b3042e4096Ad7a3459Bd007` | whitepaper; **confirm before coding** |
| USDG | `0x5fc5360d0400a0fd4f2af552add042d716f1d168` | quote |

Curve buy/sell topic0s and factory topic0s: Bitquery documents keccak preimage match **and** live occurrence. We additionally saw `TokenLaunched` live via `eth_getLogs`.

**Transport:** Alchemy WS `logs` on the factory set. Do not rely on the public RPC for production subscribe. Optional: sequencer feed for preconfirmations later — **not V1**.

**Ordinary ERC-20 + non-Pons pools:** subscribe Uniswap v4 `Initialize` and v3 `PoolCreated` (find RH Uniswap v3 factory — **NEEDS VERIFICATION**). If `token` is unknown, run the EVM security pipeline.

---

## 8. Solana security architecture

Keep V1’s fast path. Source of truth = **chain**, not RugCheck.

**Hard reject**

- Mint authority set
- Freeze authority set
- Token-2022: transfer hook, permanent delegate, confidential transfer (until we have a proven sell path)
- Transfer fee > configured max (even if “legit”)
- Sell simulation fails (Pump ix or Jupiter)
- Creator on serial-rug list
- Bundle-merged creator+insider supply ≥ threshold (prior 40%)
- LP not burned/locked on non-PumpSwap pools we would trade

**Compute ourselves**

- Authorities, extensions, supply, creator ATA
- Curve real SOL vs virtual
- Top-k holders + Jito/co-sign/funder clustering (MELT method)
- Wash fraction (same-tx buy+sell, ping-pong)
- Creator net flow
- Metadata hash / ticker collision

**External = evidence**

RugCheck, Solana Tracker `risk`, Jupiter Shield, Birdeye. Timeout → **fail closed** for first live/paper entries.

Sub-scores: `CONTRACT_RISK` (authorities/extensions), `LIQUIDITY_RISK`, `CREATOR_RISK`, `HOLDER_RISK`, `MARKET_MANIPULATION_RISK` → `RISK_SCORE`.

---

## 9. EVM scam-contract architecture

Applies to **Base and Robinhood**. Never reuse Solana assumptions.

A Clanker/Pons/Zora token can be contract-safe and still a financial rug. Split scores the same way.

### 9.1 Fast path (T+50–500ms) — template recognition

```
if factory in KNOWN_FACTORY
   and runtime_bytecode_hash in KNOWN_IMPLEMENTATION
   and not proxy_upgradeable_token   # Zora factory IS upgradeable; token may not be
then CONTRACT_RISK := LOW_TEMPLATE
else CONTRACT_RISK := UNKNOWN → full analyzer
```

Still run creator/holder/liquidity. Template ≠ investable.

Maintain:

```
KNOWN_FACTORY
KNOWN_IMPLEMENTATION
KNOWN_BYTECODE_HASH
KNOWN_POOL_FACTORY
KNOWN_ROUTER
KNOWN_HOOK
```

Verified starters: Clanker v4 factory + ClankerToken impl hash (compute from first deploys), Pons V2 factory/hook, Zora factory (flag **factory upgradeable**), Flaunch, Uniswap v3/v4, Aerodrome factory, Pons V1 factory.

### 9.2 Full analyzer (unknown bytecode)

See §§10–12.

---

## 10. Honeypot simulation

**This is the only “can we leave” proof.** Static flags lie; taxes hide in `extcodesize` branches.

Sequence against **current state** (fork, not a stale cache):

```
1. Fund a fresh attacker EOA on a fork (Anvil / revm)
2. BUY quote→token via the real router/curve (exact ix we would send)
3. Measure tokens received vs quoted → effective_buy_tax
4. TRANSFER token to a second fresh EOA
5. SELL 50% from buyer
6. SELL remainder from buyer
7. SELL from the second EOA (blacklist / only-holder traps)
8. Record reverts, gas spikes, token deltas
```

Measure: expected vs actual, buy tax, sell tax, revert reason, gas.

**Backends**

| Tool | Use |
|---|---|
| `eth_call` | Fast, often enough for simple revert |
| `debug_traceCall` | See hidden branches; need provider support (Alchemy Debug on supported chains — **confirm RH**) |
| Local Anvil fork | Best fidelity for multi-step; V1 for ELIGIBLE tokens, not every spam create |
| Tenderly | Optional; cost and vendor |
| Honeypot.is / GoPlus | Parallel signal, never sole gate |

**Pons curve:** simulation is a curve buy then sell on **that token’s curve contract**, not Uniswap, until graduated. After `LaunchSwept` and before `PoolGraduated`, **cannot sell** — treat as emergency/expired if we somehow hold.

**Do not simulate every Flap spam token at Anvil depth.** Funnel: template or cheap `eth_call` sell → only then fork.

---

## 11. Static contract analysis

Layer 1, for non-template tokens:

- If verified (Blockscout/Basescan): parse ABI, `Ownable`, `AccessControl`, tax vars, `blacklist`, `maxTx`, `tradingOpen`, `uniswapV2Pair`, router.
- If unverified: selectors from bytecode (`CAST` 4-byte), compare to known scam selector sets (`setTax`, `excludeFromFee`, `setBlacklist`, `enableTrading`, `setMaxTx`, `airdrop`, `manualSwap`).
- Compiler metadata stripped hash (Swarm/CBOR) for family matching.
- `DELEGATECALL`, `SELFDESTRUCT`, arbitrary `CALL` to storage slot addresses.
- Similarity vs our `bytecode_hash` DB (see §16).

Limitations: source can be fake-verified; proxy impl can change; obfuscation exists. Static is a **filter and a fingerprint**, not a proof of safety.

---

## 12. Proxy / privilege analysis

Must answer:

| Question | How |
|---|---|
| Proxy? | EIP-1967 slots `0x360894a13b…` (impl), `0xb53127684a…` (admin); EIP-1822; beacon; minimal proxy 0x363d3d373d… |
| Implementation verified? | getCode(impl), Blockscout |
| Upgrade admin? | admin slot, `AccessControl` `DEFAULT_ADMIN_ROLE`, `Timelock` |
| Privileged roles | `owner()`, `getRoleMember`, pauser, minter, fee setter, operator |
| Fake renounce | `owner()==0` but `feeOperator` or hidden `isFeeExempt` admin remains |
| External dependency | tax/router stored as address we don’t control |

**Zora:** factory is upgradeable. Treat newly deployed coins as template-safe only after hashing **token runtime**, not factory.

**Pons/Clanker tokens:** typically not user-upgradeable; still check.

Hard reject: upgradeable token **or** upgradeable tax/router dependency **unless** admin is a known locker/timelock we accept (almost never for memes).

---

## 13. Liquidity-rug detection

| Chain | Classic LP pull | What to watch |
|---|---|---|
| Pump.fun → PumpSwap | LP burned | Soft rug (creator dump). Curve SOL drain is the pre-grad rug. |
| Clanker / Zora / Flaunch | LP in locker/hook | Locker admin, fee recipients, vault unlock (Clanker vault min 7d) |
| Pons V2 post-grad | Permanently locked | Hook fee/tax changes if owner-mutable; swept-but-not-graduated halt |
| Pons V1 / Pools.trade / raw Uni | Possible if locker missing | LP NFT owner, `decreaseLiquidity`, `collect`, pair `skim` |
| Aerodrome / Uni v3 | NFT positions | Position manager owner |

Always track quote reserves vs our position. Emergency exit if reserves drop > threshold in one block.

---

## 14. Creator reputation

Tables: `creators`, `creator_tokens`, `creator_outcomes`.

Per creator (chain-scoped, then linked):

- n_launches, n_graduated, n_rugged (label: liquidity gone or −80% in 20m with creator sell)
- median unique buyers, median 15m volume
- last rugged_at
- factory mix (only Pons vs also raw ERC-20)

**Hard signal:** 24 rugged / 27 launched → reject #28.

**Solana:** creator = tx signer of `create`; cluster via funder + Jito.

**EVM:** `tx.from` of factory call (not inner `launchTokenFor` — Bitquery warns inner call mis-attributes). Funding: first inbound ETH/USDC. Same EOA on Base and RH is the same person.

---

## 15. Wallet clustering

| Method | Solana | EVM |
|---|---|---|
| Co-sign same tx | Yes (MELT) | Yes (multicall / same tx multi-buy) |
| Common funder | Yes (rent-exempt funder; exclude CEX) | Yes (first funder; exclude bridges/CEX) |
| Jito bundle ID | Yes | n/a |
| CREATE2 factory salt families | rare | Yes |
| Same EOA cross-chain | no (different key) | **Base ↔ RH trivial** |
| Bridge in from other chain | hard | Watch official RH/Base bridges for funded-from-Solana (harder; later) |

Store `wallet_cluster_id`. Holder concentration **after merge** is the feature (MELT: +24pp on high-risk).

---

## 16. Contract fingerprinting

Goal:

```
NEW CONTRACT
  → 98% similar to 42 rugged contracts
  → HARD REJECT
```

Fingerprints:

1. Exact `keccak256(runtime_bytecode)`
2. Normalized bytecode (strip immutables/metadata)
3. 4-byte selector set Jaccard
4. Opcode n-grams
5. Proxy impl hash
6. Factory + creator cluster

This is **defensible proprietary data** if we label outcomes ourselves. Start exact-hash + selector Jaccard; do not build a Ghidra farm in phase 1.

Known-good template hashes (ClankerToken, Pons token impl) go on an **allow** list so we do not reject every Clanker as “similar to each other.” V1 research already warned: bytecode sameness is **expected** on factories.

---

## 17. Cross-chain smart money

EVM: one scoring identity per EOA across Base + RH.

```
Wallet 0xabc
  Base: 43 young-meme trades, +182% realized, insider_score LOW
  RH:   16 trades, +64%, repeatable
```

Filters (same as V1, enforced harder):

- Not the token creator / fee recipient / snipe-tax-exempt list
- Not slot-0 / first-block only
- Profit not 80% one token
- Hold time not <15s extractive (unless we are studying snipers as a *risk* feature)
- Appears on ≥ N distinct factories

Solana wallets stay separate until we have a bridge-link experiment (later, low priority).

Do not ingest GMGN lists as ground truth.

---

## 18. Normalized multi-chain data model

```text
TokenDiscovered {
  chain                 // solana | base | robinhood
  token_address
  creator
  launchpad             // pumpfun | clanker_v4 | pons_v2 | zora | ...
  factory
  pool                  // optional at T+0
  curve                 // optional
  quote_asset
  discovered_block      // EVM
  discovered_slot       // Solana
  discovered_at
  initial_liquidity     // nullable
  launch_mechanism      // bonding_curve | locked_v4 | uni_v3 | raw_erc20 | ...
  bonding_curve         // bool
  graduation_model      // pump_amm | pons_v4_hook | none | unknown
}
```

Adapters enrich. **Security remains chain-specific.** Opportunity features share names where they mean the same thing (unique buyers, buy/sell ratio) and stay native otherwise (`curve_progress`, `snipe_tax_window`, `clanker_extension_bps`).

---

## 19. Opportunity model

Shared (after security pass):

- unique_buyer_acceleration, unique_seller_acceleration
- buy_sell_ratio, volume_acceleration (wash-adjusted)
- liquidity, liquidity_growth, holder_growth
- top_holder_change (cluster-merged)
- creator_net_flow
- smart_money_flow
- wash_trade_score
- token_age, mcap, exit_depth

Solana extra: curve_progress, bundle_supply, Jito activity, graduation proximity.

Base extra: Clanker extension allocation, vault unlock, Uni v4 hook fee, Zora coin type (content vs trend).

RH extra: Pons curve progress vs 4.2 ETH, snipe-tax window elapsed, `LaunchSwept` vs `PoolGraduated` gap, quote asset (ETH vs stock token — **stock-quoted memes are a separate bucket, probably skip V1**).

---

## 20. Infrastructure

### Solana

Helius LaserStream Business ($499) + backup RPC + Jito + Jupiter. Unchanged from V1 recommended.

### Base

| Provider | Role |
|---|---|
| **Alchemy** or **QuickNode** | WS `logs`, archive, `debug_traceCall` |
| `mainnet.base.org` | Dev only, HTTP, no WS |
| Flashblocks (optional) | Not needed for confirmation |

### Robinhood Chain

| Provider | Role |
|---|---|
| **Alchemy** (official) | `https://robinhood-mainnet.g.alchemy.com/v2/KEY`, WSS same host |
| QuickNode / dRPC / Validation Cloud | backup |
| Public `rpc.mainnet.chain.robinhood.com` | **dev probes only** (we used it; it will flake) |
| Sequencer feed `wss://feed.mainnet.chain.robinhood.com` | later |
| Blockscout | verification, holders UI |
| Bitquery `EVM(network: robinhood)` | research backfill; decoded Pons from **2026-08-14** |

**Mempool infra: do not buy.** RH has none. Base pending is for snipers.

**Simulation:** Anvil on a box that can fork Base + RH. Confirm Alchemy `debug`/`trace` on RH before depending on it.

---

## 21. Database changes (vs V1)

Keep V1 tables. Add:

**chains / launchpads / factories / implementations** — registry.

**contracts**

`chain, address, bytecode_hash, normalized_hash, factory, implementation, proxy_type, upgrade_admin, creator, first_seen, flags jsonb`

**contract_analyses**

`contract_id, as_of_block, method (static|state|sim|goplus), effective_buy_tax, effective_sell_tax, honeypot, revert_reason, roles jsonb`

**token_state** (candidate SM)

`token_id, state, reason, entered_at`

**creators / creator_links** (cross-chain EOA)

**wallet_scores** with `chain` nullable for EVM-global.

**paper_trades / live_trades** must include `chain, launchpad, contract_risk, creator_risk, holder_risk, liquidity_risk, entry_latency_ms, execution_model`.

Every rejection stored. Later: “what happened to REJECTED_HONEYPOT tokens?”

---

## 22. Dashboard changes

One scanner, three chains:

```
CHAIN  AGE   TOKEN  SOURCE   RISK  CONTRACT  LIQ    OPP  STATUS
SOL    43s   ABC    Pump     18    SAFE      $31k   82   WATCH
BASE   21s   XYZ    Clanker  91    DANGER    $8k    --   REJECT
RH     58s   PON    PonsV2   23    SAFE      $17k   77   WATCH
```

Token page tabs: Security (chain-specific), Market, Intelligence, Decision.

Paper vs live labeled. Breakdown filters: chain × launchpad × reject reason.

---

## 23. Backtesting changes

- Solana: keep event-level curve simulator (V1 §16). HuggingFace corpus still the first historical test.
- Base/RH: **no reliable tick corpus**. Do not fake candle backtests. **Prospective collection is the backtest.** Optionally Bitquery archive for Pons from 2026-08-14 (decoded) / topic0 for earlier.
- Simulate snipe tax on Pons if we ever test sub-3s entries (we should not, in V1).
- Graduation halt window on Pons must be modeled (unsellable).
- Costs: Base/RH gas is negligible; DEX fees and impact are not. Pons 1% + creator tax + hook fees.

---

## 24. Multi-chain paper-trading plan

One engine, three adapters, **same strategy code** (filtered confirmation) with per-chain min-liquidity and fee models.

Each fill stores chain + launchpad. Weekly report:

```
SOLANA vs BASE vs ROBINHOOD
Pump.fun vs Clanker vs PonsV2 vs Flap vs Pools.trade
```

Possible outcome: only Solana has EV; RH is a data goldmine for H1 (scam detection) but H3 fails because nothing liquid survives. That is a **successful experiment**.

Run all three collectors even if we only paper-trade Solana + Pons ETH-quoted + Clanker.

---

## 25. Cost analysis (monthly, USD, 2026)

| | DEVELOPMENT | PAPER (recommended) | SMALL LIVE | HIGH PERF |
|---|---|---|---|---|
| Solana gRPC (Helius Business) | 49 (dev, weak) | **499** | 499 | 999+data |
| Base RPC/WS | 0 public | Alchemy pay-as-you-go ~50–150 | 150–300 | 400+ |
| RH RPC/WS | 0 public | **Alchemy ~50–200** (high log volume) | 200–400 | 500+ |
| Compute (3 collectors + Anvil) | 40 | **120–200** | 200 | 500 colo |
| Postgres | 0 | 40 | 80 | 150 |
| GoPlus | 0 | 0–199 | 199 | 399 |
| RugCheck / Tracker | 0 | 0–55 | 55 | 430 |
| Birdeye | 0 | 0–39 | 39 | 199 |
| **Infra total** | **~90–150** | **~800–1,400** | **~1,200–2,000** | **$4k–8k** |
| Trading fees | 0 | 0 | chain-dependent | tips+impact |

RH log volume can surprise (100ms blocks, thousands of creates/day). Budget Alchemy CUs explicitly in week 1.

Do not buy shreds, Flashblocks, or sequencer colocation until paper shows **latency**, not rugs, is the loss source.

---

## 26. Development roadmap

Phases are still **discovery → data → security → measurement → (maybe) trading**.

| Phase | Objective | Success | Failure |
|---|---|---|---|
| **0** | Accept V2 scope | This doc reviewed | Insist on sniping RH day one |
| **1** | Repo + schema + `TokenDiscovered` | Fixtures decode Pump create, Clanker TokenCreated, Pons TokenLaunched | Guessed addresses |
| **2** | Solana collector (as V1) | 7d continuous, lag p95 <1s | Gaps |
| **3** | EVM log collector Base+RH | Pons V2 creates match `eth_getLogs` counts ±5%; Clanker creates match Basescan sample | Public RPC only in “prod” |
| **4** | Security fast path | Templates classified; unknown ERC-20 go to analyzer; Solana authorities | Fail-open on timeout |
| **5** | EVM sim + static | Known honeypot corpus rejected; Clanker template not flagged as honeypot | Simulator false-positives on every Uni v4 hook |
| **6** | Candidate SM + snapshots | Reject reasons stored; 15s features | Look-ahead in features |
| **7** | EXP001 Solana corpus (V1) | H1/H2 written | Skip because “chain is live” |
| **8** | EXP002 prospective 2–4w all chains | H1–H6 scored | Trade live to “get data” |
| **9** | Paper strategy B | Same gates as V1 | — |
| **10** | Tiny live if paper +EV **per chain** | Caps, kill switch | Combined PnL hiding RH disaster |

---

## 27. First experiments

Keep **EXP001** (Pump.fun corpus) exactly as V1.

**EXP002 — Prospective three-chain collector (start as soon as Phase 3 works)**

Collect 2–4 weeks:

- All Pump.fun creates + trades + migrates
- All Clanker v4 + Zora factory creates on Base (plus Uni v3 PoolCreated for unknown tokens — sampled if too hot)
- All Pons V2 TokenLaunched + CurveBuy/Sell + Sweep/Graduate; sample Flap if CU explodes

Questions:

| ID | Question |
|---|---|
| **H1** | Can early features predict dump/honeypot/rug? (Solana: 5m tape. EVM: sim + creator + concentration.) |
| **H2** | Do rejected tokens perform worse after rejection? (Must store rejects.) |
| **H3** | Among security-passed, does confirmation momentum have post-cost +EV? |
| **H4** | Does creator/wallet reputation add lift on top of H1? |
| **H5** | Which chain/launchpad has the best risk-adjusted opportunity? |
| **H6** | Does discovering at T+0 (vs DexScreener first-seen) improve T+30s features / decisions even if entry is T+30s+? |

H6 is the justification for early discovery without sniping.

---

## 28. Kill criteria

All V1 kill criteria remain. Add:

1. RH collector cannot stay synced at reasonable Alchemy cost (CU > $1k/mo for logs we don’t use).
2. After 4 weeks, RH security-passed confirmation EV ≤ 0 **and** Base same → drop those chains from trading, keep Solana if H3 holds.
3. EVM simulator cannot distinguish known honeypots from Clanker templates (tooling failure).
4. >90% of RH “candidates” are unsellable or sub-dust so H3 is untestable — then RH is **WATCH**, not BUILD, for trading.
5. We start sniping Pons second-0 despite 99% snipe tax — process kill, not market kill.

---

# Deliverables

## CHAIN VERDICT

### SOLANA: **BUILD**

Still the only chain with depth, historical corpora, and a measured (brutal) funnel. Primary paper-trading venue. Competition is the tax we pay for liquidity.

### BASE: **BUILD**

Not because it will print money. Because (1) EVM security cannot be invented on RH’s 8-week chaotic history alone, (2) Clanker/Zora/Flaunch are clean template labs, (3) EOAs overlap RH. Trading allocation stays small until H5 says otherwise. Volume is ~2–3% of launchpad fees; do not expect it to dominate PnL.

### ROBINHOOD CHAIN: **BUILD** (discovery + security + paper) / **WATCH** (live capital)

Independently verified: live, fast, Pons V2 ~150 launches/hour, graduations rare, tooling immature, no mempool. That is a **measurement edge** if we collect now. It is not a mandate to buy 3,600 tokens a day. Live size = 0 until EXP002 H3/H5 on *our* data.

## PRIORITY

1. **Solana** — P&L hypothesis and EXP001.
2. **Robinhood Chain** — collect while the market is young; Pons V2 is the cleanest RH object.
3. **Base** — EVM analyzer proving ground + Clanker/Zora; lower launch density.

#3 is not excluded.

## EXACT DISCOVERY SOURCES

See §7. Do not invent addresses. Recheck FOMO program, Base Uni V2 factory, Bankr deployer, RH Uni V3 factory, Meteora DLMM suffix, Virtuals RH curve, Clanker/Zora topic0s from current ABIs **in the first coding phase fixtures**.

## EXACT SECURITY PIPELINE

```
TOKEN FOUND
    → identify factory/program/template          (50–500ms)
    → FAST SECURITY
         Solana: authorities, extensions, creator cache
         EVM: template hash OR selector red-flags OR proxy
    → if HARD FAIL → REJECTED_* (store)
    → SCAM CONTRACT (EVM unknown only): static + storage
    → LIQUIDITY: vaults / locker / curve SOL
    → CREATOR: history + cluster
    → HOLDERS: top-k + merge
    → SELLABILITY: simulate buy/transfer/sell
    → RISK_SCORE (max of sub-scores, hard gates)
    → REJECT | WATCH | ELIGIBLE
ELIGIBLE → opportunity features → CONFIRMING → paper only
```

A 10x tape with freeze authority or 99% sell tax never reaches opportunity.

## V1 ARCHITECTURE

Modular monolith, two runtimes:

```
engine (Rust)
  ingest_solana (Yellowstone)
  ingest_evm    (alloy ws logs)     // Base + RH
  normalize     (TokenDiscovered)
  security_solana
  security_evm  (template, static, fork sim)
  data_engine   (snapshots)
  opportunity
  strategy      (confirmation)
  paper_exec / (later) live_exec per chain
  positions

research (Python)
  EXP001, wallet scores, fingerprints, reports

web (TypeScript)
  scanner, token, research
```

Postgres is the bus. No microservices. Split `execution` to a locked box only before live keys.

**Stack choice (reconsidered):** **Option A, Rust core for everything**, with Python research.

| Option | Verdict |
|---|---|
| A Rust everywhere | **Choose.** `alloy` + Yellowstone, one SM, one paper loop |
| B Rust SOL + Go EVM | Fine if team is Go-native; two ingest daemons to operate |
| C Rust SOL + TS EVM | Worse latency/GC on RH 100ms blocks; OK for dashboard only |
| D Go multi-chain + tiny Rust SOL | Attractive for EVM libs; Solana gRPC still wants Rust |

Maintainability beats micro-optimizing Solana now that we are **not sniping**.

## V1 IMPLEMENTATION ROADMAP

See §26. Earliest coding is discovery → collection → security → measurement.

## FIRST BUILD PHASE (do not implement in this turn)

**Name:** Phase 1 — Discovery skeleton + schema + fixtures

**Repo (proposed)**

```
/apps/engine          Rust
  /src/ingest/{solana,evm}
  /src/normalize
  /src/security/{solana,evm,traits.rs}
  /src/storage
/apps/research        Python
/apps/web             TS
/crates/programs      IDL + ABI snapshots (pinned)
/tests/fixtures       raw txs/logs
/sql                  migrations
```

**Modules / interfaces**

```rust
trait ChainIngest {
  async fn run(&self, tx: Sender<RawEvent>) -> Result<()>;
}
trait Decoder {
  fn decode(&self, raw: RawEvent) -> Option<TokenDiscovered>;
}
trait SecurityFast {
  async fn check(&self, t: &TokenDiscovered) -> FastSecurityResult;
}
```

**DB tables to create first:** `chains`, `launchpads`, `tokens`, `raw_events`, `token_discovered`, `risk_assessments`, `candidate_states`, `contracts`. Not execution tables yet.

**Tests**

- Decode one real Pump.fun create tx (fixture).
- Decode one real Pons V2 `TokenLaunched` log (we can pull from the 83 we saw).
- Decode one Clanker `TokenCreated` log.
- Unknown factory on RH → `launch_mechanism=raw_or_unknown`, not crash.
- Empty bytecode / RPC error → fail closed.

**Success:** three fixtures pass; `TokenDiscovered` written; no DexScreener in the path.

**Failure:** guessed program IDs; cannot decode Pons topic0; ingest only works on public RH RPC.

**Not in phase 1:** Jupiter swaps, paper fills, dashboard polish, ML, Twitter.

## FINAL VERDICT

**BUILD WITH CONDITIONS**

Conditions, updated:

1. Build **collectors on all three chains** before any live tx.
2. Solana remains the only chain allowed to approach live, and only after EXP001 + paper gates (V1).
3. Base and RH stay paper until H5 is computed on **our** prospective data.
4. Security is a hard gate; template tokens still need creator/holder checks.
5. Early discovery is for **observation lead time (H6)**, not for sniping into 99% Pons snipe tax or Jito wars.
6. Kill criteria in §28 are binding.

The three-chain plan is justified as a **research design** (especially RH while it is young). It is not justified as “more chains = more alpha.” If EXP002 shows only Solana has post-cost expectancy, we **delete RH/Base from the trading path** and keep them as optional scanners. That is success, not failure.

---

## Appendix — Independent verification log (2026-08-27)

```
RPC https://rpc.mainnet.chain.robinhood.com
eth_chainId = 0x1237 (4663)
blockTime ≈ 101.2 ms
PonsV2 factory code = 24177 bytes
TokenLaunched(20k blocks ≈ 33 min) = 83  → ~149 / hour
LaunchSwept = 0, PoolGraduated = 0  (same window)
Pons V1 TokenLaunched = 0 (same window)

Base https://mainnet.base.org
Clanker v4 factory code = 12375
ZoraFactory code = 130 (proxy)
Uni v3 factory code = 24535
Uni v4 PoolManager code = 24009
Flaunch code = 9760
Aerodrome factory code = 3516
Uni v2 0x8909Dc15… code = 0  → not deployed / wrong address
Bankr 0x2112b845… code = 0  → stale
```

---

*No production trading code until Phase 1 fixtures exist and this document is reviewed.*
