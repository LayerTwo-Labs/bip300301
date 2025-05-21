use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use bitcoin::{
    amount::serde::SerdeAmount,
    block,
    hashes::{ripemd160::Hash as Ripemd160Hash, sha256::Hash as Sha256Hash, Hash as _},
    BlockHash, Txid, Weight, Wtxid,
};
use hashlink::LinkedHashMap;
use jsonrpsee::proc_macros::rpc;
use monostate::MustBe;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value as JsonValue;
use serde_with::{serde_as, DeserializeAs, DeserializeFromStr, FromInto, Map, SerializeAs};

#[derive(Debug, Deserialize, Serialize)]
pub struct WithdrawalStatus {
    hash: bitcoin::Txid,
    nblocksleft: usize,
    nworkscore: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SpentWithdrawal {
    pub nsidechain: u8,
    pub hash: bitcoin::Txid,
    pub hashblock: bitcoin::BlockHash,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FailedWithdrawal {
    pub nsidechain: u8,
    pub hash: bitcoin::Txid,
}

#[derive(DeserializeFromStr)]
#[repr(transparent)]
struct CompactTargetRepr(bitcoin::CompactTarget);

impl std::str::FromStr for CompactTargetRepr {
    type Err = bitcoin::error::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use bitcoin::string::FromHexStr;
        bitcoin::CompactTarget::from_hex_str_no_prefix(s).map(Self)
    }
}

impl Serialize for CompactTargetRepr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        hex::serde::serialize(self.0.to_consensus().to_be_bytes(), serializer)
    }
}

impl From<CompactTargetRepr> for bitcoin::CompactTarget {
    fn from(repr: CompactTargetRepr) -> Self {
        repr.0
    }
}

impl From<bitcoin::CompactTarget> for CompactTargetRepr {
    fn from(target: bitcoin::CompactTarget) -> Self {
        Self(target)
    }
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Header {
    pub hash: BlockHash,
    pub height: u32,
    pub version: bitcoin::block::Version,
    #[serde(rename = "previousblockhash", default = "BlockHash::all_zeros")]
    pub prev_blockhash: BlockHash,
    #[serde(rename = "merkleroot")]
    pub merkle_root: bitcoin::TxMerkleNode,
    pub time: u32,
    #[serde_as(as = "FromInto<CompactTargetRepr>")]
    pub bits: bitcoin::CompactTarget,
    pub nonce: u32,
}

impl Header {
    /// Computes the target (range [0, T] inclusive) that a blockhash must land in to be valid.
    pub fn target(&self) -> bitcoin::Target {
        self.bits.into()
    }

