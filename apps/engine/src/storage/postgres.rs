use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::candidate::CandidateTransition;
use crate::domain::raw_event::{CanonicalStatus, RawEventKind};
use crate::domain::{
    Chain, CollectionSession, DecoderStatus, Finality, GraduationModel, LaunchMechanism, Launchpad,
    LifecycleObserved, QualityStatus, RawEvent, TokenDiscovered, TradeObserved,
};
use crate::error::{EngineError, Result};
use crate::features::FeatureVector;
use crate::lab::experiment::StrategyExperiment;
use crate::lab::persist::SimStore;
use crate::registry::verified_factories;
use crate::security::SecurityAssessment;
use crate::sim::harness::SimulationReport;
use crate::sim::outcome::TokenOutcome;
use crate::sim::types::SimulationRun;
use crate::sim::PolicyPerformance;
use crate::state::TokenStateSnapshot;
use crate::watch::MarketRef;

use super::{ChainHead, Checkpoint, EventStore, IngestGap, InsertRaw, SessionFinish};

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("../../sql/migrations")
            .run(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(format!("migrate: {e}")))?;
        self.upsert_artifacts().await
    }

    async fn upsert_artifacts(&self) -> Result<()> {
        for artifact in crate::artifacts::all_artifacts() {
            sqlx::query(
                r#"
                INSERT INTO decoder_artifacts (protocol, chain, version, source, retrieved_at, sha256)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (protocol, chain, version) DO UPDATE
                SET source = EXCLUDED.source, retrieved_at = EXCLUDED.retrieved_at, sha256 = EXCLUDED.sha256
                "#,
            )
            .bind(artifact.protocol)
            .bind(artifact.chain)
            .bind(artifact.version)
            .bind(artifact.source)
            .bind(artifact.retrieved_at)
            .bind(&artifact.sha256)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        }
        for factory in verified_factories() {
            sqlx::query(
                r#"
                INSERT INTO factories (chain, launchpad, address, verification_status, source, abi_idl_version, enabled)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (chain, address) DO UPDATE
                SET launchpad = EXCLUDED.launchpad,
                    verification_status = EXCLUDED.verification_status,
                    source = EXCLUDED.source,
                    abi_idl_version = EXCLUDED.abi_idl_version,
                    enabled = EXCLUDED.enabled
                "#,
            )
            .bind(factory.chain.as_str())
            .bind(factory.launchpad.as_str())
            .bind(factory.address)
            .bind(factory.verification_status.as_str())
            .bind(factory.source)
            .bind(factory.abi_idl_version)
            .bind(factory.enabled)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    async fn ensure_token(
        &self,
        chain: &str,
        token_address: &str,
        launchpad: &str,
        event_id: &str,
    ) -> Result<()> {
        if token_address.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"
            INSERT INTO tokens (chain, token_address, launchpad, first_discovered_event_id)
            VALUES ($1,$2,$3,$4)
            ON CONFLICT (chain, token_address) DO NOTHING
            "#,
        )
        .bind(chain)
        .bind(token_address)
        .bind(launchpad)
        .bind(event_id)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl EventStore for PostgresStore {
    async fn insert_raw(&self, event: &RawEvent) -> Result<InsertRaw> {
        let payload = serde_json::to_value(event)
            .map_err(|e| EngineError::Storage(format!("serialize raw: {e}")))?;
        let result = sqlx::query(
            r#"
            INSERT INTO raw_events (
                id, chain, source, block_number, block_hash, slot, tx_hash,
                log_index, instruction_index, inner_instruction_index, payload,
                observed_at, persisted_at, chain_time, canonical_status, finality,
                decoder_status, decoder_version, error, removed, transaction_index, execution_status
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NOW(),$13,$14,$15,$16,$17,$18,$19,$20,$21
            )
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(event.event_id())
        .bind(event.chain().as_str())
        .bind(&event.source)
        .bind(event.block_number())
        .bind(event.block_hash())
        .bind(event.slot())
        .bind(event.tx_hash())
        .bind(event.log_index().map(|v| v as i64))
        .bind(event.instruction_index())
        .bind(event.inner_instruction_index())
        .bind(payload)
        .bind(event.observed_at)
        .bind(event.chain_time())
        .bind(event.canonical_status.as_str())
        .bind(event.finality.as_str())
        .bind(event.decoder_status.as_str())
        .bind(&event.decoder_version)
        .bind(&event.error)
        .bind(matches!(&event.kind, RawEventKind::Evm(l) if l.removed))
        .bind(event.transaction_index())
        .bind(match &event.kind {
            RawEventKind::Solana(ix) => ix.execution_status.as_str(),
            RawEventKind::Evm(_) | RawEventKind::DecodedCorpus(_) => "success",
        })
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            Ok(InsertRaw::Duplicate)
        } else {
            Ok(InsertRaw::Inserted)
        }
    }

    async fn insert_discovered(&self, token: &TokenDiscovered) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO tokens (chain, token_address, creator, launchpad, factory_or_program, first_discovered_event_id)
            VALUES ($1,$2,$3,$4,$5,$6)
            ON CONFLICT (chain, token_address) DO NOTHING
            "#,
        )
        .bind(token.chain.as_str())
        .bind(&token.token_address)
        .bind(&token.creator)
        .bind(token.launchpad.as_str())
        .bind(&token.factory_or_program)
        .bind(&token.raw_event_id)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;

        let payload = token.canonical_json();
        sqlx::query(
            r#"
            INSERT INTO token_discovered (
                event_id, chain, chain_id, token_address, creator, launchpad,
                factory_or_program, pool, curve, quote_asset, launch_mechanism,
                bonding_curve, graduation_model, block_number, block_hash, slot,
                tx_hash, instruction_index, inner_instruction_index, log_index,
                chain_time, observed_at, persisted_at, source, decoder_version,
                initial_liquidity, raw_event_id, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                $21,$22,NOW(),$23,$24,$25,$26,$27
            )
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(&token.raw_event_id)
        .bind(token.chain.as_str())
        .bind(token.chain_id.map(|v| v as i64))
        .bind(&token.token_address)
        .bind(&token.creator)
        .bind(token.launchpad.as_str())
        .bind(&token.factory_or_program)
        .bind(&token.pool)
        .bind(&token.curve)
        .bind(&token.quote_asset)
        .bind(token.launch_mechanism.as_str())
        .bind(token.bonding_curve)
        .bind(token.graduation_model.as_str())
        .bind(token.block_number.map(|v| v as i64))
        .bind(&token.block_hash)
        .bind(token.slot.map(|v| v as i64))
        .bind(&token.tx_hash_or_signature)
        .bind(token.instruction_index.map(|v| v as i32))
        .bind(token.inner_instruction_index.map(|v| v as i32))
        .bind(token.log_index.map(|v| v as i64))
        .bind(token.chain_timestamp)
        .bind(token.observed_at)
        .bind(&token.source)
        .bind(&token.decoder_version)
        .bind(&token.initial_liquidity)
        .bind(&token.raw_event_id)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn insert_trade(&self, trade: &TradeObserved) -> Result<()> {
        self.ensure_token(
            trade.chain.as_str(),
            &trade.token_address,
            trade.launchpad.as_str(),
            &trade.raw_event_id,
        )
        .await?;
        let payload = trade.canonical_json();
        sqlx::query(
            r#"
            INSERT INTO token_trades (
                event_id, chain, token_address, launchpad, trader, side,
                base_amount_raw, quote_amount_raw, base_decimals, quote_decimals, quote_asset,
                pool, curve, price_estimate, block_number, block_hash, slot, transaction_index,
                tx_hash, log_index, instruction_index, inner_instruction_index,
                chain_time, observed_at, persisted_at, canonical_status, finality,
                source, decoder_version, raw_event_id, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                $21,$22,$23,$24,NOW(),$25,$26,$27,$28,$29,$30
            )
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(&trade.event_id)
        .bind(trade.chain.as_str())
        .bind(&trade.token_address)
        .bind(trade.launchpad.as_str())
        .bind(&trade.trader)
        .bind(trade.side.as_str())
        .bind(&trade.base_amount_raw)
        .bind(&trade.quote_amount_raw)
        .bind(trade.base_decimals as i32)
        .bind(trade.quote_decimals as i32)
        .bind(&trade.quote_asset)
        .bind(&trade.pool)
        .bind(&trade.curve)
        .bind(&trade.price_estimate)
        .bind(trade.block_number.map(|v| v as i64))
        .bind(&trade.block_hash)
        .bind(trade.slot.map(|v| v as i64))
        .bind(trade.transaction_index.map(|v| v as i64))
        .bind(&trade.tx_hash_or_signature)
        .bind(trade.log_index.map(|v| v as i64))
        .bind(trade.instruction_index.map(|v| v as i32))
        .bind(trade.inner_instruction_index.map(|v| v as i32))
        .bind(trade.chain_timestamp)
        .bind(trade.observed_at)
        .bind(trade.canonical_status.as_str())
        .bind(trade.finality.as_str())
        .bind(&trade.source)
        .bind(&trade.decoder_version)
        .bind(&trade.raw_event_id)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn insert_lifecycle(&self, life: &LifecycleObserved) -> Result<()> {
        if !life.token_address.is_empty() {
            self.ensure_token(
                life.chain.as_str(),
                &life.token_address,
                life.launchpad.as_str(),
                &life.raw_event_id,
            )
            .await?;
        }
        let payload = life.canonical_json();
        sqlx::query(
            r#"
            INSERT INTO lifecycle_events (
                event_id, chain, token_address, launchpad, type, factory, pool, curve,
                block_number, block_hash, slot, transaction_index, tx_hash, log_index,
                instruction_index, inner_instruction_index, chain_time, observed_at,
                persisted_at, canonical_status, finality, source, decoder_version,
                metadata, raw_event_id, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,NOW(),
                $19,$20,$21,$22,$23,$24,$25
            )
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(&life.event_id)
        .bind(life.chain.as_str())
        .bind(&life.token_address)
        .bind(life.launchpad.as_str())
        .bind(life.lifecycle_type.as_str())
        .bind(&life.factory)
        .bind(&life.pool)
        .bind(&life.curve)
        .bind(life.block_number.map(|v| v as i64))
        .bind(&life.block_hash)
        .bind(life.slot.map(|v| v as i64))
        .bind(life.transaction_index.map(|v| v as i64))
        .bind(&life.tx_hash_or_signature)
        .bind(life.log_index.map(|v| v as i64))
        .bind(life.instruction_index.map(|v| v as i32))
        .bind(life.inner_instruction_index.map(|v| v as i32))
        .bind(life.chain_timestamp)
        .bind(life.observed_at)
        .bind(life.canonical_status.as_str())
        .bind(life.finality.as_str())
        .bind(&life.source)
        .bind(&life.decoder_version)
        .bind(&life.metadata)
        .bind(&life.raw_event_id)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn mark_decoder(
        &self,
        event_id: &str,
        status: DecoderStatus,
        version: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE raw_events SET decoder_status = $2, decoder_version = $3, error = $4 WHERE id = $1",
        )
        .bind(event_id)
        .bind(status.as_str())
        .bind(version)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn mark_orphaned(&self, event_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE raw_events SET canonical_status = 'orphaned', removed = TRUE WHERE id = $1",
        )
        .bind(event_id)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_raw(&self, event_id: &str) -> Result<Option<RawEvent>> {
        let row: Option<(serde_json::Value, String, bool)> = sqlx::query_as(
            "SELECT payload, canonical_status, removed FROM raw_events WHERE id = $1",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.map(|(payload, status, removed)| {
            let mut event: RawEvent = serde_json::from_value(payload).expect("stored raw");
            event.canonical_status = if status == "orphaned" {
                CanonicalStatus::Orphaned
            } else {
                CanonicalStatus::Canonical
            };
            if let RawEventKind::Evm(log) = &mut event.kind {
                log.removed = removed;
            }
            event
        }))
    }

    async fn get_discovered(&self, event_id: &str) -> Result<Option<TokenDiscovered>> {
        let row: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM token_discovered WHERE event_id = $1")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.map(|v| serde_json::from_value(v).expect("stored discovered")))
    }

    async fn get_trade(&self, event_id: &str) -> Result<Option<TradeObserved>> {
        let row: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM token_trades WHERE event_id = $1")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.map(|v| serde_json::from_value(v).expect("stored trade")))
    }

    async fn get_lifecycle(&self, event_id: &str) -> Result<Option<LifecycleObserved>> {
        let row: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM lifecycle_events WHERE event_id = $1")
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.map(|v| serde_json::from_value(v).expect("stored lifecycle")))
    }

    async fn save_checkpoint(&self, checkpoint: &Checkpoint) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingest_checkpoints (
                ingest_id, chain, stream, last_block, last_block_hash, last_slot, last_signature,
                overlap_blocks, overlap_slots, updated_at,
                last_seen_block, last_finalized_block, last_seen_slot, last_confirmed_slot,
                last_finalized_slot
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NOW(),$10,$11,$12,$13,$14)
            ON CONFLICT (ingest_id) DO UPDATE SET
                chain = EXCLUDED.chain,
                stream = EXCLUDED.stream,
                last_block = EXCLUDED.last_block,
                last_block_hash = EXCLUDED.last_block_hash,
                last_slot = EXCLUDED.last_slot,
                last_signature = EXCLUDED.last_signature,
                overlap_blocks = EXCLUDED.overlap_blocks,
                overlap_slots = EXCLUDED.overlap_slots,
                last_seen_block = EXCLUDED.last_seen_block,
                last_finalized_block = EXCLUDED.last_finalized_block,
                last_seen_slot = EXCLUDED.last_seen_slot,
                last_confirmed_slot = EXCLUDED.last_confirmed_slot,
                last_finalized_slot = EXCLUDED.last_finalized_slot,
                updated_at = NOW()
            "#,
        )
        .bind(&checkpoint.ingest_id)
        .bind(checkpoint.chain.as_str())
        .bind(&checkpoint.stream)
        .bind(checkpoint.last_block)
        .bind(&checkpoint.last_block_hash)
        .bind(checkpoint.last_slot)
        .bind(&checkpoint.last_signature)
        .bind(checkpoint.overlap_blocks)
        .bind(checkpoint.overlap_slots)
        .bind(checkpoint.last_block)
        .bind(checkpoint.last_finalized_block)
        .bind(checkpoint.last_slot)
        .bind(checkpoint.last_confirmed_slot)
        .bind(checkpoint.last_finalized_slot)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn load_checkpoint(&self, ingest_id: &str) -> Result<Option<Checkpoint>> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<i64>,
                Option<String>,
                Option<i64>,
                Option<String>,
                i32,
                i32,
                Option<i64>,
                Option<i64>,
                Option<i64>,
            ),
        >(
            r#"
            SELECT ingest_id, chain, stream, last_block, last_block_hash, last_slot, last_signature,
                   overlap_blocks, overlap_slots, last_finalized_block, last_confirmed_slot,
                   last_finalized_slot
            FROM ingest_checkpoints WHERE ingest_id = $1
            "#,
        )
        .bind(ingest_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.map(
            |(
                ingest_id,
                chain,
                stream,
                last_block,
                last_block_hash,
                last_slot,
                last_signature,
                overlap_blocks,
                overlap_slots,
                last_finalized_block,
                last_confirmed_slot,
                last_finalized_slot,
            )| Checkpoint {
                ingest_id,
                chain: Chain::parse(&chain).unwrap_or(Chain::Solana),
                stream,
                last_block,
                last_block_hash,
                last_finalized_block,
                last_slot,
                last_confirmed_slot,
                last_finalized_slot,
                last_signature,
                overlap_blocks,
                overlap_slots,
            },
        ))
    }

    async fn set_persisted_at(&self, event_id: &str, at: DateTime<Utc>) -> Result<()> {
        sqlx::query("UPDATE raw_events SET persisted_at = $2 WHERE id = $1")
            .bind(event_id)
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        sqlx::query("UPDATE token_discovered SET persisted_at = $2 WHERE event_id = $1")
            .bind(event_id)
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        sqlx::query("UPDATE token_trades SET persisted_at = $2 WHERE event_id = $1")
            .bind(event_id)
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        sqlx::query("UPDATE lifecycle_events SET persisted_at = $2 WHERE event_id = $1")
            .bind(event_id)
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn insert_gap(&self, gap: &IngestGap) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO ingest_gaps (
                chain, source, stream, from_block, to_block, from_slot, to_slot, reason
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            RETURNING id
            "#,
        )
        .bind(gap.chain.as_str())
        .bind(&gap.source)
        .bind(&gap.stream)
        .bind(gap.from_block)
        .bind(gap.to_block)
        .bind(gap.from_slot)
        .bind(gap.to_slot)
        .bind(&gap.reason)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn mark_gap_recovered(&self, id: i64) -> Result<()> {
        sqlx::query("UPDATE ingest_gaps SET recovered = TRUE, recovered_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn upsert_head(&self, head: &ChainHead) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO chain_heads (
                chain, latest_block, latest_block_hash, latest_slot, finalized_block,
                finalized_slot, observed_at, lag_ms
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            ON CONFLICT (chain) DO UPDATE SET
                latest_block = EXCLUDED.latest_block,
                latest_block_hash = EXCLUDED.latest_block_hash,
                latest_slot = EXCLUDED.latest_slot,
                finalized_block = EXCLUDED.finalized_block,
                finalized_slot = EXCLUDED.finalized_slot,
                observed_at = EXCLUDED.observed_at,
                lag_ms = EXCLUDED.lag_ms
            "#,
        )
        .bind(head.chain.as_str())
        .bind(head.latest_block)
        .bind(&head.latest_block_hash)
        .bind(head.latest_slot)
        .bind(head.finalized_block)
        .bind(head.finalized_slot)
        .bind(head.observed_at)
        .bind(head.lag_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn insert_session(&self, session: &CollectionSession) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO collection_sessions (
                chain, mode, provider, started_at, ended_at, start_block, end_block,
                start_slot, end_slot, complete, quality_status, gap_count, notes
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
            RETURNING id
            "#,
        )
        .bind(session.chain.as_str())
        .bind(&session.mode)
        .bind(&session.provider)
        .bind(session.started_at)
        .bind(session.ended_at)
        .bind(session.start_block)
        .bind(session.end_block)
        .bind(session.start_slot)
        .bind(session.end_slot)
        .bind(session.complete)
        .bind(session.quality_status.as_str())
        .bind(session.gap_count)
        .bind(&session.notes)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn finish_session(&self, id: i64, finish: SessionFinish) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE collection_sessions
            SET ended_at = $2, end_block = $3, end_slot = $4, complete = $5,
                quality_status = $6, gap_count = $7, notes = $8
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(finish.ended_at)
        .bind(finish.end_block)
        .bind(finish.end_slot)
        .bind(finish.complete)
        .bind(finish.quality_status.as_str())
        .bind(finish.gap_count)
        .bind(&finish.notes)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn get_session(&self, id: i64) -> Result<Option<CollectionSession>> {
        let row = sqlx::query_as::<
            _,
            (
                i64,
                String,
                String,
                String,
                DateTime<Utc>,
                Option<DateTime<Utc>>,
                Option<i64>,
                Option<i64>,
                Option<i64>,
                Option<i64>,
                bool,
                String,
                i32,
                Option<String>,
            ),
        >(
            r#"
            SELECT id, chain, mode, provider, started_at, ended_at, start_block, end_block,
                   start_slot, end_slot, complete, quality_status, gap_count, notes
            FROM collection_sessions WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.map(
            |(
                id,
                chain,
                mode,
                provider,
                started_at,
                ended_at,
                start_block,
                end_block,
                start_slot,
                end_slot,
                complete,
                quality_status,
                gap_count,
                notes,
            )| CollectionSession {
                id: Some(id),
                chain: Chain::parse(&chain).unwrap_or(Chain::Solana),
                mode,
                provider,
                started_at,
                ended_at,
                start_block,
                end_block,
                start_slot,
                end_slot,
                complete,
                quality_status: QualityStatus::parse(&quality_status)
                    .unwrap_or(QualityStatus::DevelopmentIncomplete),
                gap_count,
                notes,
            },
        ))
    }

    async fn upsert_watched_market(
        &self,
        market: &MarketRef,
        source_event_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO watched_markets (
                chain, launchpad, token_address, pool, curve, pool_id,
                source_event_id, source_curve, destination_dex, quote_asset, active
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,TRUE)
            ON CONFLICT (chain, token_address) DO UPDATE SET
                launchpad = EXCLUDED.launchpad,
                pool = COALESCE(EXCLUDED.pool, watched_markets.pool),
                curve = COALESCE(EXCLUDED.curve, watched_markets.curve),
                pool_id = COALESCE(EXCLUDED.pool_id, watched_markets.pool_id),
                source_curve = COALESCE(EXCLUDED.source_curve, watched_markets.source_curve),
                destination_dex = COALESCE(EXCLUDED.destination_dex, watched_markets.destination_dex),
                quote_asset = COALESCE(EXCLUDED.quote_asset, watched_markets.quote_asset),
                active = TRUE
            "#,
        )
        .bind(market.chain.as_str())
        .bind(market.launchpad.as_str())
        .bind(&market.token_address)
        .bind(&market.pool)
        .bind(&market.curve)
        .bind(&market.pool_id)
        .bind(source_event_id)
        .bind(&market.curve)
        .bind(if market.chain == Chain::Solana && market.pool.is_some() {
            Some("pumpswap")
        } else {
            None
        })
        .bind(&market.quote_asset)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn load_watched_markets(&self, chain: Chain) -> Result<Vec<MarketRef>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            SELECT launchpad, token_address, pool, curve, pool_id, quote_asset
            FROM watched_markets
            WHERE chain = $1 AND active
            "#,
        )
        .bind(chain.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(
                |(launchpad, token_address, pool, curve, pool_id, quote_asset)| MarketRef {
                    chain,
                    launchpad: Launchpad::parse(&launchpad),
                    token_address,
                    curve,
                    pool,
                    pool_id,
                    quote_asset,
                },
            )
            .collect())
    }

    async fn unrecovered_gap_count(&self, chain: Chain) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ingest_gaps WHERE chain = $1 AND recovered = FALSE",
        )
        .bind(chain.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(count)
    }

    async fn insert_snapshot(&self, snap: &TokenStateSnapshot) -> Result<i64> {
        let payload = serde_json::to_value(snap)
            .map_err(|e| EngineError::Storage(format!("snapshot json: {e}")))?;
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO token_state_snapshots (
                chain, token_address, launchpad, snapshot_time, age_ms, snapshot_kind,
                lifecycle_trigger, lifecycle_state, quote_asset,
                buy_count_total, sell_count_total, unique_buyers_total, unique_sellers_total,
                buy_quote_volume_raw_total, sell_quote_volume_raw_total,
                buy_token_volume_raw_total, sell_token_volume_raw_total,
                creator_buy_count, creator_sell_count, creator_buy_quote_raw, creator_sell_quote_raw,
                last_trade_side, last_trade_token_raw, last_trade_quote_raw,
                last_trade_token_decimals, last_trade_quote_decimals,
                curve_progress_bps, graduation_progress_bps, market_state_type,
                market_state_json, rolling_5s_json, rolling_15s_json, rolling_30s_json,
                rolling_60s_json, rolling_120s_json, rolling_300s_json, rolling_900s_json,
                as_of_event_id, as_of_block, as_of_slot, as_of_event_order,
                data_quality, source_session_id, canonical_status, finality,
                version, superseded, fingerprint, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                $21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34,$35,$36,$37,
                $38,$39,$40,$41,$42,$43,$44,$45,$46,$47,$48,$49
            )
            RETURNING id
            "#,
        )
        .bind(snap.chain.as_str())
        .bind(&snap.token_address)
        .bind(snap.launchpad.as_str())
        .bind(snap.snapshot_time)
        .bind(snap.age_ms)
        .bind(snap.snapshot_kind.as_str())
        .bind(&snap.lifecycle_trigger)
        .bind(snap.lifecycle_state.as_str())
        .bind(&snap.quote_asset)
        .bind(snap.buy_count_total as i64)
        .bind(snap.sell_count_total as i64)
        .bind(snap.unique_buyers_total as i64)
        .bind(snap.unique_sellers_total as i64)
        .bind(&snap.buy_quote_volume_raw_total)
        .bind(&snap.sell_quote_volume_raw_total)
        .bind(&snap.buy_token_volume_raw_total)
        .bind(&snap.sell_token_volume_raw_total)
        .bind(snap.creator_buy_count as i64)
        .bind(snap.creator_sell_count as i64)
        .bind(&snap.creator_buy_quote_raw)
        .bind(&snap.creator_sell_quote_raw)
        .bind(snap.last_trade_side.map(|s| s.as_str()))
        .bind(&snap.last_trade_token_raw)
        .bind(&snap.last_trade_quote_raw)
        .bind(snap.last_trade_token_decimals.map(|v| v as i32))
        .bind(snap.last_trade_quote_decimals.map(|v| v as i32))
        .bind(snap.curve_progress_bps.map(|v| v as i32))
        .bind(snap.graduation_progress_bps.map(|v| v as i32))
        .bind(&snap.market_state_type)
        .bind(serde_json::to_value(&snap.market_state).unwrap_or(serde_json::json!({})))
        .bind(serde_json::to_value(&snap.rolling_5s).unwrap_or(serde_json::json!({})))
        .bind(serde_json::to_value(&snap.rolling_15s).unwrap_or(serde_json::json!({})))
        .bind(serde_json::to_value(&snap.rolling_30s).unwrap_or(serde_json::json!({})))
        .bind(serde_json::to_value(&snap.rolling_60s).unwrap_or(serde_json::json!({})))
        .bind(serde_json::to_value(&snap.rolling_120s).unwrap_or(serde_json::json!({})))
        .bind(serde_json::to_value(&snap.rolling_300s).unwrap_or(serde_json::json!({})))
        .bind(serde_json::to_value(&snap.rolling_900s).unwrap_or(serde_json::json!({})))
        .bind(&snap.as_of_event_id)
        .bind(snap.as_of_block)
        .bind(snap.as_of_slot)
        .bind(&snap.as_of_event_order)
        .bind(snap.data_quality.as_str())
        .bind(snap.source_session_id)
        .bind(snap.canonical_status.as_str())
        .bind(snap.finality.as_str())
        .bind(snap.version)
        .bind(snap.superseded)
        .bind(&snap.fingerprint)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn list_snapshots(
        &self,
        chain: Chain,
        token: &str,
        include_superseded: bool,
    ) -> Result<Vec<TokenStateSnapshot>> {
        let rows: Vec<(serde_json::Value, i64, bool)> = if include_superseded {
            sqlx::query_as(
                "SELECT payload, id, superseded FROM token_state_snapshots WHERE chain = $1 AND token_address = $2 ORDER BY snapshot_time, id",
            )
            .bind(chain.as_str())
            .bind(token)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as(
                "SELECT payload, id, superseded FROM token_state_snapshots WHERE chain = $1 AND token_address = $2 AND NOT superseded ORDER BY snapshot_time, id",
            )
            .bind(chain.as_str())
            .bind(token)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|(p, id, sup)| {
                let mut s: TokenStateSnapshot = serde_json::from_value(p).ok()?;
                s.id = Some(id);
                s.superseded = sup;
                Some(s)
            })
            .collect())
    }

    async fn latest_snapshot(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<TokenStateSnapshot>> {
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM token_state_snapshots WHERE chain = $1 AND token_address = $2 AND NOT superseded ORDER BY snapshot_time DESC, version DESC, id DESC LIMIT 1",
        )
        .bind(chain.as_str())
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|(p, id)| {
            let mut s: TokenStateSnapshot = serde_json::from_value(p).ok()?;
            s.id = Some(id);
            Some(s)
        }))
    }

    async fn snapshot_at_or_before(
        &self,
        chain: Chain,
        token: &str,
        time: DateTime<Utc>,
    ) -> Result<Option<TokenStateSnapshot>> {
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM token_state_snapshots WHERE chain = $1 AND token_address = $2 AND NOT superseded AND snapshot_time <= $3 ORDER BY snapshot_time DESC, version DESC LIMIT 1",
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(time)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|(p, id)| {
            let mut s: TokenStateSnapshot = serde_json::from_value(p).ok()?;
            s.id = Some(id);
            Some(s)
        }))
    }

    async fn milestone_snapshot(
        &self,
        chain: Chain,
        token: &str,
        age_ms: i64,
    ) -> Result<Option<TokenStateSnapshot>> {
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM token_state_snapshots WHERE chain = $1 AND token_address = $2 AND NOT superseded AND snapshot_kind = 'MILESTONE' AND age_ms = $3 ORDER BY version DESC LIMIT 1",
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(age_ms)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|(p, id)| {
            let mut s: TokenStateSnapshot = serde_json::from_value(p).ok()?;
            s.id = Some(id);
            Some(s)
        }))
    }

    async fn mark_snapshots_superseded(&self, chain: Chain, token: &str) -> Result<u64> {
        let res = sqlx::query(
            "UPDATE token_state_snapshots SET superseded = TRUE WHERE chain = $1 AND token_address = $2 AND NOT superseded",
        )
        .bind(chain.as_str())
        .bind(token)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(res.rows_affected())
    }

    async fn upsert_current_state(
        &self,
        chain: Chain,
        token: &str,
        snapshot_id: Option<i64>,
        lifecycle: &str,
        last_event_time: Option<DateTime<Utc>>,
        last_event_id: Option<&str>,
        data_quality: QualityStatus,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO token_current_state (
                chain, token_address, latest_snapshot_id, lifecycle_state, last_event_time,
                last_event_id, data_quality, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,NOW())
            ON CONFLICT (chain, token_address) DO UPDATE SET
                latest_snapshot_id = EXCLUDED.latest_snapshot_id,
                lifecycle_state = EXCLUDED.lifecycle_state,
                last_event_time = EXCLUDED.last_event_time,
                last_event_id = EXCLUDED.last_event_id,
                data_quality = EXCLUDED.data_quality,
                updated_at = NOW()
            "#,
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(snapshot_id)
        .bind(lifecycle)
        .bind(last_event_time)
        .bind(last_event_id)
        .bind(data_quality.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn load_token_trades(&self, chain: Chain, token: &str) -> Result<Vec<TradeObserved>> {
        let rows: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT payload FROM token_trades WHERE chain = $1 AND token_address = $2",
        )
        .bind(chain.as_str())
        .bind(token)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    async fn load_token_lifecycle(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Vec<LifecycleObserved>> {
        let rows: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT payload FROM lifecycle_events WHERE chain = $1 AND token_address = $2",
        )
        .bind(chain.as_str())
        .bind(token)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    async fn load_token_discovered(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<TokenDiscovered>> {
        let row: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT payload FROM token_discovered WHERE chain = $1 AND token_address = $2 ORDER BY observed_at LIMIT 1",
        )
        .bind(chain.as_str())
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|v| serde_json::from_value(v).ok()))
    }

    async fn insert_assessment(&self, a: &SecurityAssessment) -> Result<i64> {
        let payload = serde_json::to_value(a)
            .map_err(|e| EngineError::Storage(format!("assessment json: {e}")))?;
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO security_assessments (
                chain, token_address, launchpad, snapshot_id, as_of_block, as_of_block_hash,
                as_of_slot, as_of_time, verdict, contract_risk, token_mechanics_risk,
                privilege_risk, sellability_risk, liquidity_structure_risk,
                hard_reject_reasons, warnings, evidence, analyzer_version, policy_version,
                data_quality, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21
            )
            RETURNING id
            "#,
        )
        .bind(a.chain.as_str())
        .bind(&a.token_address)
        .bind(a.launchpad.as_str())
        .bind(a.snapshot_id)
        .bind(a.as_of_block)
        .bind(&a.as_of_block_hash)
        .bind(a.as_of_slot)
        .bind(a.as_of_time)
        .bind(a.verdict.as_str())
        .bind(a.contract_risk.as_str())
        .bind(a.token_mechanics_risk.as_str())
        .bind(a.privilege_risk.as_str())
        .bind(a.sellability_risk.as_str())
        .bind(a.liquidity_structure_risk.as_str())
        .bind(serde_json::to_value(&a.hard_reject_reasons).unwrap_or(serde_json::json!([])))
        .bind(serde_json::to_value(&a.warnings).unwrap_or(serde_json::json!([])))
        .bind(serde_json::to_value(&a.evidence).unwrap_or(serde_json::json!([])))
        .bind(&a.analyzer_version)
        .bind(&a.policy_version)
        .bind(a.data_quality.as_str())
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO token_current_security (chain, token_address, latest_assessment_id, verdict, updated_at)
            VALUES ($1,$2,$3,$4,NOW())
            ON CONFLICT (chain, token_address) DO UPDATE SET
                latest_assessment_id = EXCLUDED.latest_assessment_id,
                verdict = EXCLUDED.verdict,
                updated_at = NOW()
            "#,
        )
        .bind(a.chain.as_str())
        .bind(&a.token_address)
        .bind(id)
        .bind(a.verdict.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn list_assessments(&self, chain: Chain, token: &str) -> Result<Vec<SecurityAssessment>> {
        let rows: Vec<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM security_assessments WHERE chain = $1 AND token_address = $2 ORDER BY id",
        )
        .bind(chain.as_str())
        .bind(token)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|(p, id)| {
                let mut a: SecurityAssessment = serde_json::from_value(p).ok()?;
                a.id = Some(id);
                Some(a)
            })
            .collect())
    }

    async fn latest_assessment(
        &self,
        chain: Chain,
        token: &str,
    ) -> Result<Option<SecurityAssessment>> {
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM security_assessments WHERE chain = $1 AND token_address = $2 ORDER BY id DESC LIMIT 1",
        )
        .bind(chain.as_str())
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|(p, id)| {
            let mut a: SecurityAssessment = serde_json::from_value(p).ok()?;
            a.id = Some(id);
            Some(a)
        }))
    }

    async fn insert_feature_vector(&self, v: &FeatureVector) -> Result<i64> {
        let payload = serde_json::to_value(v)
            .map_err(|e| EngineError::Storage(format!("feature json: {e}")))?;
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO feature_vectors (
                chain, token_address, launchpad, snapshot_id, security_assessment_id,
                as_of_block, as_of_block_hash, as_of_slot, as_of_time, token_age_ms,
                feature_version, data_quality, flow_quality, liquidity_quality,
                holder_quality, creator_quality, fingerprint, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18
            )
            RETURNING id
            "#,
        )
        .bind(v.chain.as_str())
        .bind(&v.token_address)
        .bind(v.launchpad.as_str())
        .bind(v.snapshot_id)
        .bind(v.security_assessment_id)
        .bind(v.as_of_block)
        .bind(&v.as_of_block_hash)
        .bind(v.as_of_slot)
        .bind(v.as_of_time)
        .bind(v.token_age_ms)
        .bind(&v.feature_version)
        .bind(v.data_quality.as_str())
        .bind(v.flow_quality.as_str())
        .bind(v.liquidity_quality.as_str())
        .bind(v.holder_quality.as_str())
        .bind(v.creator_quality.as_str())
        .bind(&v.fingerprint)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO token_current_features (chain, token_address, latest_vector_id, feature_version, updated_at)
            VALUES ($1,$2,$3,$4,NOW())
            ON CONFLICT (chain, token_address) DO UPDATE SET
                latest_vector_id = EXCLUDED.latest_vector_id,
                feature_version = EXCLUDED.feature_version,
                updated_at = NOW()
            "#,
        )
        .bind(v.chain.as_str())
        .bind(&v.token_address)
        .bind(id)
        .bind(&v.feature_version)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn list_feature_vectors(&self, chain: Chain, token: &str) -> Result<Vec<FeatureVector>> {
        let rows: Vec<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM feature_vectors WHERE chain = $1 AND token_address = $2 ORDER BY id",
        )
        .bind(chain.as_str())
        .bind(token)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|(p, id)| {
                let mut v: FeatureVector = serde_json::from_value(p).ok()?;
                v.id = Some(id);
                Some(v)
            })
            .collect())
    }

    async fn feature_at_or_before(
        &self,
        chain: Chain,
        token: &str,
        time: DateTime<Utc>,
    ) -> Result<Option<FeatureVector>> {
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM feature_vectors WHERE chain = $1 AND token_address = $2 AND as_of_time <= $3 ORDER BY as_of_time DESC, id DESC LIMIT 1",
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(time)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|(p, id)| {
            let mut v: FeatureVector = serde_json::from_value(p).ok()?;
            v.id = Some(id);
            Some(v)
        }))
    }

    async fn insert_candidate_transition(&self, t: &CandidateTransition) -> Result<i64> {
        let payload = serde_json::to_value(t)
            .map_err(|e| EngineError::Storage(format!("candidate json: {e}")))?;
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO candidate_state_transitions (
                chain, token_address, launchpad, policy_id, policy_version,
                from_state, to_state, reason, as_of_time, snapshot_id,
                security_assessment_id, feature_vector_id, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13
            )
            RETURNING id
            "#,
        )
        .bind(t.chain.as_str())
        .bind(&t.token_address)
        .bind(t.launchpad.as_str())
        .bind(&t.policy_id)
        .bind(&t.policy_version)
        .bind(t.from_state.as_str())
        .bind(t.to_state.as_str())
        .bind(&t.reason)
        .bind(t.as_of_time)
        .bind(t.snapshot_id)
        .bind(t.security_assessment_id)
        .bind(t.feature_vector_id)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO token_current_candidate (
                chain, token_address, policy_id, latest_transition_id, state, policy_version, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,NOW())
            ON CONFLICT (chain, token_address, policy_id) DO UPDATE SET
                latest_transition_id = EXCLUDED.latest_transition_id,
                state = EXCLUDED.state,
                policy_version = EXCLUDED.policy_version,
                updated_at = NOW()
            "#,
        )
        .bind(t.chain.as_str())
        .bind(&t.token_address)
        .bind(&t.policy_id)
        .bind(id)
        .bind(t.to_state.as_str())
        .bind(&t.policy_version)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn list_candidate_transitions(
        &self,
        chain: Chain,
        token: &str,
        policy_id: &str,
    ) -> Result<Vec<CandidateTransition>> {
        let rows: Vec<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM candidate_state_transitions WHERE chain = $1 AND token_address = $2 AND policy_id = $3 ORDER BY id",
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(policy_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|(p, id)| {
                let mut t: CandidateTransition = serde_json::from_value(p).ok()?;
                t.id = Some(id);
                Some(t)
            })
            .collect())
    }

    async fn latest_candidate(
        &self,
        chain: Chain,
        token: &str,
        policy_id: &str,
    ) -> Result<Option<CandidateTransition>> {
        let row: Option<(serde_json::Value, i64)> = sqlx::query_as(
            "SELECT payload, id FROM candidate_state_transitions WHERE chain = $1 AND token_address = $2 AND policy_id = $3 ORDER BY id DESC LIMIT 1",
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(policy_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|(p, id)| {
            let mut t: CandidateTransition = serde_json::from_value(p).ok()?;
            t.id = Some(id);
            Some(t)
        }))
    }

    async fn export_feature_vectors(
        &self,
        chain: Option<Chain>,
        limit: i64,
    ) -> Result<Vec<FeatureVector>> {
        let cap = if limit <= 0 { 100_000 } else { limit };
        let rows: Vec<(serde_json::Value, i64)> = if let Some(c) = chain {
            sqlx::query_as(
                "SELECT payload, id FROM feature_vectors WHERE chain = $1 ORDER BY as_of_time, id LIMIT $2",
            )
            .bind(c.as_str())
            .bind(cap)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?
        } else {
            sqlx::query_as(
                "SELECT payload, id FROM feature_vectors ORDER BY as_of_time, id LIMIT $1",
            )
            .bind(cap)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?
        };
        Ok(rows
            .into_iter()
            .filter_map(|(p, id)| {
                let mut v: FeatureVector = serde_json::from_value(p).ok()?;
                v.id = Some(id);
                Some(v)
            })
            .collect())
    }
}

