//! Prospective experiment integrity checks. Fail closed on invariant violations.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::lab::pons_exp::{
    experiment_arm_like, experiment_prefix, is_research_arm, ENTRY_POLICIES, EXIT_POLICIES,
};
use crate::storage::postgres::PostgresStore;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntegrityReport {
    pub experiment_id: String,
    pub ok: bool,
    pub violations: Vec<String>,
    pub entry_orders: i64,
    pub entry_fills: i64,
    pub positions_opened: i64,
    pub positions_closed: i64,
    pub positions_open: i64,
    pub positions_session_ended: i64,
    pub exit_fills: i64,
    pub duplicate_arm_entries: i64,
    pub positions_without_fill: i64,
    pub fills_without_position: i64,
    pub smoke_leaked: i64,
    pub invalid_arms: i64,
    pub pre_start_entries: i64,
    #[serde(default)]
    pub exit_orders: i64,
    #[serde(default)]
    pub closed_without_exit_fill: i64,
    #[serde(default)]
    pub exit_fill_without_position: i64,
    #[serde(default)]
    pub inventory_mismatch: i64,
    #[serde(default)]
    pub pnl_mismatch: i64,
    #[serde(default)]
    pub negative_inventory: i64,
    #[serde(default)]
    pub over_sold_positions: i64,
    #[serde(default)]
    pub failed_exit_marked_closed: i64,
    #[serde(default)]
    pub partial_exit_fills: i64,
    #[serde(default)]
    pub failed_exit_attempts: i64,
    #[serde(default)]
    pub duplicate_entry_fills: i64,
}

impl IntegrityReport {
    pub fn fail(&mut self, msg: impl Into<String>) {
        self.ok = false;
        self.violations.push(msg.into());
    }
}

