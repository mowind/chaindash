use serde::{
    ser::SerializeStruct,
    Deserialize,
    Serialize,
    Serializer,
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NodeDetailRespose {
    #[serde(rename = "errMsg")]
    pub err_msg: String,
    pub code: i32,
    pub data: Option<NodeDetail>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NodeDetail {
    #[serde(rename = "nodeName")]
    pub node_name: String,
    #[serde(rename = "totalValue")]
    pub total_value: String,
    #[serde(rename = "delegateValue")]
    pub delegate_value: String,
    #[serde(rename = "delegateQty")]
    pub delegate_qty: i64,
    #[serde(rename = "blockQty")]
    pub block_qty: i64,
    #[serde(rename = "expectBlockQty")]
    pub expect_block_qty: i64,
    #[serde(rename = "genBlocksRate")]
    pub gen_blocks_rate: String,
    #[serde(rename = "rewardPer")]
    pub reward_per: String,
    #[serde(rename = "rewardValue")]
    pub reward_value: String,
    #[serde(rename = "denefitAddr")]
    pub denefit_addr: String,
    #[serde(rename = "verifierTime")]
    pub verifier_time: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NodeListResponse {
    #[serde(rename = "errMsg")]
    pub err_msg: String,
    pub code: i32,
    pub data: Option<Vec<NodeInfo>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NodeInfo {
    #[serde(rename = "nodeId")]
    pub node_id: String,
    pub ranking: i64,
}