#[async_trait]
impl SimStore for PostgresStore {
    async fn insert_simulation_run(&self, r: &SimulationRun) -> Result<i64> {
        let payload = serde_json::to_value(r)
            .map_err(|e| EngineError::Storage(format!("sim run json: {e}")))?;
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO simulation_runs (
                mode, chain, launchpad, strategy_policy_id, strategy_policy_version,
                execution_model_version, fee_model_version, impact_model_version,
                failure_model_version, source_session_id, started_at, ended_at,
                data_quality, research_valid, config_snapshot, random_seed, experiment_id, payload
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
            RETURNING id
            "#,
        )
        .bind(r.mode.as_str())
        .bind(r.chain.map(|c| c.as_str()))
        .bind(r.launchpad.map(|l| l.as_str()))
        .bind(&r.strategy_policy_id)
        .bind(&r.strategy_policy_version)
        .bind(&r.execution_model_version)
        .bind(&r.fee_model_version)
        .bind(&r.impact_model_version)
        .bind(&r.failure_model_version)
        .bind(r.source_session_id)
        .bind(r.started_at)
        .bind(r.ended_at)
        .bind(r.data_quality.as_str())
        .bind(r.research_valid)
        .bind(&r.config_snapshot)
        .bind(r.random_seed as i64)
        .bind(&r.experiment_id)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn get_simulation_run(&self, id: i64) -> Result<Option<SimulationRun>> {
        let row: Option<(serde_json::Value, i64)> =
            sqlx::query_as("SELECT payload, id FROM simulation_runs WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|(p, id)| {
            let mut r: SimulationRun = serde_json::from_value(p).ok()?;
            r.id = Some(id);
            Some(r)
        }))
    }

    async fn persist_report(&self, report: &SimulationReport) -> Result<i64> {
        let mut run = report.run.clone();
        let payload = serde_json::to_value(report)
            .map_err(|e| EngineError::Storage(format!("report json: {e}")))?;
        run.config_snapshot = payload.clone();
        let id = self.insert_simulation_run(&run).await?;
        for o in &report.orders {
            let p = serde_json::to_value(o).unwrap_or(serde_json::json!({}));
            sqlx::query(
                r#"
                INSERT INTO simulated_orders (
                    simulation_run_id, policy_id, chain, token_address, side, decision_time,
                    requested_amount, status, feature_vector_id, security_assessment_id,
                    candidate_transition_id, payload
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                "#,
            )
            .bind(id)
            .bind(&o.policy_id)
            .bind(o.chain.as_str())
            .bind(&o.token)
            .bind(o.side.as_str())
            .bind(o.decision_time)
            .bind(&o.requested_amount)
            .bind(o.status.as_str())
            .bind(o.feature_vector_id)
            .bind(o.security_assessment_id)
            .bind(o.candidate_transition_id)
            .bind(p)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        }
        for pos in &report.positions {
            let p = serde_json::to_value(pos).unwrap_or(serde_json::json!({}));
            let pid: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO simulated_positions (
                    simulation_run_id, chain, token_address, launchpad, strategy_policy_id,
                    opened_at, closed_at, status, quote_cost, realized_quote, mfe_quote, mae_quote,
                    capture_ratio_bps, payload
                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
                RETURNING id
                "#,
            )
            .bind(id)
            .bind(pos.chain.as_str())
            .bind(&pos.token)
            .bind(pos.launchpad.as_str())
            .bind(&pos.strategy_policy_id)
            .bind(pos.opened_at)
            .bind(pos.closed_at)
            .bind(pos.status.as_str())
            .bind(&pos.quote_cost)
            .bind(&pos.realized_quote)
            .bind(&pos.mfe_quote)
            .bind(&pos.mae_quote)
            .bind(pos.capture_ratio_bps.map(|v| v as i32))
            .bind(p)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
            for e in &pos.events {
                let ep = serde_json::to_value(e).unwrap_or(serde_json::json!({}));
                sqlx::query(
                    "INSERT INTO position_events (position_id, kind, at, payload) VALUES ($1,$2,$3,$4)",
                )
                .bind(pid)
                .bind(e.kind.as_str())
                .bind(e.at)
                .bind(ep)
                .execute(&self.pool)
                .await
                .map_err(|err| EngineError::Storage(err.to_string()))?;
            }
        }
        let _ = payload;
        Ok(id)
    }

    async fn load_report(&self, run_id: i64) -> Result<Option<SimulationReport>> {
        let row: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT config_snapshot FROM simulation_runs WHERE id = $1")
                .bind(run_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|v| serde_json::from_value::<SimulationReport>(v).ok()))
    }

    async fn insert_token_outcome(&self, o: &TokenOutcome) -> Result<i64> {
        let payload = serde_json::to_value(o)
            .map_err(|e| EngineError::Storage(format!("outcome json: {e}")))?;
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO token_outcomes (
                chain, token_address, launchpad, reference_time, horizon_ms,
                max_return_bps, final_return_bps, reached_2x, reached_5x, reached_10x, reached_20x,
                time_to_2x_ms, time_to_5x_ms, time_to_10x_ms, time_to_20x_ms,
                outcome_quality, outcome_model_version, payload
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
            RETURNING id
            "#,
        )
        .bind(o.chain.as_str())
        .bind(&o.token)
        .bind(o.launchpad.as_str())
        .bind(o.reference_time)
        .bind(o.horizon_ms)
        .bind(o.max_return_bps)
        .bind(o.final_return_bps)
        .bind(o.reached_2x)
        .bind(o.reached_5x)
        .bind(o.reached_10x)
        .bind(o.reached_20x)
        .bind(o.time_to_2x_ms)
        .bind(o.time_to_5x_ms)
        .bind(o.time_to_10x_ms)
        .bind(o.time_to_20x_ms)
        .bind(o.outcome_quality.as_str())
        .bind(&o.outcome_model_version)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn insert_policy_performance(&self, run_id: i64, p: &PolicyPerformance) -> Result<i64> {
        let payload = serde_json::to_value(p).unwrap_or(serde_json::json!({}));
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO policy_performance (simulation_run_id, policy_id, n_orders, filled_entries, research_valid, payload)
            VALUES ($1,$2,$3,$4,$5,$6) RETURNING id
            "#,
        )
        .bind(run_id)
        .bind(&p.policy_id)
        .bind(p.n_orders as i32)
        .bind(p.filled_entries as i32)
        .bind(p.research_valid)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn upsert_experiment(&self, e: &StrategyExperiment) -> Result<()> {
        let payload = serde_json::to_value(e)
            .map_err(|err| EngineError::Storage(format!("exp json: {err}")))?;
        sqlx::query(
            r#"
            INSERT INTO strategy_experiments (
                experiment_id, name, description, hypothesis, dataset_id, dataset_hash,
                chain, launchpad, train_start, train_end, validation_start, validation_end,
                test_start, test_end, feature_version, security_policy_version,
                candidate_policy_version, strategy_policy_version, execution_model_version,
                fee_model_version, impact_model_version, slippage_model_version,
                outcome_model_version, position_size_config, exit_policy_config, config_hash,
                locked_config, status, variants_evaluated, hypotheses_tested, git_commit, seed, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33
            )
            ON CONFLICT (experiment_id) DO UPDATE SET
                status = EXCLUDED.status,
                config_hash = EXCLUDED.config_hash,
                locked_config = EXCLUDED.locked_config,
                payload = EXCLUDED.payload,
                locked_at = EXCLUDED.locked_at,
                completed_at = EXCLUDED.completed_at
            "#,
        )
        .bind(&e.experiment_id)
        .bind(&e.name)
        .bind(&e.description)
        .bind(&e.hypothesis)
        .bind(&e.dataset_id)
        .bind(&e.dataset_hash)
        .bind(e.chain.map(|c| c.as_str()))
        .bind(e.launchpad.map(|l| l.as_str()))
        .bind(e.splits.as_ref().map(|s| s.train_start))
        .bind(e.splits.as_ref().map(|s| s.train_end))
        .bind(e.splits.as_ref().map(|s| s.validation_start))
        .bind(e.splits.as_ref().map(|s| s.validation_end))
        .bind(e.splits.as_ref().map(|s| s.test_start))
        .bind(e.splits.as_ref().map(|s| s.test_end))
        .bind(&e.feature_version)
        .bind(&e.security_policy_version)
        .bind(&e.candidate_policy_version)
        .bind(&e.strategy_policy_version)
        .bind(&e.execution_model_version)
        .bind(&e.fee_model_version)
        .bind(&e.impact_model_version)
        .bind(&e.slippage_model_version)
        .bind(&e.outcome_model_version)
        .bind(serde_json::json!({ "quote": e.position_size }))
        .bind(serde_json::json!({ "exit": e.exit_policy_id }))
        .bind(&e.config_hash)
        .bind(&e.locked_config)
        .bind(e.status.as_str())
        .bind(e.variants_evaluated as i32)
        .bind(e.hypotheses_tested as i32)
        .bind(&e.git_commit)
        .bind(e.seed as i64)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|err| EngineError::Storage(err.to_string()))?;
        Ok(())
    }

    async fn get_experiment(&self, id: &str) -> Result<Option<StrategyExperiment>> {
        let row: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM strategy_experiments WHERE experiment_id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|v| serde_json::from_value(v).ok()))
    }
}