pub async fn check_experiment(
    store: &PostgresStore,
    experiment_id: &str,
) -> Result<IntegrityReport> {
    let mut r = IntegrityReport {
        experiment_id: experiment_id.into(),
        ok: true,
        ..Default::default()
    };
    let like = experiment_arm_like(experiment_id);

    r.entry_orders = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND side='BUY'",
        &like,
    )
    .await;
    r.entry_fills = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND side='BUY' AND status IN ('FILLED','PARTIAL_FILL')",
        &like,
    )
    .await;
    r.exit_orders = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND side='SELL'",
        &like,
    )
    .await;
    r.exit_fills = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND side='SELL' AND status IN ('FILLED','PARTIAL_FILL')",
        &like,
    )
    .await;
    r.partial_exit_fills = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND side='SELL' AND status='PARTIAL_FILL'",
        &like,
    )
    .await;
    r.failed_exit_attempts = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND side='SELL' AND status NOT IN ('FILLED','PARTIAL_FILL')",
        &like,
    )
    .await;
    r.positions_opened = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_positions WHERE strategy_policy_id LIKE $1",
        &like,
    )
    .await;
    r.positions_closed = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_positions WHERE strategy_policy_id LIKE $1 AND status='CLOSED'",
        &like,
    )
    .await;
    r.positions_open = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_positions WHERE strategy_policy_id LIKE $1 AND status='OPEN'",
        &like,
    )
    .await;
    r.positions_session_ended = scalar(
        store,
        "SELECT COUNT(*) FROM simulated_positions WHERE strategy_policy_id LIKE $1 AND status='SESSION_ENDED_OPEN'",
        &like,
    )
    .await;

    r.duplicate_arm_entries = sqlx::query_scalar(
        r#"
        SELECT COALESCE(COUNT(*),0) FROM (
          SELECT token_address, strategy_policy_id FROM simulated_positions
          WHERE strategy_policy_id LIKE $1
          GROUP BY 1,2 HAVING COUNT(*) > 1
        ) d
        "#,
    )
    .bind(&like)
    .fetch_one(store.pool())
    .await
    .unwrap_or(0);
    if r.duplicate_arm_entries > 0 {
        r.fail(format!(
            "duplicate token+arm positions: {}",
            r.duplicate_arm_entries
        ));
    }

    r.duplicate_entry_fills = sqlx::query_scalar(
        r#"
        SELECT COALESCE(COUNT(*),0) FROM (
          SELECT token_address, policy_id FROM simulated_orders
          WHERE policy_id LIKE $1 AND side='BUY' AND status IN ('FILLED','PARTIAL_FILL')
          GROUP BY 1,2 HAVING COUNT(*) > 1
        ) d
        "#,
    )
    .bind(&like)
    .fetch_one(store.pool())
    .await
    .unwrap_or(0);
    if r.duplicate_entry_fills > 0 {
        r.fail(format!(
            "duplicate token+arm BUY fills: {}",
            r.duplicate_entry_fills
        ));
    }

    r.positions_without_fill = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM simulated_positions p
        WHERE p.strategy_policy_id LIKE $1
          AND NOT EXISTS (
            SELECT 1 FROM simulated_orders o
            WHERE o.token_address = p.token_address
              AND o.policy_id = p.strategy_policy_id
              AND o.side = 'BUY'
              AND o.status IN ('FILLED','PARTIAL_FILL')
          )
        "#,
    )
    .bind(&like)
    .fetch_one(store.pool())
    .await
    .unwrap_or(0);
    if r.positions_without_fill > 0 {
        r.fail(format!(
            "positions without entry fill: {}",
            r.positions_without_fill
        ));
    }

    r.fills_without_position = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM simulated_orders o
        WHERE o.policy_id LIKE $1 AND o.side='BUY' AND o.status IN ('FILLED','PARTIAL_FILL')
          AND NOT EXISTS (
            SELECT 1 FROM simulated_positions p
            WHERE p.token_address = o.token_address AND p.strategy_policy_id = o.policy_id
          )
        "#,
    )
    .bind(&like)
    .fetch_one(store.pool())
    .await
    .unwrap_or(0);
    if r.fills_without_position > 0 {
        r.fail(format!(
            "entry fills without position: {}",
            r.fills_without_position
        ));
    }

    r.smoke_leaked = sqlx::query_scalar(
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id LIKE $1 AND policy_id LIKE '%PIPELINE_SMOKE%'",
    )
    .bind(&like)
    .fetch_one(store.pool())
    .await
    .unwrap_or(0);
    if r.smoke_leaked > 0 {
        r.fail("PIPELINE_SMOKE_POLICY leaked into experiment orders");
    }

    let arms: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT strategy_policy_id FROM simulated_positions WHERE strategy_policy_id LIKE $1",
    )
    .bind(&like)
    .fetch_all(store.pool())
    .await
    .unwrap_or_default();
    for a in &arms {
        if !is_research_arm(a) {
            r.invalid_arms += 1;
            r.fail(format!("invalid research arm: {a}"));
            continue;
        }
        if experiment_prefix(a) != Some(experiment_id) {
            r.fail(format!("cross-experiment leakage: {a}"));
        }
        let parts: Vec<_> = a.split(':').collect();
        if parts.len() != 3
            || !ENTRY_POLICIES.contains(&parts[1])
            || !EXIT_POLICIES.contains(&parts[2])
        {
            r.invalid_arms += 1;
            r.fail(format!("malformed arm: {a}"));
        }
    }

    let skip_exit_fail = experiment_id == crate::lab::pons_exp::EXP001_ID
        || experiment_id == crate::lab::pons_exp::EXP002_QUAL_ID;
    if let Ok(positions) = store.load_experiment_positions(experiment_id).await {
        if let Ok(orders) = store.load_experiment_orders(experiment_id).await {
            use crate::lab::reconcile::{reconcile_position, FillLeg};
            use std::collections::HashMap;
            let mut buys: HashMap<(String, String), Vec<FillLeg>> = HashMap::new();
            let mut sells_by_pos: HashMap<i64, Vec<FillLeg>> = HashMap::new();
            let mut sells_by_arm: HashMap<(String, String), Vec<FillLeg>> = HashMap::new();
            let pos_keys: std::collections::HashSet<(String, String)> = positions
                .iter()
                .map(|(_, p)| (p.token.clone(), p.strategy_policy_id.clone()))
                .collect();
            let pos_ids: std::collections::HashSet<i64> =
                positions.iter().map(|(id, _)| *id).collect();
            for (_id, side, status, token, policy, position_id, payload) in &orders {
                let fill = payload
                    .get("result")
                    .cloned()
                    .unwrap_or_else(|| payload.clone());
                let tok = fill
                    .get("filled_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0")
                    .to_string();
                let q = fill
                    .get("filled_quote")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0")
                    .to_string();
                let is_fill = status == "FILLED" || status == "PARTIAL_FILL";
                if side == "BUY" && is_fill {
                    buys.entry((token.clone(), policy.clone()))
                        .or_default()
                        .push(FillLeg {
                            token: tok,
                            quote: q,
                        });
                } else if side == "SELL" && is_fill {
                    let leg = FillLeg {
                        token: tok,
                        quote: q,
                    };
                    if let Some(pid) = position_id {
                        sells_by_pos.entry(*pid).or_default().push(leg.clone());
                    }
                    sells_by_arm
                        .entry((token.clone(), policy.clone()))
                        .or_default()
                        .push(leg);
                    let linked = position_id.is_some_and(|pid| pos_ids.contains(&pid))
                        || pos_keys.contains(&(token.clone(), policy.clone()));
                    if !linked {
                        r.exit_fill_without_position += 1;
                    }
                }
            }
            for (id, pos) in &positions {
                let b = buys
                    .get(&(pos.token.clone(), pos.strategy_policy_id.clone()))
                    .cloned()
                    .unwrap_or_default();
                let s = sells_by_arm
                    .get(&(pos.token.clone(), pos.strategy_policy_id.clone()))
                    .cloned()
                    .or_else(|| sells_by_pos.get(id).cloned())
                    .unwrap_or_default();
                let rec = reconcile_position(pos, &b, &s);
                if rec.closed_without_exit_fill {
                    r.closed_without_exit_fill += 1;
                }
                if !rec.inventory_ok {
                    r.inventory_mismatch += 1;
                }
                if !rec.pnl_ok {
                    r.pnl_mismatch += 1;
                }
                if rec.negative_inventory {
                    r.negative_inventory += 1;
                }
                if rec.oversold {
                    r.over_sold_positions += 1;
                }
                if rec.failed_exit_marked_closed {
                    r.failed_exit_marked_closed += 1;
                }
            }
            if !skip_exit_fail {
                if r.closed_without_exit_fill > 0 {
                    r.fail(format!(
                        "closed_without_exit_fill={}",
                        r.closed_without_exit_fill
                    ));
                }
                if r.inventory_mismatch > 0 {
                    r.fail(format!("inventory_mismatch={}", r.inventory_mismatch));
                }
                if r.pnl_mismatch > 0 {
                    r.fail(format!("pnl_mismatch={}", r.pnl_mismatch));
                }
                if r.negative_inventory > 0 {
                    r.fail(format!("negative_inventory={}", r.negative_inventory));
                }
                if r.over_sold_positions > 0 {
                    r.fail(format!("over_sold_positions={}", r.over_sold_positions));
                }
                if r.failed_exit_marked_closed > 0 {
                    r.fail(format!(
                        "failed_exit_marked_closed={}",
                        r.failed_exit_marked_closed
                    ));
                }
                if r.exit_fill_without_position > 0 {
                    r.fail(format!(
                        "exit_fill_without_position={}",
                        r.exit_fill_without_position
                    ));
                }
            }
        }
    }

    let opened = r.positions_opened;
    let accounted = r.positions_closed + r.positions_open + r.positions_session_ended;
    if opened != accounted {
        r.fail(format!(
            "position lifecycle mismatch: opened={opened} closed+open+session_ended={accounted}"
        ));
    }
    if r.positions_opened != r.entry_fills {
        r.fail(format!(
            "positions_opened ({}) != entry_fills ({})",
            r.positions_opened, r.entry_fills
        ));
    }

    let st = store
        .load_experiment_state(experiment_id)
        .await
        .ok()
        .flatten();
    if let Some(st) = st {
        if let Some(start) = st.started_at {
            r.pre_start_entries = sqlx::query_scalar(
                r#"
                SELECT COUNT(*) FROM simulated_orders o
                JOIN token_discovered d
                  ON d.chain=o.chain AND d.token_address=o.token_address
                WHERE o.policy_id LIKE $1 AND o.side='BUY'
                  AND d.observed_at < $2
                "#,
            )
            .bind(&like)
            .bind(start)
            .fetch_one(store.pool())
            .await
            .unwrap_or(0);
            if r.pre_start_entries > 0 && experiment_id != crate::lab::pons_exp::EXP001_ID {
                r.fail(format!("pre-start token entries: {}", r.pre_start_entries));
            }
        }
    }

    Ok(r)
}

async fn scalar(store: &PostgresStore, sql: &str, like: &str) -> i64 {
    sqlx::query_scalar(sql)
        .bind(like)
        .fetch_one(store.pool())
        .await
        .unwrap_or(0)
}
