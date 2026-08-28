use alloy_primitives::keccak256;

/// 4-byte selectors are evidence, never a reject by themselves (collisions exist).
pub fn selector(sig: &str) -> [u8; 4] {
    let h = keccak256(sig.as_bytes());
    [h[0], h[1], h[2], h[3]]
}

pub fn extract_push4(bytecode: &[u8]) -> Vec<[u8; 4]> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 < bytecode.len() {
        if bytecode[i] == 0x63 {
            let sel = [
                bytecode[i + 1],
                bytecode[i + 2],
                bytecode[i + 3],
                bytecode[i + 4],
            ];
            if !out.contains(&sel) {
                out.push(sel);
            }
            i += 5;
        } else {
            i += 1;
        }
    }
    out
}

pub fn hex4(s: [u8; 4]) -> String {
    format!("0x{}", hex::encode(s))
}

pub struct LabeledSelector {
    pub sig: &'static str,
    pub kind: &'static str,
}

pub fn security_selectors() -> &'static [LabeledSelector] {
    &[
        LabeledSelector {
            sig: "owner()",
            kind: "owner",
        },
        LabeledSelector {
            sig: "transferOwnership(address)",
            kind: "owner",
        },
        LabeledSelector {
            sig: "renounceOwnership()",
            kind: "owner",
        },
        LabeledSelector {
            sig: "mint(address,uint256)",
            kind: "mint",
        },
        LabeledSelector {
            sig: "burn(uint256)",
            kind: "burn",
        },
        LabeledSelector {
            sig: "pause()",
            kind: "pause",
        },
        LabeledSelector {
            sig: "unpause()",
            kind: "pause",
        },
        LabeledSelector {
            sig: "blacklist(address)",
            kind: "blacklist",
        },
        LabeledSelector {
            sig: "setBlacklist(address,bool)",
            kind: "blacklist",
        },
        LabeledSelector {
            sig: "enableTrading()",
            kind: "trading",
        },
        LabeledSelector {
            sig: "openTrading()",
            kind: "trading",
        },
        LabeledSelector {
            sig: "setTax(uint256)",
            kind: "tax",
        },
        LabeledSelector {
            sig: "setFees(uint256,uint256)",
            kind: "tax",
        },
        LabeledSelector {
            sig: "setBuyTax(uint256)",
            kind: "tax",
        },
        LabeledSelector {
            sig: "setSellTax(uint256)",
            kind: "tax",
        },
        LabeledSelector {
            sig: "setMaxTx(uint256)",
            kind: "max_tx",
        },
        LabeledSelector {
            sig: "setMaxWallet(uint256)",
            kind: "max_wallet",
        },
        LabeledSelector {
            sig: "excludeFromFees(address,bool)",
            kind: "fees",
        },
        LabeledSelector {
            sig: "setRouter(address)",
            kind: "router",
        },
        LabeledSelector {
            sig: "setPair(address)",
            kind: "pair",
        },
        LabeledSelector {
            sig: "upgradeTo(address)",
            kind: "upgrade",
        },
        LabeledSelector {
            sig: "upgradeToAndCall(address,bytes)",
            kind: "upgrade",
        },
        LabeledSelector {
            sig: "grantRole(bytes32,address)",
            kind: "role",
        },
        LabeledSelector {
            sig: "revokeRole(bytes32,address)",
            kind: "role",
        },
        LabeledSelector {
            sig: "hasRole(bytes32,address)",
            kind: "role",
        },
    ]
}

pub fn labeled_present(bytecode: &[u8]) -> Vec<(&'static str, &'static str, String)> {
    let sels = extract_push4(bytecode);
    let mut hit = Vec::new();
    for lab in security_selectors() {
        let want = selector(lab.sig);
        if sels.contains(&want) {
            hit.push((lab.sig, lab.kind, hex4(want)));
        }
    }
    hit
}