impl PostgresStore {
    pub async fn insert_open_paper_position(
        &self,
        pos: &crate::sim::position::SimulatedPosition,
    ) -> Result<i64> {
        let p = serde_json::to_value(pos).unwrap_or(serde_json::json!({}));
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO simulated_positions (
                chain, token_address, launchpad, strategy_policy_id,
                opened_at, closed_at, status, quote_cost, realized_quote, mfe_quote, mae_quote,
                capture_ratio_bps, remaining_token_amount, payload,
                experiment_id, realized_pnl_quote, initial_token_amount
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
            RETURNING id
            "#,
        )
        .bind(pos.chain.as_str())
        .bind(&pos.token)
        .bind(pos.launchpad.as_str())
        .bind(&pos.strategy_policy_id)
        .bind(pos.opened_at)
        .bind(pos.closed_at)
        .bind(pos.status.as_str())
        .bind(&pos.quote_cost)
        .bind(&pos.realized_quote)
        .bind(&pos.mfe_quote)
        .bind(&pos.mae_quote)
        .bind(pos.capture_ratio_bps.map(|v| v as i32))
        .bind(&pos.remaining_token_amount)
        .bind(p)
        .bind(crate::lab::pons_exp::experiment_prefix(
            &pos.strategy_policy_id,
        ))
        .bind(&pos.realized_pnl_quote)
        .bind(&pos.initial_token_amount)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn load_open_paper_positions(
        &self,
    ) -> Result<Vec<crate::sim::position::SimulatedPosition>> {
        self.load_open_paper_positions_prefixed(None).await
    }

    pub async fn load_open_paper_positions_prefixed(
        &self,
        prefix: Option<&str>,
    ) -> Result<Vec<crate::sim::position::SimulatedPosition>> {
        let rows: Vec<(i64, serde_json::Value)> = match prefix {
            Some(p) => sqlx::query_as(
                "SELECT id, payload FROM simulated_positions WHERE status IN ('OPEN', 'SESSION_ENDED_OPEN') AND strategy_policy_id LIKE $1",
            )
            .bind(crate::lab::pons_exp::experiment_arm_like(p))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?,
            None => sqlx::query_as(
                "SELECT id, payload FROM simulated_positions WHERE status IN ('OPEN', 'SESSION_ENDED_OPEN')",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?,
        };
        Ok(rows
            .into_iter()
            .filter_map(|(id, v)| {
                let mut p: crate::sim::position::SimulatedPosition =
                    serde_json::from_value(v).ok()?;
                p.id = id;
                Some(p)
            })
            .collect())
    }

    pub async fn update_paper_position(
        &self,
        pos: &crate::sim::position::SimulatedPosition,
    ) -> Result<()> {
        let p = serde_json::to_value(pos).unwrap_or(serde_json::json!({}));
        sqlx::query(
            r#"
            UPDATE simulated_positions
            SET status = $1, closed_at = $2, remaining_token_amount = $3, payload = $4,
                realized_quote = $5, realized_pnl_quote = $6, quote_cost = $7
            WHERE id = $8
               OR (id IS DISTINCT FROM $8 AND chain = $9 AND token_address = $10
                   AND strategy_policy_id = $11 AND status IN ('OPEN', 'SESSION_ENDED_OPEN'))
            "#,
        )
        .bind(pos.status.as_str())
        .bind(pos.closed_at)
        .bind(&pos.remaining_token_amount)
        .bind(p)
        .bind(&pos.realized_quote)
        .bind(&pos.realized_pnl_quote)
        .bind(&pos.quote_cost)
        .bind(pos.id)
        .bind(pos.chain.as_str())
        .bind(&pos.token)
        .bind(&pos.strategy_policy_id)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn list_recent_discovered(
        &self,
        max_age: chrono::Duration,
    ) -> Result<Vec<(Chain, String)>> {
        let cutoff = Utc::now() - max_age;
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT DISTINCT chain, token_address FROM token_discovered WHERE observed_at > $1",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|(c, t)| Chain::parse(&c).map(|ch| (ch, t)))
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_prospective_signal(
        &self,
        chain: Chain,
        token: &str,
        launchpad: Launchpad,
        policy_id: &str,
        decision_time: DateTime<Utc>,
        enter: bool,
        reason: &str,
        research_valid_for_alpha: bool,
        feature_vector_id: Option<i64>,
        security_assessment_id: Option<i64>,
        candidate_state: &str,
        desired_notional: &str,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO prospective_signals (
                chain, token_address, launchpad, policy_id, decision_time, enter, reason,
                research_valid_for_alpha, feature_vector_id, security_assessment_id,
                candidate_state, desired_notional
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            RETURNING id
            "#,
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(launchpad.as_str())
        .bind(policy_id)
        .bind(decision_time)
        .bind(enter)
        .bind(reason)
        .bind(research_valid_for_alpha)
        .bind(feature_vector_id)
        .bind(security_assessment_id)
        .bind(candidate_state)
        .bind(desired_notional)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_paper_order(
        &self,
        policy_id: &str,
        chain: Chain,
        token: &str,
        side: &str,
        decision_time: DateTime<Utc>,
        requested_amount: &str,
        status: &str,
        feature_vector_id: Option<i64>,
        security_assessment_id: Option<i64>,
        payload: serde_json::Value,
    ) -> Result<i64> {
        self.insert_paper_order_ex(
            policy_id,
            chain,
            token,
            side,
            decision_time,
            requested_amount,
            status,
            feature_vector_id,
            security_assessment_id,
            payload,
            None,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_paper_order_ex(
        &self,
        policy_id: &str,
        chain: Chain,
        token: &str,
        side: &str,
        decision_time: DateTime<Utc>,
        requested_amount: &str,
        status: &str,
        feature_vector_id: Option<i64>,
        security_assessment_id: Option<i64>,
        payload: serde_json::Value,
        experiment_id: Option<&str>,
        position_id: Option<i64>,
        exit_reason: Option<&str>,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO simulated_orders (
                policy_id, chain, token_address, side, decision_time, requested_amount,
                status, feature_vector_id, security_assessment_id, payload,
                experiment_id, position_id, exit_reason
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
            RETURNING id
            "#,
        )
        .bind(policy_id)
        .bind(chain.as_str())
        .bind(token)
        .bind(side)
        .bind(decision_time)
        .bind(requested_amount)
        .bind(status)
        .bind(feature_vector_id)
        .bind(security_assessment_id)
        .bind(payload)
        .bind(experiment_id)
        .bind(position_id)
        .bind(exit_reason)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn update_paper_order_status(
        &self,
        order_id: i64,
        status: &str,
        payload: serde_json::Value,
        position_id: Option<i64>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE simulated_orders SET status = $2, payload = $3, position_id = COALESCE($4, position_id) WHERE id = $1",
        )
        .bind(order_id)
        .bind(status)
        .bind(payload)
        .bind(position_id)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn attach_order_position(&self, order_id: i64, position_id: i64) -> Result<()> {
        sqlx::query("UPDATE simulated_orders SET position_id = $2 WHERE id = $1")
            .bind(order_id)
            .bind(position_id)
            .execute(&self.pool)
            .await
            .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_execution_attempt(
        &self,
        order_id: Option<i64>,
        attempt_number: i32,
        status: &str,
        eligible_time: DateTime<Utc>,
        fill_time: Option<DateTime<Utc>>,
        filled_quote: Option<&str>,
        filled_token: Option<&str>,
        reason: Option<&str>,
        payload: serde_json::Value,
        experiment_id: Option<&str>,
        position_id: Option<i64>,
        chain: Option<Chain>,
        token: Option<&str>,
        side: Option<&str>,
        decision_time: Option<DateTime<Utc>>,
        block_number: Option<i64>,
        block_hash: Option<&str>,
        curve_state_id: Option<i64>,
        requested_token_amount: Option<&str>,
        effective_fill_price: Option<&str>,
        price_impact_bps: Option<i32>,
        slippage_bps: Option<i32>,
        protocol_fee: Option<&str>,
        creator_tax: Option<&str>,
        snipe_tax: Option<&str>,
        execution_quality: Option<&str>,
        curve_state_quality: Option<&str>,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO execution_attempts (
                order_id, attempt_number, status, eligible_time, fill_time,
                filled_quote, filled_token, reason, payload,
                experiment_id, position_id, chain, token_address, side, decision_time,
                block_number, block_hash, curve_state_id, requested_token_amount,
                filled_token_amount, quote_received, effective_fill_price,
                price_impact_bps, slippage_bps, protocol_fee, creator_tax, snipe_tax,
                execution_quality, curve_state_quality, failure_reason
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,
                $10,$11,$12,$13,$14,$15,
                $16,$17,$18,$19,
                $20,$21,$22,
                $23,$24,$25,$26,$27,
                $28,$29,$30
            )
            RETURNING id
            "#,
        )
        .bind(order_id)
        .bind(attempt_number)
        .bind(status)
        .bind(eligible_time)
        .bind(fill_time)
        .bind(filled_quote)
        .bind(filled_token)
        .bind(reason)
        .bind(payload)
        .bind(experiment_id)
        .bind(position_id)
        .bind(chain.map(|c| c.as_str().to_string()))
        .bind(token)
        .bind(side)
        .bind(decision_time)
        .bind(block_number)
        .bind(block_hash)
        .bind(curve_state_id)
        .bind(requested_token_amount)
        .bind(filled_token)
        .bind(filled_quote)
        .bind(effective_fill_price)
        .bind(price_impact_bps)
        .bind(slippage_bps)
        .bind(protocol_fee)
        .bind(creator_tax)
        .bind(snipe_tax)
        .bind(execution_quality)
        .bind(curve_state_quality)
        .bind(reason)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn insert_position_event(
        &self,
        position_id: i64,
        kind: &str,
        at: DateTime<Utc>,
        payload: serde_json::Value,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO position_events (position_id, kind, at, payload) VALUES ($1,$2,$3,$4) RETURNING id",
        )
        .bind(position_id)
        .bind(kind)
        .bind(at)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn load_experiment_positions(
        &self,
        experiment_id: &str,
    ) -> Result<Vec<(i64, crate::sim::position::SimulatedPosition)>> {
        let like = crate::lab::pons_exp::experiment_arm_like(experiment_id);
        let rows: Vec<(i64, serde_json::Value)> = sqlx::query_as(
            "SELECT id, payload FROM simulated_positions WHERE strategy_policy_id LIKE $1 ORDER BY id",
        )
        .bind(&like)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, v)| {
                let mut p: crate::sim::position::SimulatedPosition =
                    serde_json::from_value(v).ok()?;
                p.id = id;
                Some((id, p))
            })
            .collect())
    }

    pub async fn load_experiment_orders(
        &self,
        experiment_id: &str,
    ) -> Result<
        Vec<(
            i64,
            String,
            String,
            String,
            String,
            Option<i64>,
            serde_json::Value,
        )>,
    > {
        let like = crate::lab::pons_exp::experiment_arm_like(experiment_id);
        sqlx::query_as(
            r#"
            SELECT id, side, status, token_address, policy_id, position_id, payload
            FROM simulated_orders WHERE policy_id LIKE $1 ORDER BY id
            "#,
        )
        .bind(&like)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_shadow_order(
        &self,
        chain: Chain,
        token: &str,
        launchpad: Launchpad,
        decision_time: DateTime<Utc>,
        side: &str,
        requested_amount: &str,
        status: &str,
        research_valid: bool,
        reason: &str,
        payload: serde_json::Value,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO shadow_orders (
                chain, token_address, launchpad, decision_time, side, requested_amount,
                status, research_valid, reason, payload
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            RETURNING id
            "#,
        )
        .bind(chain.as_str())
        .bind(token)
        .bind(launchpad.as_str())
        .bind(decision_time)
        .bind(side)
        .bind(requested_amount)
        .bind(status)
        .bind(research_valid)
        .bind(reason)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn insert_descriptive_outcome(
        &self,
        o: &crate::sim::descriptive::DescriptiveTokenOutcome,
    ) -> Result<i64> {
        let payload = serde_json::to_value(o).unwrap_or(serde_json::json!({}));
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO descriptive_token_outcomes (
                chain, launchpad, token_address, reference_time, reference_source_price,
                max_source_price_5m, max_source_price_15m, max_source_price_30m, max_source_price_1h,
                max_return_bps, reached_2x, reached_5x, reached_10x, reached_20x,
                time_to_2x_ms, time_to_5x_ms, time_to_10x_ms, time_to_20x_ms,
                label_quality, source, payload, maturity
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22
            )
            RETURNING id
            "#,
        )
        .bind(o.chain.as_str())
        .bind(o.launchpad.as_str())
        .bind(&o.token)
        .bind(o.reference_time)
        .bind(&o.reference_source_price)
        .bind(&o.max_source_price_5m)
        .bind(&o.max_source_price_15m)
        .bind(&o.max_source_price_30m)
        .bind(&o.max_source_price_1h)
        .bind(o.max_return_bps)
        .bind(o.reached_2x)
        .bind(o.reached_5x)
        .bind(o.reached_10x)
        .bind(o.reached_20x)
        .bind(o.time_to_2x_ms)
        .bind(o.time_to_5x_ms)
        .bind(o.time_to_10x_ms)
        .bind(o.time_to_20x_ms)
        .bind(o.quality.as_str())
        .bind(&o.source)
        .bind(payload)
        .bind(o.maturity.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn insert_pons_curve_state(&self, s: &crate::state::PonsCurveState) -> Result<i64> {
        let payload = serde_json::to_value(s).unwrap_or(serde_json::json!({}));
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO pons_curve_states (
                chain, token_address, curve, block_number, block_hash, observed_at,
                virtual_quote_reserve, virtual_token_reserve, real_quote_reserve, real_token_reserve,
                quote_collected, graduation_threshold, progress_bps, status, fee_bps,
                snipe_tax_bps, creator_tax_bps, state_quality, source, abi_version, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21
            )
            ON CONFLICT (curve, block_number) DO UPDATE
            SET observed_at = EXCLUDED.observed_at, payload = EXCLUDED.payload
            RETURNING id
            "#,
        )
        .bind(s.chain.as_str())
        .bind(&s.token)
        .bind(&s.curve)
        .bind(s.block_number.map(|b| b as i64).unwrap_or(0))
        .bind(&s.block_hash)
        .bind(s.observed_at)
        .bind(&s.virtual_quote_reserve)
        .bind(&s.virtual_token_reserve)
        .bind(&s.real_quote_reserve)
        .bind(&s.real_token_reserve)
        .bind(&s.quote_collected)
        .bind(&s.graduation_threshold)
        .bind(s.progress_bps.map(|v| v as i32))
        .bind(s.status.as_str())
        .bind(s.fee_bps as i32)
        .bind(s.snipe_tax_bps.map(|v| v as i32))
        .bind(s.creator_tax_bps as i32)
        .bind(s.state_quality.as_str())
        .bind(&s.source)
        .bind(&s.abi_version)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn censor_pending_outcomes(&self) -> Result<u64> {
        self.censor_pending_outcomes_since(None).await
    }

    pub async fn censor_pending_outcomes_since(&self, since: Option<DateTime<Utc>>) -> Result<u64> {
        let r = if let Some(t) = since {
            sqlx::query(
                "UPDATE descriptive_token_outcomes SET maturity = 'CENSORED_SESSION_END' WHERE maturity = 'PENDING' AND created_at >= $1",
            )
            .bind(t)
            .execute(&self.pool)
            .await
        } else {
            sqlx::query(
                "UPDATE descriptive_token_outcomes SET maturity = 'CENSORED_SESSION_END' WHERE maturity = 'PENDING'",
            )
            .execute(&self.pool)
            .await
        }
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(r.rows_affected())
    }

    pub async fn upsert_exp001_state(&self, st: &crate::lab::pons_exp::Exp001State) -> Result<()> {
        let payload = serde_json::to_value(st).unwrap_or(serde_json::json!({}));
        sqlx::query(
            r#"
            INSERT INTO strategy_experiments (
                experiment_id, name, description, hypothesis, chain, launchpad,
                feature_version, security_policy_version, candidate_policy_version,
                strategy_policy_version, execution_model_version, fee_model_version,
                impact_model_version, slippage_model_version, outcome_model_version,
                position_size_config, exit_policy_config, config_hash, locked_config,
                status, git_commit, locked_at, payload
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23
            )
            ON CONFLICT (experiment_id) DO UPDATE SET
                status = EXCLUDED.status,
                config_hash = EXCLUDED.config_hash,
                locked_config = EXCLUDED.locked_config,
                locked_at = EXCLUDED.locked_at,
                git_commit = EXCLUDED.git_commit,
                payload = EXCLUDED.payload
            "#,
        )
        .bind(&st.lock.experiment_id)
        .bind("Pons prospective EXP001")
        .bind("Locked prospective paper test of Solana-transferred P0-P4 on Pons V2")
        .bind("Do predeclared Solana signals transfer under block-pinned Pons execution?")
        .bind("robinhood")
        .bind("pons_v2")
        .bind(&st.lock.feature_version)
        .bind(&st.lock.security_policy_version)
        .bind(&st.lock.candidate_policy_version)
        .bind(&st.lock.strategy_policy_version)
        .bind(&st.lock.execution_model_version)
        .bind(&st.lock.fee_model_version)
        .bind(&st.lock.impact_model_version)
        .bind(&st.lock.execution_model_version)
        .bind(&st.lock.outcome_model_version)
        .bind(serde_json::json!({ "quote": st.lock.position_size }))
        .bind(serde_json::json!({ "exits": st.lock.exits }))
        .bind(&st.config_hash)
        .bind(st.lock.to_value())
        .bind(st.run_status.as_str())
        .bind(&st.git_commit)
        .bind(st.locked_at)
        .bind(payload)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn load_exp001_state(&self) -> Result<Option<crate::lab::pons_exp::Exp001State>> {
        self.load_experiment_state(crate::lab::pons_exp::EXP001_ID)
            .await
    }

    pub async fn load_experiment_state(
        &self,
        experiment_id: &str,
    ) -> Result<Option<crate::lab::pons_exp::Exp001State>> {
        let row: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT payload FROM strategy_experiments WHERE experiment_id = $1")
                .bind(experiment_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(row.and_then(|v| serde_json::from_value(v).ok()))
    }

    pub async fn session_end_experiment_positions(&self, experiment_id: &str) -> Result<u64> {
        let r = sqlx::query(
            "UPDATE simulated_positions SET status='SESSION_ENDED_OPEN' WHERE status='OPEN' AND strategy_policy_id LIKE $1",
        )
        .bind(crate::lab::pons_exp::experiment_arm_like(experiment_id))
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(r.rows_affected())
    }

    pub async fn close_open_observation_interval(
        &self,
        experiment_id: &str,
        ended_at: DateTime<Utc>,
        status: &str,
        reason: Option<&str>,
    ) -> Result<u64> {
        let r = sqlx::query(
            r#"
            UPDATE experiment_observation_intervals
            SET ended_at = $2, status = $3, gap_reason = COALESCE($4, gap_reason)
            WHERE experiment_id = $1 AND ended_at IS NULL
            "#,
        )
        .bind(experiment_id)
        .bind(ended_at)
        .bind(status)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(r.rows_affected())
    }

    pub async fn open_observation_interval(
        &self,
        experiment_id: &str,
        started_at: DateTime<Utc>,
        status: &str,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO experiment_observation_intervals (experiment_id, started_at, status, heartbeat_at)
            VALUES ($1,$2,$3,$2)
            ON CONFLICT (experiment_id, started_at) DO UPDATE SET heartbeat_at = EXCLUDED.heartbeat_at
            RETURNING id
            "#,
        )
        .bind(experiment_id)
        .bind(started_at)
        .bind(status)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn heartbeat_observation(
        &self,
        experiment_id: &str,
        at: DateTime<Utc>,
        head: Option<u64>,
        healthy: bool,
    ) -> Result<()> {
        let status = if healthy { "VALID" } else { "PARTIAL" };
        sqlx::query(
            r#"
            UPDATE experiment_observation_intervals
            SET heartbeat_at = $2,
                status = CASE WHEN ended_at IS NULL THEN $3 ELSE status END,
                execution_quality = $4
            WHERE experiment_id = $1 AND ended_at IS NULL
            "#,
        )
        .bind(experiment_id)
        .bind(at)
        .bind(status)
        .bind(head.map(|h| h.to_string()))
        .execute(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(())
    }

    pub async fn valid_uptime_secs(&self, experiment_id: &str) -> Result<i64> {
        let v: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
              SUM((EXTRACT(EPOCH FROM (COALESCE(ended_at, heartbeat_at, started_at) - started_at)))::bigint),
              0
            )::bigint
            FROM experiment_observation_intervals
            WHERE experiment_id = $1 AND status = 'VALID'
            "#,
        )
        .bind(experiment_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(v)
    }

    pub async fn insert_experiment_audit(
        &self,
        experiment_id: &str,
        event: &str,
        payload: serde_json::Value,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO experiment_audit (experiment_id, event, payload) VALUES ($1,$2,$3) RETURNING id",
        )
        .bind(experiment_id)
        .bind(event)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn insert_experiment_health(
        &self,
        experiment_id: &str,
        status: &str,
        payload: serde_json::Value,
    ) -> Result<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO experiment_health (experiment_id, status, payload) VALUES ($1,$2,$3) RETURNING id",
        )
        .bind(experiment_id)
        .bind(status)
        .bind(payload)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EngineError::Storage(e.to_string()))?;
        Ok(id)
    }

    pub async fn exp001_counts(
        &self,
        since: DateTime<Utc>,
    ) -> Result<crate::lab::pons_exp::Exp001StatusReport> {
        let tokens: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM token_discovered WHERE chain='robinhood' AND observed_at >= $1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let signals: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM prospective_signals WHERE policy_id LIKE 'P%' AND decision_time >= $1 AND enter",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let orders: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND decision_time >= $2",
        )
        .bind(crate::lab::pons_exp::experiment_arm_like(
            crate::lab::pons_exp::EXP001_ID,
        ))
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let fills: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND decision_time >= $2 AND status IN ('FILLED','PARTIAL_FILL')",
        )
        .bind(crate::lab::pons_exp::experiment_arm_like(
            crate::lab::pons_exp::EXP001_ID,
        ))
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM simulated_positions WHERE strategy_policy_id LIKE $1 AND status IN ('OPEN','SESSION_ENDED_OPEN')",
        )
        .bind(crate::lab::pons_exp::experiment_arm_like(
            crate::lab::pons_exp::EXP001_ID,
        ))
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let closed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM simulated_positions WHERE strategy_policy_id LIKE $1 AND status = 'CLOSED'",
        )
        .bind(crate::lab::pons_exp::experiment_arm_like(
            crate::lab::pons_exp::EXP001_ID,
        ))
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM descriptive_token_outcomes WHERE chain='robinhood' AND maturity='PENDING' AND created_at >= $1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let mature: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM descriptive_token_outcomes WHERE chain='robinhood' AND maturity='MATURE' AND created_at >= $1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let censored: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM descriptive_token_outcomes WHERE chain='robinhood' AND maturity='CENSORED_SESSION_END' AND created_at >= $1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        Ok(crate::lab::pons_exp::Exp001StatusReport {
            experiment_id: crate::lab::pons_exp::EXP001_ID.into(),
            tokens,
            signals,
            orders,
            fills,
            positions_open: open,
            positions_closed: closed,
            outcomes_pending: pending,
            outcomes_mature: mature,
            outcomes_censored: censored,
            note: "operational counts only; no strategy PnL".into(),
            ..Default::default()
        })
    }
}

pub fn _keep_enums_used() {
    let _ = (
        Finality::Unknown,
        Launchpad::Unknown,
        LaunchMechanism::Unknown,
        GraduationModel::Unknown,
        DecoderStatus::Pending,
    );
}
