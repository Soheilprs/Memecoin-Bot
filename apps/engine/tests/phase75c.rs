use chrono::{TimeZone, Utc};
use memecoin_engine::candidate::CandidateState;
use memecoin_engine::domain::{Chain, Launchpad};
use memecoin_engine::features::engine::{FeatureEngine, FeatureInput};
use memecoin_engine::lab::pons_exp::{
    arm_id_for, Exp001State, ExpRunStatus, EXP002_ID, EXP003_ID, EXP003_RESTARTQUAL_ID,
};
use memecoin_engine::live::{
    queue_exp001_arms, restore_open_positions_prefixed, wall_clock_snap, LiveResearchRuntime,
};
use memecoin_engine::sim::exec::ExecutionResult;
use memecoin_engine::sim::position::SimulatedPosition;
use memecoin_engine::sim::types::{
    ExecutionQuality, ExecutionStatus, ExitReason, OrderSide, PositionStatus,
};
use memecoin_engine::strategy::StrategyContext;

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

fn fill(token: &str, quote: &str) -> ExecutionResult {
    let mut r = ExecutionResult::empty(
        OrderSide::Buy,
        ts(1_000),
        ts(1_100),
        quote.into(),
        token.into(),
        ExecutionStatus::Filled,
        ExecutionQuality::Modelled,
        true,
        "",
        1,
        0,
    );
    r.filled_token = token.into();
    r.filled_quote = quote.into();
    r.actual_simulated_fill_time = Some(ts(1_100));
    r
}

fn arm(exp: &str) -> String {
    arm_id_for(exp, "P0_FIRST_ELIGIBLE_CONTROL", "X2_TIME_5M")
}

fn open_pos(exp: &str, token: &str) -> SimulatedPosition {
    SimulatedPosition::open(
        1,
        Chain::Robinhood,
        token.into(),
        Launchpad::PonsV2,
        arm(exp),
        &fill("1000", "1000000000"),
        None,
        None,
    )
}

fn eligible_ctx<'a>(
    features: &'a memecoin_engine::features::FeatureVector,
    token: &'a str,
) -> StrategyContext<'a> {
    StrategyContext {
        features: Some(features),
        candidate: CandidateState::Eligible,
        security: None,
        first_eligible_at: Some(features.as_of_time),
        now: features.as_of_time,
        token,
        seed: 1,
    }
}

fn queue_eligible(runtime: &mut LiveResearchRuntime, token: &str) {
    let mut snap = wall_clock_snap(Chain::Robinhood, token, Launchpad::PonsV2, ts(30_000));
    snap.age_ms = 30_000;
    let feat = FeatureEngine::compute(FeatureInput::from_history(&snap, &[], None));
    let ctx = eligible_ctx(&feat, token);
    queue_exp001_arms(runtime, &snap, &[], &ctx, None, None);
}

fn pending_arms(runtime: &LiveResearchRuntime) -> Vec<String> {
    runtime.pending.iter().map(|p| p.arm_id.clone()).collect()
}

#[test]
fn closed_position_must_remain_claimed_in_memory() {
    let mut runtime = LiveResearchRuntime::new_mode(true, true);
    runtime.experiment_id = Some(EXP003_ID.into());
    let mut p = open_pos(EXP003_ID, "0xclosed");
    p.status = PositionStatus::Closed;
    runtime
        .entered_arms
        .insert((p.chain, p.token.clone(), p.strategy_policy_id.clone()));
    queue_eligible(&mut runtime, "0xclosed");
    assert!(
        !pending_arms(&runtime).contains(&arm(EXP003_ID)),
        "reentry=false must not queue a second BUY after a closed position"
    );
}

#[test]
fn open_position_must_not_queue_second_buy() {
    let mut runtime = LiveResearchRuntime::new_mode(true, true);
    runtime.experiment_id = Some(EXP003_ID.into());
    let p = open_pos(EXP003_ID, "0xopen");
    runtime
        .entered_arms
        .insert((p.chain, p.token.clone(), p.strategy_policy_id.clone()));
    queue_eligible(&mut runtime, "0xopen");
    assert!(!pending_arms(&runtime).contains(&arm(EXP003_ID)));
}

