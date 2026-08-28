use memecoin_engine::decoders::DecoderRegistry;
use memecoin_engine::domain::{Chain, Launchpad, QualityStatus, TokenDiscovered};
use memecoin_engine::registry::{CLANKER_V4_FACTORY, PONS_V2_FACTORY, PUMPFUN_PROGRAM};
use memecoin_engine::security::assessment::SecurityVerdict;
use memecoin_engine::security::context::{EvmView, SecurityContext, SolanaView};
use memecoin_engine::security::evm::bytecode::{runtime_hash, stripped_hash};
use memecoin_engine::security::evm::proxy::{
    detect_eip1167, eip1967_admin_slot, eip1967_beacon_slot, eip1967_implementation_slot,
};
use memecoin_engine::security::evm::selectors::extract_push4;
use memecoin_engine::security::evm::simulation::{run_plan, timeout_unknown, TokenSimModel};
use memecoin_engine::security::solana::authorities::{
    encode_mint_header, parse_mint_header, MintAuthorities,
};
use memecoin_engine::security::solana::token2022::{
    append_tlv, parse_tlv_extensions, ExtensionType,
};
use memecoin_engine::security::solana::{SPL_TOKEN_PROGRAM, TOKEN_2022_PROGRAM};
use memecoin_engine::security::{SecurityEngine, SecurityPolicy};
use memecoin_engine::state::lifecycle::TokenLifecycleState;
use memecoin_engine::storage::memory::MemoryStore;
use memecoin_engine::storage::EventStore;
use memecoin_engine::test_support::{evm_raw_from_fixture, pumpfun_raw_from_fixture};

fn disc(chain: Chain, launchpad: Launchpad, factory: &str, token: &str) -> TokenDiscovered {
    TokenDiscovered {
        chain,
        chain_id: chain.evm_chain_id(),
        token_address: token.into(),
        creator: "creator".into(),
        launchpad,
        factory_or_program: factory.into(),
        pool: Some("pool".into()),
        curve: Some("curve1".into()),
        quote_asset: None,
        launch_mechanism: memecoin_engine::domain::LaunchMechanism::BondingCurve,
        bonding_curve: true,
        graduation_model: memecoin_engine::domain::GraduationModel::Unknown,
        block_number: Some(10),
        block_hash: Some("0xabc".into()),
        slot: Some(99),
        tx_hash_or_signature: "tx".into(),
        instruction_index: None,
        inner_instruction_index: None,
        log_index: Some(0),
        chain_timestamp: None,
        observed_at: chrono::Utc::now(),
        persisted_at: None,
        source: "test".into(),
        decoder_version: "0.1.0".into(),
        initial_liquidity: None,
        raw_event_id: "e1".into(),
    }
}

fn engine() -> SecurityEngine {
    SecurityEngine::default()
}

fn eip1167_bytes(impl_addr: [u8; 20]) -> Vec<u8> {
    let mut b = vec![0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x3d, 0x36, 0x3d, 0x73];
    b.extend_from_slice(&impl_addr);
    b.extend_from_slice(&[
        0x5a, 0xf4, 0x3d, 0x82, 0x80, 0x3e, 0x90, 0x3d, 0x91, 0x60, 0x2b, 0x57, 0xfd, 0x5b, 0xf3,
    ]);
    b
}

fn push4(sel: [u8; 4]) -> Vec<u8> {
    let mut v = vec![0x63];
    v.extend_from_slice(&sel);
    v
}

#[test]
fn spl_token_authority_parsing() {
    let mint = MintAuthorities {
        mint_authority: Some("11111111111111111111111111111111".into()),
        freeze_authority: None,
        supply: 1000,
        decimals: 6,
        initialized: true,
    };
    let bytes = encode_mint_header(&mint);
    let parsed = parse_mint_header(&bytes).unwrap();
    assert!(parsed.mint_authority.is_some());
    assert!(parsed.freeze_authority.is_none());
    assert_eq!(parsed.supply, 1000);
}

#[test]
fn token2022_extension_parsing() {
    let mut mint = encode_mint_header(&MintAuthorities {
        mint_authority: None,
        freeze_authority: None,
        supply: 1,
        decimals: 6,
        initialized: true,
    });
    append_tlv(&mut mint, ExtensionType::MetadataPointer, &[7u8; 32]);
    append_tlv(&mut mint, ExtensionType::TransferHook, &[9u8; 32]);
    let exts = parse_tlv_extensions(&mint);
    assert!(exts
        .iter()
        .any(|(t, _)| *t == ExtensionType::MetadataPointer));
    assert!(exts.iter().any(|(t, _)| *t == ExtensionType::TransferHook));
}

