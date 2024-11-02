#![allow(unused)]

use ckb_types::{core::EpochNumberWithFraction, h256, H256};
pub const PREFIX_MAINNET: &str = "ckb";
pub const PREFIX_TESTNET: &str = "ckt";

pub const NETWORK_MAINNET: &str = "ckb";
pub const NETWORK_TESTNET: &str = "ckb_testnet";
pub const NETWORK_STAGING: &str = "ckb_staging";
pub const NETWORK_DEV: &str = "ckb_dev";

pub const SECP_SIGNATURE_SIZE: usize = 65;

// Since relative mask
pub const LOCK_TYPE_FLAG: u64 = 1 << 63;
pub const METRIC_TYPE_FLAG_MASK: u64 = 0x6000_0000_0000_0000;
pub const VALUE_MASK: u64 = 0x00ff_ffff_ffff_ffff;
pub const REMAIN_FLAGS_BITS: u64 = 0x1f00_0000_0000_0000;

// Special cells in genesis transactions: (transaction-index, output-index)
pub const SIGHASH_OUTPUT_LOC: (usize, usize) = (0, 1);
pub const MULTISIG_OUTPUT_LOC: (usize, usize) = (0, 4);
pub const DAO_OUTPUT_LOC: (usize, usize) = (0, 2);
pub const SIGHASH_GROUP_OUTPUT_LOC: (usize, usize) = (1, 0);
pub const MULTISIG_GROUP_OUTPUT_LOC: (usize, usize) = (1, 1);

pub const ONE_CKB: u64 = 100_000_000;
pub const MIN_SECP_CELL_CAPACITY: u64 = 61 * ONE_CKB;
// mainnet,testnet cellbase maturity
pub const CELLBASE_MATURITY: EpochNumberWithFraction =
    EpochNumberWithFraction::new_unchecked(4, 0, 1);

/// "TYPE_ID" in hex (copied from ckb-chain-spec)
pub const TYPE_ID_CODE_HASH: H256 = h256!("0x545950455f4944");

pub const SIGHASH_TYPE_HASH: H256 =
    h256!("0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8");
pub const MULTISIG_TYPE_HASH: H256 =
    h256!("0x5c5069eb0857efc65e1bca0c07df34c31663b3622fd3876c876320fc9634e2a8");
pub const DAO_TYPE_HASH: H256 =
    h256!("0x82d76d1b75fe2fd9a27dfbaa65a039221a380d76c926f378d3f81cf3e7e13f2e");

/// anyone can pay script mainnet code hash, see:
/// <https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0026-anyone-can-pay/0026-anyone-can-pay.md#notes>
pub const ACP_TYPE_HASH_LINA: H256 =
    h256!("0xd369597ff47f29fbc0d47d2e3775370d1250b85140c670e4718af712983a2354");
/// anyone can pay script testnet code hash
pub const ACP_TYPE_HASH_AGGRON: H256 =
    h256!("0x3419a1c09eb2567f6552ee7a8ecffd64155cffe0f1796e6e61ec088d740c1356");

/// cheque withdraw since value
pub const CHEQUE_CELL_SINCE: u64 = 0xA000000000000006;

pub const SPORE_CODE_HASH_LINA: H256 =
    h256!("0x4a4dce1df3dffff7f8b2cd7dff7303df3b6150c9788cb75dcf6747247132b9f5");
pub const SPORE_CODE_HASH_AGGRON: [H256; 3] = [
    h256!("0x685a60219309029d01310311dba953d67029170ca4848a4ff638e57002130a0d"),
    h256!("0x5e063b4c0e7abeaa6a428df3b693521a3050934cf3b0ae97a800d1bc31449398"),
    h256!("0xbbad126377d45f90a8ee120da988a2d7332c78ba8fd679aab478a19d6c133494"),
];

pub const DELEGATE_LOCK_CODE_HASH: H256 =
    h256!("0x6361d4b20d845953d9c9431bbba08905573005a71e2a2432e7e0e7c685666f24");