    /// Returns the total work of the block.
    pub fn work(&self) -> bitcoin::Work {
        self.target().to_work()
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct RawMempoolTxFees {
    pub base: u64,
    pub modified: u64,
    pub ancestor: u64,
    pub descendant: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawMempoolTxInfo {
    pub vsize: u64,
    pub weight: u64,
    #[serde(rename = "descendantcount")]
    pub descendant_count: u64,
    #[serde(rename = "descendantsize")]
    pub descendant_size: u64,
    #[serde(rename = "ancestorcount")]
    pub ancestor_count: u64,
    #[serde(rename = "ancestorsize")]
    pub ancestor_size: u64,
    pub wtxid: Wtxid,
    pub fees: RawMempoolTxFees,
    pub depends: Vec<Txid>,
    #[serde(rename = "spentby")]
    pub spent_by: Vec<Txid>,
    #[serde(rename = "bip125replaceable")]
    pub bip125_replaceable: bool,
    pub unbroadcast: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawMempoolWithSequence {
    pub txids: Vec<Txid>,
    pub mempool_sequence: u64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize)]
pub struct RawMempoolVerbose {
    #[serde_as(as = "Map<_, _>")]
    pub entries: Vec<(Txid, RawMempoolTxInfo)>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TxOutSetInfo {
    pub height: u32,
    #[serde(rename = "bestblock")]
    pub best_block: BlockHash,
    #[serde(rename = "transactions")]
    pub n_txs: u64,
    #[serde(rename = "txouts")]
    pub n_txouts: u64,
    #[serde(with = "hex::serde")]
    pub hash_serialized_3: [u8; 32],
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Vote {
    Upvote,
    Abstain,
    Downvote,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub hash: bitcoin::BlockHash,
    pub confirmations: usize,
    pub strippedsize: usize,
    pub size: usize,
    pub weight: usize,
    pub height: usize,
    pub version: i32,
    pub version_hex: String,
    pub merkleroot: bitcoin::hash_types::TxMerkleNode,
    pub tx: Vec<bitcoin::Txid>,
    pub time: u32,
    pub mediantime: u32,
    pub nonce: u32,
    pub bits: String,
    pub difficulty: f64,
    pub chainwork: String,
    pub previousblockhash: Option<bitcoin::BlockHash>,
    pub nextblockhash: Option<bitcoin::BlockHash>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct SidechainId(pub u8);

fn deserialize_reverse_hex<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: hex::FromHex,
    <T as hex::FromHex>::Error: std::fmt::Display,
{
    let mut bytes: Vec<u8> = hex::serde::deserialize(deserializer)?;
    bytes.reverse();
    T::from_hex(hex::encode(bytes)).map_err(<D::Error as serde::de::Error>::custom)
}

/// Array item returned by `getblockcommitments`
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum BlockCommitment {
    #[serde(rename = "BMM h*")]
    BmmHStar {
        #[serde(rename = "h", deserialize_with = "deserialize_reverse_hex")]
        commitment: [u8; 32],
        #[serde(rename = "nsidechain")]
        sidechain_id: SidechainId,
        #[serde(rename = "prevbytes", deserialize_with = "hex::serde::deserialize")]
        prev_bytes: [u8; 4],
    },
    #[serde(rename = "SCDB update bytes")]
    ScdbUpdateBytes {
        // TODO: parse script?
        script: String,
    },
    #[serde(rename = "Sidechain activation ack")]
    SidechainActivationAck {
        #[serde(rename = "hash", deserialize_with = "deserialize_reverse_hex")]
        commitment: [u8; 32],
    },
    #[serde(rename = "Sidechain proposal")]
    SidechainProposal,
    #[serde(rename = "Withdrawal bundle hash")]
    WithdrawalBundleHash {
        #[serde(rename = "hash", deserialize_with = "deserialize_reverse_hex")]
        commitment: [u8; 32],
        #[serde(rename = "nsidechain")]
        sidechain_id: SidechainId,
    },
    #[serde(rename = "Witness commitment")]
    WitnessCommitment {
        // TODO: parse script?
        script: String,
    },
}

#[derive(Clone, Debug)]
pub(super) struct BlockCommitments(pub Vec<(u32, BlockCommitment)>);

impl<'de> Deserialize<'de> for BlockCommitments {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct Repr {
            txout: u32,
            #[serde(flatten)]
            commitment: BlockCommitment,
        }

        impl From<Repr> for (u32, BlockCommitment) {
            fn from(repr: Repr) -> Self {
                (repr.txout, repr.commitment)
            }
        }

        Vec::<FromInto<Repr>>::deserialize_as(deserializer).map(Self)
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BlockTemplateRequest {
    #[allow(clippy::type_complexity)]
    rules: [MustBe!("segwit"); 1],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockTemplateTransaction {
    #[serde(with = "hex::serde")]
    pub data: Vec<u8>,
    pub txid: Txid,
    // TODO: check that this is the wtxid
    pub hash: Wtxid,
    pub depends: Vec<u32>,
    pub fee: i64,
    pub sigops: Option<u64>,
    pub weight: u64,
}

/// Representation used with serde_with
#[derive(Clone, Copy, Debug, Default)]
struct LinkedHashMapRepr<K, V>(PhantomData<(K, V)>);

impl<'de, K0, K1, V0, V1> DeserializeAs<'de, LinkedHashMap<K1, V1>> for LinkedHashMapRepr<K0, V0>
where
    K0: DeserializeAs<'de, K1>,
    K1: Eq + std::hash::Hash,
    V0: DeserializeAs<'de, V1>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<LinkedHashMap<K1, V1>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        <serde_with::Map<K0, V0> as DeserializeAs<'de, Vec<(K1, V1)>>>::deserialize_as(deserializer)
            .map(LinkedHashMap::from_iter)
    }
}

impl<K0, K1, V0, V1> SerializeAs<LinkedHashMap<K1, V1>> for LinkedHashMapRepr<K0, V0>
where
    K0: SerializeAs<K1>,
    V0: SerializeAs<V1>,
{
    fn serialize_as<S>(source: &LinkedHashMap<K1, V1>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <serde_with::Map<&K0, &V0> as SerializeAs<Vec<(&K1, &V1)>>>::serialize_as(
            &Vec::from_iter(source),
            serializer,
        )
    }
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockTemplate {
    pub version: block::Version,
    pub rules: Vec<String>,
    #[serde(rename = "vbavailable")]
    pub version_bits_available: LinkedHashMap<String, JsonValue>,
    #[serde(rename = "vbrequired")]
    pub version_bits_required: block::Version,
    #[serde(rename = "previousblockhash")]
    pub prev_blockhash: bitcoin::BlockHash,
    pub transactions: Vec<BlockTemplateTransaction>,
    #[serde(rename = "coinbaseaux")]
    #[serde_as(as = "LinkedHashMapRepr<_, serde_with::hex::Hex>")]
    pub coinbase_aux: LinkedHashMap<String, Vec<u8>>,
    #[serde(rename = "coinbasevalue")]
    pub coinbase_value: u64,
    /// MUST be omitted if the server does not support long polling
    #[serde(rename = "longpollid")]
    pub long_poll_id: Option<String>,
    #[serde_as(as = "serde_with::hex::Hex")]
    pub target: [u8; 32],
    pub mintime: u64,
    pub mutable: Vec<String>,
    #[serde(rename = "noncerange")]
    #[serde_as(as = "serde_with::hex::Hex")]
    pub nonce_range: [u8; 8],
    #[serde(rename = "sigoplimit")]
    pub sigop_limit: u64,
    #[serde(rename = "sizelimit")]
    pub size_limit: u64,
    #[serde(rename = "weightlimit")]
    pub weight_limit: Weight,
    #[serde(rename = "curtime")]
    pub current_time: u64,
    #[serde(rename = "bits")]
    #[serde_as(as = "FromInto<CompactTargetRepr>")]
    pub compact_target: bitcoin::CompactTarget,
    pub height: u32,
    #[serde_as(as = "Option<serde_with::hex::Hex>")]
    pub default_witness_commitment: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
pub struct BlockchainInfo {
    #[serde(with = "bitcoin::network::as_core_arg")]
    pub chain: bitcoin::Network,
    pub blocks: u32,
    #[serde(rename = "bestblockhash")]
    pub best_blockhash: bitcoin::BlockHash,
    pub difficulty: f64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    pub hashblock: bitcoin::BlockHash,
    pub nburnindex: usize,
    pub ntx: usize,
    pub strdest: String,
    pub txhex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidechainInfo {
    #[serde(rename = "title")]
    pub name: String,
    #[serde(alias = "nversion")]
    pub version: i32,
    pub description: String,
    #[serde(alias = "hashid1", alias = "hashID1")]
    pub hash_id_1: Sha256Hash,
    #[serde(alias = "hashid2", alias = "hashID2")]
    pub hash_id_2: Ripemd160Hash,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidechainProposal {
    #[serde(rename = "nSidechain")]
    pub sidechain_id: SidechainId,
    #[serde(flatten)]
    pub info: SidechainInfo,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SidechainActivationStatus {
    #[serde(rename = "title")]
    pub name: String,
    pub description: String,
    #[serde(alias = "nage")]
    pub age: u32,
    // TODO: this needs a better name
    #[serde(alias = "nfail")]
    pub fail: u32,
}

#[rpc(client)]
pub trait Main {
    #[method(name = "countsidechaindeposits")]
    async fn count_sidechain_deposits(&self, nsidechain: u8)
        -> Result<u32, jsonrpsee::core::Error>;

    #[method(name = "createbmmcriticaldatatx")]
    async fn createbmmcriticaldatatx(
        &self,
        amount: AmountBtc,
        height: u32,
        criticalhash: &bitcoin::BlockHash,
        nsidechain: u8,
        prevbytes: &str,
    ) -> Result<serde_json::Value, jsonrpsee::core::Error>;

    #[method(name = "createsidechaindeposit")]
    async fn createsidechaindeposit(
        &self,
        nsidechain: u8,
        depositaddress: &str,
        amount: AmountBtc,
        fee: AmountBtc,
    ) -> Result<serde_json::Value, jsonrpsee::core::Error>;

    #[method(name = "createsidechainproposal")]
    async fn create_sidechain_proposal(
        &self,
        nsidechain: u8,
        sidechain_name: &str,
        sidechain_description: &str,
    ) -> Result<SidechainProposal, jsonrpsee::core::Error>;

    #[method(name = "generate")]
    async fn generate(&self, num: u32) -> Result<serde_json::Value, jsonrpsee::core::Error>;

    #[method(name = "generatetoaddress")]
    async fn generate_to_address(
        &self,
        n_blocks: u32,
        address: &bitcoin::Address<bitcoin::address::NetworkUnchecked>,
    ) -> Result<Vec<BlockHash>, jsonrpsee::core::Error>;

    #[method(name = "getblockcommitments")]
    async fn get_block_commitments(
        &self,
        blockhash: bitcoin::BlockHash,
    ) -> Result<BlockCommitments, jsonrpsee::core::Error>;

    #[method(name = "getblocktemplate")]
    async fn get_block_template(
        &self,
        block_template_request: BlockTemplateRequest,
    ) -> Result<BlockTemplate, jsonrpsee::core::Error>;

    #[method(name = "getblockchaininfo")]
    async fn get_blockchain_info(&self) -> Result<BlockchainInfo, jsonrpsee::core::Error>;

    #[method(name = "getmempoolentry")]
    async fn get_mempool_entry(
        &self,
        txid: Txid,
    ) -> Result<RawMempoolTxInfo, jsonrpsee::core::Error>;

    #[method(name = "getnetworkinfo")]
    async fn get_network_info(&self) -> jsonrpsee::core::RpcResult<serde_json::Value>;

    #[method(name = "getbestblockhash")]
    async fn getbestblockhash(&self) -> Result<bitcoin::BlockHash, jsonrpsee::core::Error>;

    #[method(name = "getblock")]
    async fn getblock(
        &self,
        blockhash: bitcoin::BlockHash,
        verbosity: Option<usize>,
    ) -> Result<Block, jsonrpsee::core::Error>;

    #[method(name = "getblockcount")]
    async fn getblockcount(&self) -> Result<usize, jsonrpsee::core::Error>;

    #[method(name = "getblockheader")]
    async fn getblockheader(
        &self,
        block_hash: bitcoin::BlockHash,
    ) -> Result<Header, jsonrpsee::core::Error>;

    #[method(name = "getnewaddress")]
    async fn getnewaddress(
        &self,
        account: &str,
        address_type: &str,
    ) -> Result<bitcoin::Address<bitcoin::address::NetworkUnchecked>, jsonrpsee::core::Error>;

    #[method(name = "gettxoutsetinfo")]
    async fn gettxoutsetinfo(&self) -> Result<TxOutSetInfo, jsonrpsee::core::Error>;

    #[method(name = "invalidateblock")]
    async fn invalidate_block(
        &self,
        block_hash: bitcoin::BlockHash,
    ) -> Result<(), jsonrpsee::core::Error>;

    #[method(name = "listactivesidechains")]
    async fn list_active_sidechains(
        &self,
    ) -> Result<Vec<serde_json::Value>, jsonrpsee::core::Error>;

    #[method(name = "listsidechainactivationstatus")]
    async fn list_sidechain_activation_status(
        &self,
    ) -> Result<Vec<SidechainActivationStatus>, jsonrpsee::core::Error>;

    #[method(name = "listsidechainproposals")]
    async fn list_sidechain_proposals(&self) -> Result<Vec<SidechainInfo>, jsonrpsee::core::Error>;

    #[method(name = "listfailedwithdrawals")]
    async fn listfailedwithdrawals(&self) -> Result<Vec<FailedWithdrawal>, jsonrpsee::core::Error>;

    #[method(name = "listsidechaindepositsbyblock")]
    async fn listsidechaindepositsbyblock(
        &self,
        nsidechain: u8,
        end_blockhash: Option<bitcoin::BlockHash>,
        start_blockhash: Option<bitcoin::BlockHash>,
    ) -> Result<Vec<Deposit>, jsonrpsee::core::Error>;

    #[method(name = "listspentwithdrawals")]
    async fn listspentwithdrawals(&self) -> Result<Vec<SpentWithdrawal>, jsonrpsee::core::Error>;

    // FIXME: Define a "Deposit Address" type.
    #[method(name = "listwithdrawalstatus")]
    async fn listwithdrawalstatus(
        &self,
        nsidechain: u8,
    ) -> Result<Vec<WithdrawalStatus>, jsonrpsee::core::Error>;

    #[method(name = "receivewithdrawalbundle")]
    async fn receivewithdrawalbundle(
        &self,
        nsidechain: u8,
        // Raw transaction hex.
        rawtx: &str,
    ) -> Result<serde_json::Value, jsonrpsee::core::Error>;

    #[method(name = "stop")]
    async fn stop(&self) -> Result<String, jsonrpsee::core::Error>;

    #[method(name = "verifybmm")]
    async fn verifybmm(
        &self,
        blockhash: bitcoin::BlockHash,
        criticalhash: bitcoin::BlockHash,
        nsidechain: u8,
    ) -> Result<serde_json::Value, jsonrpsee::core::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoolWitness<const BOOL: bool>;

impl<const BOOL: bool> Serialize for BoolWitness<{ BOOL }> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        BOOL.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BoolWitness<false> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct Repr(monostate::MustBe!(false));
        let _ = Repr::deserialize(deserializer)?;
        Ok(Self)
    }
}

impl<'de> Deserialize<'de> for BoolWitness<true> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct Repr(monostate::MustBe!(true));
        let _ = Repr::deserialize(deserializer)?;
        Ok(Self)
    }
}

pub struct GetRawMempoolParams<Verbose, MempoolSequence>(PhantomData<(Verbose, MempoolSequence)>);

pub trait GetRawMempoolResponse {
    type Response: DeserializeOwned;
}

impl GetRawMempoolResponse for GetRawMempoolParams<BoolWitness<false>, BoolWitness<false>> {
    type Response = Vec<Txid>;
}

impl GetRawMempoolResponse for GetRawMempoolParams<BoolWitness<false>, BoolWitness<true>> {
    type Response = RawMempoolWithSequence;
}

impl GetRawMempoolResponse for GetRawMempoolParams<BoolWitness<true>, BoolWitness<false>> {
    type Response = RawMempoolVerbose;
}

#[rpc(
    client,
    client_bounds(
        Verbose: Serialize + Send + Sync + 'static,
        MempoolSequence: Serialize + Send + Sync + 'static,
        GetRawMempoolParams<Verbose, MempoolSequence>: GetRawMempoolResponse
    )
)]
pub trait GetRawMempool<Verbose, MempoolSequence>
where
    GetRawMempoolParams<Verbose, MempoolSequence>: GetRawMempoolResponse,
{
    #[method(name = "getrawmempool")]
    async fn get_raw_mempool(
        &self,
        verbose: Verbose,
        mempool_sequence: MempoolSequence,
    ) -> Result<
        <GetRawMempoolParams<Verbose, MempoolSequence> as GetRawMempoolResponse>::Response,
        jsonrpsee::core::Error,
    >;
}

pub trait GetRawTransactionVerbosity {
    type Response: DeserializeOwned;
}

#[derive(Debug)]
pub struct GetRawTransactionVerbose<const VERBOSE: bool>;

impl<const VERBOSE: bool> Serialize for GetRawTransactionVerbose<{ VERBOSE }> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        VERBOSE.serialize(serializer)
    }
}

impl GetRawTransactionVerbosity for GetRawTransactionVerbose<false> {
    type Response = String;
}

impl<'de> Deserialize<'de> for GetRawTransactionVerbose<false> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct Repr(monostate::MustBe!(false));
        let _ = Repr::deserialize(deserializer)?;
        Ok(Self)
    }
}

impl GetRawTransactionVerbosity for GetRawTransactionVerbose<true> {
    type Response = serde_json::Value;
}

impl<'de> Deserialize<'de> for GetRawTransactionVerbose<true> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, Deserialize)]
        struct Repr(monostate::MustBe!(true));
        let _ = Repr::deserialize(deserializer)?;
        Ok(Self)
    }
}