#[test]
fn candidate_replay_does_not_requeue_entered_arm() {
    let mut runtime = LiveResearchRuntime::new_mode(true, true);
    runtime.experiment_id = Some(EXP003_RESTARTQUAL_ID.into());
    queue_eligible(&mut runtime, "0xreplay");
    let first = pending_arms(&runtime);
    assert!(first.contains(&arm(EXP003_RESTARTQUAL_ID)));
    runtime.pending.clear();
    queue_eligible(&mut runtime, "0xreplay");
    assert!(
        !pending_arms(&runtime).contains(&arm(EXP003_RESTARTQUAL_ID)),
        "candidate rebuild / second snapshot must not emit another BUY"
    );
}

#[test]
fn observation_overlap_does_not_requeue() {
    let mut runtime = LiveResearchRuntime::new_mode(true, true);
    runtime.experiment_id = Some(EXP003_ID.into());
    queue_eligible(&mut runtime, "0xoverlap");
    queue_eligible(&mut runtime, "0xoverlap");
    let n = runtime
        .pending
        .iter()
        .filter(|p| p.arm_id == arm(EXP003_ID))
        .count();
    assert_eq!(n, 1);
}

#[test]
fn multiple_in_memory_restarts_keep_one_lifetime_entry() {
    let mut runtime = LiveResearchRuntime::new_mode(true, true);
    runtime.experiment_id = Some(EXP003_ID.into());
    let token = "0xmulti";
    queue_eligible(&mut runtime, token);
    assert_eq!(
        runtime
            .pending
            .iter()
            .filter(|p| p.arm_id == arm(EXP003_ID))
            .count(),
        1
    );
    for _ in 0..5 {
        runtime.pending.clear();
        queue_eligible(&mut runtime, token);
        assert!(!pending_arms(&runtime).contains(&arm(EXP003_ID)));
    }
}

#[test]
fn invalidated_status_is_terminal() {
    let mut st = Exp001State::locked_for(EXP002_ID, None);
    st.run_status = ExpRunStatus::Invalidated;
    st.pause_reason = Some("RESTART_ENTRY_IDEMPOTENCY".into());
    assert_eq!(st.run_status.as_str(), "INVALIDATED");
    assert_eq!(st.lock.experiment_id, EXP002_ID);
    assert!(!st.lock.reentry);
}