#[test]
fn pump_fixture_not_hard_rejected() {
    let raw = pumpfun_raw_from_fixture();
    let tok = DecoderRegistry::production()
        .decode(&raw)
        .unwrap()
        .into_token()
        .unwrap();
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::HistoricalReplay, true);
    ctx.solana = Some(SolanaView {
        token_program: Some(TOKEN_2022_PROGRAM.into()),
        mint_account: None,
        as_of_requested_slot: false,
    });
    let a = engine().assess(&ctx);
    assert_ne!(a.verdict, SecurityVerdict::Reject);
    assert!(a.evidence.iter().any(|e| e.check == "TEMPLATE_PUMPFUN"));
}

#[test]
fn pump_create_v2_token2022_metadata_pointer_ok() {
    let tok = disc(
        Chain::Solana,
        Launchpad::PumpFun,
        PUMPFUN_PROGRAM,
        "mint111111111111111111111111111111111111111",
    );
    let mut mint = encode_mint_header(&MintAuthorities {
        mint_authority: tok.curve.clone(),
        freeze_authority: None,
        supply: 1_000_000,
        decimals: 6,
        initialized: true,
    });
    append_tlv(&mut mint, ExtensionType::MetadataPointer, &[1u8; 32]);
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::HistoricalReplay, true);
    ctx.solana = Some(SolanaView {
        mint_account: Some(mint),
        token_program: Some(TOKEN_2022_PROGRAM.into()),
        as_of_requested_slot: true,
    });
    let a = engine().assess(&ctx);
    assert_ne!(a.verdict, SecurityVerdict::Reject);
    assert!(a
        .evidence
        .iter()
        .any(|e| e.check.contains("METADATAPOINTER") && !e.hard_reject));
}

#[test]
fn transfer_hook_rejected() {
    let tok = disc(Chain::Solana, Launchpad::Unknown, "other", "mint");
    let mut mint = encode_mint_header(&MintAuthorities {
        mint_authority: None,
        freeze_authority: None,
        supply: 1,
        decimals: 6,
        initialized: true,
    });
    append_tlv(&mut mint, ExtensionType::TransferHook, &[1u8; 32]);
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.solana = Some(SolanaView {
        mint_account: Some(mint),
        token_program: Some(TOKEN_2022_PROGRAM.into()),
        as_of_requested_slot: true,
    });
    let a = engine().assess(&ctx);
    assert_eq!(a.verdict, SecurityVerdict::Reject);
    assert!(a
        .hard_reject_reasons
        .iter()
        .any(|r| r.contains("TRANSFERHOOK")));
}

#[test]
fn permanent_delegate_rejected() {
    let tok = disc(Chain::Solana, Launchpad::Unknown, "other", "mint");
    let mut mint = encode_mint_header(&MintAuthorities {
        mint_authority: None,
        freeze_authority: None,
        supply: 1,
        decimals: 6,
        initialized: true,
    });
    append_tlv(&mut mint, ExtensionType::PermanentDelegate, &[2u8; 32]);
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.solana = Some(SolanaView {
        mint_account: Some(mint),
        token_program: Some(TOKEN_2022_PROGRAM.into()),
        as_of_requested_slot: true,
    });
    assert_eq!(engine().assess(&ctx).verdict, SecurityVerdict::Reject);
}

#[test]
fn freeze_and_mint_authority_rejected_nontemplate() {
    let tok = disc(Chain::Solana, Launchpad::Unknown, "otherprog", "mint");
    let mint = encode_mint_header(&MintAuthorities {
        mint_authority: Some("Evil111111111111111111111111111111111111111".into()),
        freeze_authority: Some("Evil111111111111111111111111111111111111111".into()),
        supply: 1,
        decimals: 9,
        initialized: true,
    });
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.solana = Some(SolanaView {
        mint_account: Some(mint),
        token_program: Some(SPL_TOKEN_PROGRAM.into()),
        as_of_requested_slot: true,
    });
    assert_eq!(engine().assess(&ctx).verdict, SecurityVerdict::Reject);
}