#[rpc(client)]
pub trait GetRawTransaction<T>
where
    T: GetRawTransactionVerbosity,
{
    #[method(name = "getrawtransaction")]
    async fn get_raw_transaction(
        &self,
        txid: Txid,
        verbose: T,
        block_hash: Option<bitcoin::BlockHash>,
    ) -> Result<<T as GetRawTransactionVerbosity>::Response, jsonrpsee::core::Error>;
}

// Arguments:
// 1. "amount"         (numeric or string, required) The amount in BTC to be spent.
// 2. "height"         (numeric, required) The block height this transaction must be included in.
// Note: If 0 is passed in for height, current block height will be used
// 3. "criticalhash"   (string, required) h* you want added to a coinbase
// 4. "nsidechain"     (numeric, required) Sidechain requesting BMM
// 5. "prevbytes"      (string, required) a portion of the previous block hash

// FIXME: Make mainchain API machine friendly. Parsing human readable amounts
// here is stupid -- just take and return values in satoshi.
#[derive(Clone, Copy)]
pub struct AmountBtc(pub bitcoin::Amount);

impl From<bitcoin::Amount> for AmountBtc {
    fn from(other: bitcoin::Amount) -> AmountBtc {
        AmountBtc(other)
    }
}

impl From<AmountBtc> for bitcoin::Amount {
    fn from(other: AmountBtc) -> bitcoin::Amount {
        other.0
    }
}

impl Deref for AmountBtc {
    type Target = bitcoin::Amount;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AmountBtc {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'de> Deserialize<'de> for AmountBtc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(AmountBtc(bitcoin::Amount::des_btc(deserializer)?))
    }
}

impl Serialize for AmountBtc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.ser_btc(serializer)
    }
}
