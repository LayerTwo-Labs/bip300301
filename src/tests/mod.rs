use crate::client;

use jsonrpsee::{
    core::RpcResult,
    types::{response, Response},
};

#[test]

// Test deserializing a result from `getblockcommitments`
fn test_deserialize_getblockcommitments() {
    let json_str = include_str!("json/getblockcommitments.json");
    let mut json_des = serde_json::Deserializer::from_str(json_str);
    let res: Response<client::BlockCommitments> = serde_path_to_error::deserialize(&mut json_des)
        .expect("Failed to deserialize block header");
    let res: RpcResult<response::Success<_>> = res.try_into();
    assert!(res.is_ok())
}

// Test deserializing a result from `getblockheader`
#[test]
fn test_deserialize_getblockheader() {
    let json_str = include_str!("json/getblockheader.json");
    let mut json_des = serde_json::Deserializer::from_str(json_str);
    let res: Response<client::Header> = serde_path_to_error::deserialize(&mut json_des)
        .expect("Failed to deserialize block header");
    let res: RpcResult<response::Success<_>> = res.try_into();
    assert!(res.is_ok())
}

// Test deserializing a genesis block result from `getblockheader`.
// The genesis block header will have no `previousblockhash`.
#[test]
fn test_deserialize_getblockheader_genesis() {
    let json_str = include_str!("json/getblockheader-genesis.json");
    let mut json_des = serde_json::Deserializer::from_str(json_str);
    let res: Response<client::Header> = serde_path_to_error::deserialize(&mut json_des)
        .expect("Failed to deserialize block header");
    let res: RpcResult<response::Success<_>> = res.try_into();
    assert!(res.is_ok())
}

// Test deserializing a result from `getblocktemplate`, and check that
// the serialization roundtrips to the same data
#[test]
fn test_deserialize_serialize_getblocktemplate() {
    let json_str = include_str!("json/getblocktemplate.json");
    let mut json_des = serde_json::Deserializer::from_str(json_str);
    let res: Response<client::BlockTemplate> = serde_path_to_error::deserialize(&mut json_des)
        .expect("Failed to deserialize block template");
    let res: RpcResult<response::Success<_>> = res.try_into();
    let block_template = res.expect("Expected to deserialize as ok success").result;
    let reserialized =
        serde_json::to_string(&block_template).expect("Failed to serialize block template");
    let mut json_des = serde_json::Deserializer::from_str(&reserialized);
    let block_template_2: client::BlockTemplate = serde_path_to_error::deserialize(&mut json_des)
        .expect("Failed to deserialize reserialized block template");
    assert_eq!(block_template, block_template_2);
}