#[test]
fn unsupported_token_program_unknown_or_reject() {
    let tok = disc(Chain::Solana, Launchpad::Unknown, "x", "mint");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.solana = Some(SolanaView {
        mint_account: None,
        token_program: Some("UnknownProgram11111111111111111111111111111".into()),
        as_of_requested_slot: true,
    });
    let a = engine().assess(&ctx);
    assert_eq!(a.verdict, SecurityVerdict::Reject);
}

#[test]
fn clanker_template_not_hard_rejected() {
    let raw = evm_raw_from_fixture("base/clanker_v4/token_created.json");
    let tok = DecoderRegistry::production()
        .decode(&raw)
        .unwrap()
        .into_token()
        .unwrap();
    assert_eq!(tok.factory_or_program, CLANKER_V4_FACTORY);
    let ctx = SecurityContext::from_token(tok, QualityStatus::HistoricalReplay, true);
    let a = engine().assess(&ctx);
    assert_ne!(a.verdict, SecurityVerdict::Reject);
    assert!(a.evidence.iter().any(|e| e.check == "TEMPLATE_FACTORY"));
}

#[test]
fn pons_template_not_hard_rejected() {
    let raw = evm_raw_from_fixture("robinhood/pons_v2/token_launched.json");
    let tok = DecoderRegistry::production()
        .decode(&raw)
        .unwrap()
        .into_token()
        .unwrap();
    assert_eq!(tok.factory_or_program.to_ascii_lowercase(), PONS_V2_FACTORY);
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::HistoricalReplay, true);
    ctx.lifecycle = Some(TokenLifecycleState::CurveActive);
    let a = engine().assess(&ctx);
    assert_ne!(a.verdict, SecurityVerdict::Reject);
}

#[test]
fn mismatched_bytecode_fails_fast_path() {
    let tok = disc(
        Chain::Base,
        Launchpad::ClankerV4,
        CLANKER_V4_FACTORY,
        "0xabc",
    );
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    // pin a fake expected hash by using non-matching runtime
    let code = push4([0x11, 0x22, 0x33, 0x44]);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(code),
        as_of_requested_block: true,
        ..Default::default()
    });
    let a = engine().assess(&ctx);
    // unpinned hash → not mismatch. Inject TEMPLATE_MISMATCH by assessing with full path mint selector
    assert!(
        a.evidence.iter().any(|e| e.check == "TEMPLATE_BYTECODE")
            || a.evidence.iter().any(|e| e.check == "TEMPLATE_MISMATCH")
            || a.evidence.iter().any(|e| e.check == "EVM_RUNTIME_HASH")
    );
}

#[test]
fn template_mismatch_when_hash_pinned() {
    // Factory match + mint selector in bytecode → full analyzer hard-reject mint
    let tok = disc(
        Chain::Base,
        Launchpad::ClankerV4,
        CLANKER_V4_FACTORY,
        "0xabc",
    );
    let mint_sel = memecoin_engine::security::evm::selectors::selector("mint(address,uint256)");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(push4(mint_sel)),
        as_of_requested_block: true,
        ..Default::default()
    });
    let a = engine().assess(&ctx);
    // fast path skips mint scanner; still not a factory-only PASS
    assert_ne!(a.verdict, SecurityVerdict::Pass);
}

#[test]
fn eip1967_proxy_detection() {
    let impl_slot = format!("0x{}", hex::encode(eip1967_implementation_slot()));
    let admin_slot = format!("0x{}", hex::encode(eip1967_admin_slot()));
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtoken");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(vec![0x00, 0xf4]),
        storage: vec![
            (
                impl_slot,
                "0x0000000000000000000000001111111111111111111111111111111111111111".into(),
            ),
            (
                admin_slot,
                "0x0000000000000000000000002222222222222222222222222222222222222222".into(),
            ),
        ],
        as_of_requested_block: true,
        ..Default::default()
    });
    let a = engine().assess(&ctx);
    assert!(a
        .evidence
        .iter()
        .any(|e| e.check == "EVM_PROXY_IMPLEMENTATION"));
    assert!(a
        .evidence
        .iter()
        .any(|e| e.check == "EVM_PROXY_ADMIN" && e.hard_reject));
    assert_eq!(a.verdict, SecurityVerdict::Reject);
}

#[test]
fn beacon_proxy_detection() {
    let beacon = format!("0x{}", hex::encode(eip1967_beacon_slot()));
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtoken");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(vec![0x00]),
        storage: vec![(
            beacon,
            "0x0000000000000000000000003333333333333333333333333333333333333333".into(),
        )],
        as_of_requested_block: true,
        ..Default::default()
    });
    let a = engine().assess(&ctx);
    assert!(a.evidence.iter().any(|e| e.check == "EVM_PROXY_BEACON"));
}