#[tokio::test]
async fn postgres_restart_idempotency_suite() {
    use sqlx::postgres::PgPoolOptions;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;

    let node = match Postgres::default().start().await {
        Ok(n) => n,
        Err(err) => {
            eprintln!("SKIP postgres tests; docker unavailable: {err}");
            return;
        }
    };
    let host_port = node.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{host_port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .expect("connect");
    let store = memecoin_engine::storage::postgres::PostgresStore::from_pool(pool);
    store.migrate().await.expect("migrate 0017");

    let exp = EXP003_RESTARTQUAL_ID;
    let token_open = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let token_closed = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let token_claim = "0xcccccccccccccccccccccccccccccccccccccccc";
    let token_crash_order = "0xdddddddddddddddddddddddddddddddddddddddd";
    let token_crash_attempt = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let token_conc = "0xffffffffffffffffffffffffffffffffffffffff";
    let a = arm(exp);

    // A. existing OPEN position + process restart
    let pos_open = open_pos(exp, token_open);
    store
        .insert_open_paper_position(&pos_open)
        .await
        .expect("open pos");
    let mut rt = LiveResearchRuntime::new_mode(true, true);
    rt.experiment_id = Some(exp.into());
    restore_open_positions_prefixed(&store, &mut rt, Some(exp))
        .await
        .expect("restore open");
    assert!(rt
        .entered_arms
        .contains(&(Chain::Robinhood, token_open.into(), a.clone())));
    queue_eligible(&mut rt, token_open);
    assert!(!pending_arms(&rt).contains(&a));
    let buys = count_buys(&store, &a, token_open).await;
    assert_eq!(buys, 0, "restore of open position must not persist a BUY");

    // B. previously CLOSED position + restart
    let mut pos_closed = open_pos(exp, token_closed);
    let sell = {
        let mut s = fill("1000", "900000000");
        s.side = OrderSide::Sell;
        s
    };
    pos_closed.apply_exit(&sell, ExitReason::TimeStop, true);
    assert_eq!(pos_closed.status, PositionStatus::Closed);
    store
        .insert_open_paper_position(&pos_closed)
        .await
        .expect("closed pos");
    let mut rt = LiveResearchRuntime::new_mode(true, true);
    rt.experiment_id = Some(exp.into());
    restore_open_positions_prefixed(&store, &mut rt, Some(exp))
        .await
        .expect("restore closed");
    assert!(
        rt.entered_arms
            .contains(&(Chain::Robinhood, token_closed.into(), a.clone())),
        "CLOSED positions must restore into entered_arms"
    );
    assert!(
        !rt.positions
            .iter()
            .any(|p| p.token == token_closed && p.status == PositionStatus::Open),
        "CLOSED positions are not reopened for exit management"
    );
    queue_eligible(&mut rt, token_closed);
    assert!(!pending_arms(&rt).contains(&a));

    // C/D. candidate replay + observation overlap after restore
    queue_eligible(&mut rt, token_closed);
    queue_eligible(&mut rt, token_closed);
    assert_eq!(
        rt.pending
            .iter()
            .filter(|p| p.arm_id == a && p.token == token_closed)
            .count(),
        0
    );

    // F. crash after claim+order, before execution attempt / position
    let payload = serde_json::json!({"result": fill("1000", "1000000000")});
    let oid = store
        .insert_paper_order_claimed(
            &a,
            Chain::Robinhood,
            token_crash_order,
            "BUY",
            ts(2_000),
            "1000000000",
            "FILLED",
            None,
            None,
            payload.clone(),
            Some(exp),
            None,
            None,
        )
        .await
        .expect("claimed buy")
        .expect("won claim");
    let again = store
        .insert_paper_order_claimed(
            &a,
            Chain::Robinhood,
            token_crash_order,
            "BUY",
            ts(3_000),
            "1000000000",
            "FILLED",
            None,
            None,
            payload.clone(),
            Some(exp),
            None,
            None,
        )
        .await
        .expect("second claim");
    assert!(again.is_none(), "second BUY must lose the claim");
    assert_eq!(count_buys(&store, &a, token_crash_order).await, 1);

    // crash after fill, before position: restore reconstructs exactly one position
    let mut rt = LiveResearchRuntime::new_mode(true, true);
    rt.experiment_id = Some(exp.into());
    restore_open_positions_prefixed(&store, &mut rt, Some(exp))
        .await
        .expect("restore orphan fill");
    assert!(rt
        .positions
        .iter()
        .any(|p| p.token == token_crash_order && p.strategy_policy_id == a));
    assert_eq!(count_positions(&store, &a, token_crash_order).await, 1);
    queue_eligible(&mut rt, token_crash_order);
    assert!(!pending_arms(&rt).contains(&a));
    let _ = oid;

    // F. crash after order + execution_attempt, before position
    let oid2 = store
        .insert_paper_order_claimed(
            &a,
            Chain::Robinhood,
            token_crash_attempt,
            "BUY",
            ts(5_000),
            "1000000000",
            "FILLED",
            None,
            None,
            payload.clone(),
            Some(exp),
            None,
            None,
        )
        .await
        .expect("claimed buy+attempt")
        .expect("won claim");
    store
        .insert_execution_attempt(
            Some(oid2),
            1,
            "FILLED",
            ts(5_000),
            Some(ts(5_100)),
            Some("1000000000"),
            Some("1000"),
            None,
            payload.clone(),
            Some(exp),
            None,
            Some(Chain::Robinhood),
            Some(token_crash_attempt),
            Some("BUY"),
            Some(ts(5_000)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("attempt");
    let mut rt = LiveResearchRuntime::new_mode(true, true);
    rt.experiment_id = Some(exp.into());
    restore_open_positions_prefixed(&store, &mut rt, Some(exp))
        .await
        .expect("restore after attempt");
    assert_eq!(count_buys(&store, &a, token_crash_attempt).await, 1);
    assert_eq!(count_positions(&store, &a, token_crash_attempt).await, 1);
    queue_eligible(&mut rt, token_crash_attempt);
    assert!(!rt
        .pending
        .iter()
        .any(|p| p.arm_id == a && p.token == token_crash_attempt));

    // F. crash after claim only (no order yet)
    assert!(store
        .claim_arm_entry(exp, Chain::Robinhood, token_claim, &a, "CLAIM")
        .await
        .unwrap());
    assert!(!store
        .claim_arm_entry(exp, Chain::Robinhood, token_claim, &a, "CLAIM")
        .await
        .unwrap());
    let mut rt = LiveResearchRuntime::new_mode(true, true);
    rt.experiment_id = Some(exp.into());
    restore_open_positions_prefixed(&store, &mut rt, Some(exp))
        .await
        .unwrap();
    assert!(rt
        .entered_arms
        .contains(&(Chain::Robinhood, token_claim.into(), a.clone())));
    let skipped = store
        .insert_paper_order_claimed(
            &a,
            Chain::Robinhood,
            token_claim,
            "BUY",
            ts(4_000),
            "1000000000",
            "FILLED",
            None,
            None,
            payload.clone(),
            Some(exp),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(skipped.is_none());
    assert_eq!(count_buys(&store, &a, token_claim).await, 0);

    // F. order + attempt, no position already covered by reconstruct above.
    // G. multiple restores still one lifetime entry
    for _ in 0..3 {
        let mut rt = LiveResearchRuntime::new_mode(true, true);
        rt.experiment_id = Some(exp.into());
        restore_open_positions_prefixed(&store, &mut rt, Some(exp))
            .await
            .unwrap();
        queue_eligible(&mut rt, token_closed);
        queue_eligible(&mut rt, token_open);
        queue_eligible(&mut rt, token_crash_order);
        assert!(!rt.pending.iter().any(|p| p.arm_id == a
            && (p.token == token_closed || p.token == token_open || p.token == token_crash_order)));
    }
    assert_eq!(count_buys(&store, &a, token_open).await, 0);
    assert_eq!(count_buys(&store, &a, token_closed).await, 0);
    assert_eq!(count_buys(&store, &a, token_crash_order).await, 1);
    assert_eq!(count_positions(&store, &a, token_crash_order).await, 1);

    // E. concurrent entry evaluations: exactly one BUY path wins
    let conc_arm = arm_id_for(exp, "P1_SOLANA_BUYERS_3_30S", "X2_TIME_5M");
    let store_c = store.clone();
    let conc_arm_c = conc_arm.clone();
    let mut joins = Vec::new();
    for i in 0..8 {
        let s = store_c.clone();
        let arm = conc_arm_c.clone();
        joins.push(tokio::spawn(async move {
            s.insert_paper_order_claimed(
                &arm,
                Chain::Robinhood,
                token_conc,
                "BUY",
                ts(10_000 + i),
                "1000000000",
                "FILLED",
                None,
                None,
                serde_json::json!({"result": fill("1000", "1000000000"), "i": i}),
                Some(exp),
                None,
                None,
            )
            .await
            .expect("conc insert")
        }));
    }
    let mut wins = 0u32;
    for j in joins {
        if j.await.expect("join").is_some() {
            wins += 1;
        }
    }
    assert_eq!(wins, 1, "exactly one concurrent BUY claim must win");
    assert_eq!(count_buys(&store, &conc_arm, token_conc).await, 1);

    // last-resort unique index: bypassing claim still cannot persist a second EXP003 BUY
    let dup = store
        .insert_paper_order_ex(
            &conc_arm,
            Chain::Robinhood,
            token_conc,
            "BUY",
            ts(20_000),
            "1000000000",
            "FILLED",
            None,
            None,
            payload,
            Some(exp),
            None,
            None,
        )
        .await;
    assert!(
        dup.is_err(),
        "DB unique index must reject a second EXP003 BUY"
    );
}

async fn count_buys(
    store: &memecoin_engine::storage::postgres::PostgresStore,
    policy: &str,
    token: &str,
) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM simulated_orders WHERE policy_id=$1 AND token_address=$2 AND side='BUY'",
    )
    .bind(policy)
    .bind(token)
    .fetch_one(store.pool())
    .await
    .unwrap_or(0)
}

async fn count_positions(
    store: &memecoin_engine::storage::postgres::PostgresStore,
    policy: &str,
    token: &str,
) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM simulated_positions WHERE strategy_policy_id=$1 AND token_address=$2",
    )
    .bind(policy)
    .bind(token)
    .fetch_one(store.pool())
    .await
    .unwrap_or(0)
}
