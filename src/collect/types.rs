use serde::{
    Deserialize,
    Deserializer,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum StringLike {
    String(String),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
}

fn deserialize_string_or_default<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<StringLike>::deserialize(deserializer)?;
    Ok(match value {
        Some(StringLike::String(value)) => value,
        Some(StringLike::Integer(value)) => value.to_string(),
        Some(StringLike::Unsigned(value)) => value.to_string(),
        Some(StringLike::Float(value)) => value.to_string(),
        Some(StringLike::Bool(value)) => value.to_string(),
        None => String::new(),
    })
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum I64Like {
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    String(String),
}

fn deserialize_i64_or_default<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<I64Like>::deserialize(deserializer)?;
    Ok(match value {
        Some(I64Like::Integer(value)) => value,
        Some(I64Like::Unsigned(value)) => i64::try_from(value).unwrap_or(i64::MAX),
        Some(I64Like::Float(value)) => value as i64,
        Some(I64Like::String(value)) => value.parse::<i64>().ok().unwrap_or(0),
        None => 0,
    })
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NodeDetailResponse {
    #[serde(rename = "errMsg")]
    pub err_msg: String,
    pub code: i32,
    pub data: Option<NodeDetail>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct NodeDetail {
    #[serde(
        rename = "nodeName",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub node_name: String,
    #[serde(
        rename = "totalValue",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub total_value: String,
    #[serde(
        rename = "delegateValue",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub delegate_value: String,
    #[serde(
        rename = "delegateQty",
        default,
        deserialize_with = "deserialize_i64_or_default"
    )]
    pub delegate_qty: i64,
    #[serde(
        rename = "blockQty",
        default,
        deserialize_with = "deserialize_i64_or_default"
    )]
    pub block_qty: i64,
    #[serde(
        rename = "expectBlockQty",
        default,
        deserialize_with = "deserialize_i64_or_default"
    )]
    pub expect_block_qty: i64,
    #[serde(
        rename = "genBlocksRate",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub gen_blocks_rate: String,
    #[serde(
        rename = "rewardPer",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub reward_per: String,
    #[serde(
        rename = "rewardValue",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub reward_value: String,
    #[serde(
        rename = "denefitAddr",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub benefit_addr: String,
    #[serde(
        rename = "verifierTime",
        default,
        deserialize_with = "deserialize_i64_or_default"
    )]
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
    #[serde(
        rename = "nodeId",
        default,
        deserialize_with = "deserialize_string_or_default"
    )]
    pub node_id: String,
    #[serde(default, deserialize_with = "deserialize_i64_or_default")]
    pub ranking: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_detail_response_allows_null_and_numeric_fields() {
        let body = r#"
        {
            "errMsg": "",
            "code": 0,
            "data": {
                "nodeName": null,
                "totalValue": null,
                "delegateValue": "12",
                "delegateQty": null,
                "blockQty": "34",
                "expectBlockQty": 56,
                "genBlocksRate": null,
                "rewardPer": null,
                "rewardValue": 78.5,
                "denefitAddr": null,
                "verifierTime": null
            }
        }
        "#;

        let parsed: NodeDetailResponse = serde_json::from_str(body).expect("response should parse");
        let detail = parsed.data.expect("detail should exist");

        assert_eq!(detail.node_name, "");
        assert_eq!(detail.total_value, "");
        assert_eq!(detail.delegate_value, "12");
        assert_eq!(detail.delegate_qty, 0);
        assert_eq!(detail.block_qty, 34);
        assert_eq!(detail.expect_block_qty, 56);
        assert_eq!(detail.gen_blocks_rate, "");
        assert_eq!(detail.reward_per, "");
        assert_eq!(detail.reward_value, "78.5");
        assert_eq!(detail.benefit_addr, "");
        assert_eq!(detail.verifier_time, 0);
    }

    #[test]
    fn test_node_list_response_allows_nullable_fields() {
        let body = r#"
        {
            "errMsg": "",
            "code": 0,
            "data": [
                { "nodeId": null, "ranking": null },
                { "nodeId": "node-a", "ranking": "7" }
            ]
        }
        "#;

        let parsed: NodeListResponse = serde_json::from_str(body).expect("response should parse");
        let data = parsed.data.expect("list should exist");

        assert_eq!(data[0].node_id, "");
        assert_eq!(data[0].ranking, 0);
        assert_eq!(data[1].node_id, "node-a");
        assert_eq!(data[1].ranking, 7);
    }
}