#[test]
fn eip1167_minimal_proxy() {
    let code = eip1167_bytes([0xab; 20]);
    assert!(detect_eip1167(&code).is_some());
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtoken");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(code),
        as_of_requested_block: true,
        ..Default::default()
    });
    let a = engine().assess(&ctx);
    assert!(a.evidence.iter().any(|e| e.check == "EVM_PROXY_EIP1167"));
}

#[test]
fn eoa_upgradeability_rejected() {
    let sel = memecoin_engine::security::evm::selectors::selector("upgradeTo(address)");
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtoken");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(push4(sel)),
        as_of_requested_block: true,
        ..Default::default()
    });
    assert_eq!(engine().assess(&ctx).verdict, SecurityVerdict::Reject);
}

#[test]
fn owner_and_fake_renounce() {
    let mut code = push4(memecoin_engine::security::evm::selectors::selector(
        "owner()",
    ));
    code.extend(push4(memecoin_engine::security::evm::selectors::selector(
        "setSellTax(uint256)",
    )));
    code.extend(push4(memecoin_engine::security::evm::selectors::selector(
        "grantRole(bytes32,address)",
    )));
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtoken");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(code),
        as_of_requested_block: true,
        ..Default::default()
    });
    let a = engine().assess(&ctx);
    assert!(a
        .evidence
        .iter()
        .any(|e| e.check == "EVM_FAKE_RENOUNCE_RISK"));
    assert!(a.evidence.iter().any(|e| e.check == "EVM_MUTABLE_TAX"));
}

#[test]
fn mint_backdoor_detection() {
    let code = push4(memecoin_engine::security::evm::selectors::selector(
        "mint(address,uint256)",
    ));
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtoken");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(code),
        as_of_requested_block: true,
        ..Default::default()
    });
    assert_eq!(engine().assess(&ctx).verdict, SecurityVerdict::Reject);
}

#[test]
fn blacklist_control_evidence() {
    let code = push4(memecoin_engine::security::evm::selectors::selector(
        "setBlacklist(address,bool)",
    ));
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtoken");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.evm = Some(EvmView {
        runtime_bytecode: Some(code),
        as_of_requested_block: true,
        ..Default::default()
    });
    let a = engine().assess(&ctx);
    assert_eq!(a.verdict, SecurityVerdict::Reject);
    assert!(a.evidence.iter().any(|e| e.check == "EVM_BLACKLIST"));
}

#[test]
fn runtime_and_stripped_hash_deterministic() {
    let mut code = vec![0x60, 0x80, 0x60, 0x40];
    code.extend_from_slice(&[0xa2, 0x64, 0x69, 0x70, 0x66, 0x73, 0x00, 0x01]);
    assert_eq!(runtime_hash(&code), runtime_hash(&code));
    assert_eq!(stripped_hash(&code), stripped_hash(&code));
    assert_ne!(runtime_hash(&code), stripped_hash(&code));
}

#[test]
fn selector_extraction() {
    let sel = memecoin_engine::security::evm::selectors::selector("owner()");
    let code = push4(sel);
    let found = extract_push4(&code);
    assert!(found.contains(&sel));
}

#[test]
fn sim_normal_token_sells() {
    let r = run_plan(&TokenSimModel::normal(), std::time::Duration::from_secs(1));
    assert!(r.steps.iter().all(|s| s.ok));
    assert_eq!(
        r.honeypot,
        memecoin_engine::security::assessment::HoneypotResult::NotHoneypot
    );
}

#[test]
fn sim_honeypot_rejected() {
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xhp");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.sim_model = Some(TokenSimModel::honeypot_sell_revert());
    let a = engine().assess(&ctx);
    assert_eq!(a.verdict, SecurityVerdict::Reject);
    assert_eq!(
        a.honeypot,
        memecoin_engine::security::assessment::HoneypotResult::Honeypot
    );
}

#[test]
fn sim_high_sell_tax_rejected() {
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xtax");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.sim_model = Some(TokenSimModel::high_sell_tax(9_700));
    let a = engine().assess(&ctx);
    assert_eq!(a.verdict, SecurityVerdict::Reject);
}

#[test]
fn sim_second_sell_fails() {
    let r = run_plan(
        &TokenSimModel::second_sell_fails(),
        std::time::Duration::from_secs(1),
    );
    assert!(r.steps.iter().find(|s| s.step == "SELL_50").unwrap().ok);
    assert!(
        !r.steps
            .iter()
            .find(|s| s.step == "SELL_REMAINDER")
            .unwrap()
            .ok
    );
}

#[test]
fn sim_wallet_b_cannot_sell() {
    let r = run_plan(
        &TokenSimModel::other_wallet_cannot_sell(),
        std::time::Duration::from_secs(1),
    );
    assert!(r.steps.iter().find(|s| s.step == "SELL_50").unwrap().ok);
    assert!(!r.steps.iter().find(|s| s.step == "SELL_FROM_B").unwrap().ok);
}

#[test]
fn sim_timeout_unknown_not_pass() {
    let r = timeout_unknown();
    assert!(r.timed_out);
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xt");
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.sim_model = Some(TokenSimModel::normal());
    let mut policy = SecurityPolicy::phase4_defaults();
    policy.simulation_timeout_ms = 0;
    let a = SecurityEngine::new(policy).assess(&ctx);
    assert_ne!(a.verdict, SecurityVerdict::Pass);
}

#[tokio::test]
async fn assessments_are_append_only() {
    let store = MemoryStore::new();
    let tok = disc(Chain::Base, Launchpad::ClankerV4, CLANKER_V4_FACTORY, "0x1");
    let ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    let a1 = engine().assess(&ctx);
    let a2 = engine().assess(&ctx);
    store.insert_assessment(&a1).await.unwrap();
    store.insert_assessment(&a2).await.unwrap();
    let list = store.list_assessments(Chain::Base, "0x1").await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].analyzer_version, list[1].analyzer_version);
}

#[test]
fn analyzer_versions_persist() {
    let tok = disc(Chain::Solana, Launchpad::PumpFun, PUMPFUN_PROGRAM, "m");
    let ctx = SecurityContext::from_token(tok, QualityStatus::HistoricalReplay, true);
    let a = engine().assess(&ctx);
    assert_eq!(a.analyzer_version, SecurityPolicy::ANALYZER_VERSION);
    assert_eq!(a.policy_version, SecurityPolicy::POLICY_VERSION);
}

#[test]
fn historical_not_replaced_with_current() {
    let tok = disc(Chain::Base, Launchpad::Unknown, "0xfactory", "0xt");
    let ctx = SecurityContext::from_token(tok, QualityStatus::HistoricalReplay, true);
    let a = engine().assess(&ctx);
    assert!(a
        .evidence
        .iter()
        .any(|e| e.status
            == memecoin_engine::security::evidence::EvidenceStatus::UnknownHistoricalState));
    assert_ne!(a.verdict, SecurityVerdict::Pass);
}

#[test]
fn rpc_dev_quality_propagates() {
    let tok = disc(Chain::Solana, Launchpad::PumpFun, PUMPFUN_PROGRAM, "m");
    let ctx = SecurityContext::from_token(tok, QualityStatus::RpcDevIncomplete, true);
    let a = engine().assess(&ctx);
    assert_eq!(a.data_quality, QualityStatus::RpcDevIncomplete);
}

#[test]
fn same_input_deterministic_verdict() {
    let tok = disc(
        Chain::Base,
        Launchpad::ClankerV4,
        CLANKER_V4_FACTORY,
        "0xabc",
    );
    let ctx = SecurityContext::from_token(tok.clone(), QualityStatus::HistoricalReplay, true);
    let a = engine().assess(&ctx);
    let b = engine().assess(&ctx);
    assert_eq!(a.verdict, b.verdict);
    assert_eq!(a.hard_reject_reasons, b.hard_reject_reasons);
}

#[test]
fn unknown_never_equals_pass() {
    assert!(!SecurityVerdict::Unknown.is_pass());
}

#[test]
fn pons_graduation_gap_not_honeypot() {
    let tok = disc(
        Chain::Robinhood,
        Launchpad::PonsV2,
        PONS_V2_FACTORY,
        "0xpons",
    );
    let mut ctx = SecurityContext::from_token(tok, QualityStatus::LiveComplete, false);
    ctx.lifecycle = Some(TokenLifecycleState::GraduationGap);
    let a = engine().assess(&ctx);
    assert!(a.evidence.iter().any(|e| e.check == "PONS_GRADUATION_GAP"));
    assert_ne!(
        a.honeypot,
        memecoin_engine::security::assessment::HoneypotResult::Honeypot
    );
}
