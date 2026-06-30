use rmcp::model::{CallToolResult, ContentBlock, JsonObject, Meta};
use serde::{Deserialize, Serialize};

pub const RESULT_STATUS_META_KEY: &str = "symforge/result_status";
pub const RESULT_STATUS_CONTRACT_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeClass {
    Found,
    NotFound,
    Ambiguous,
    InvalidRequest,
    EmptyResult,
    InternalFailure,
}

impl OutcomeClass {
    pub const ALL: [Self; 6] = [
        Self::Found,
        Self::NotFound,
        Self::Ambiguous,
        Self::InvalidRequest,
        Self::EmptyResult,
        Self::InternalFailure,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Found => "found",
            Self::NotFound => "not_found",
            Self::Ambiguous => "ambiguous",
            Self::InvalidRequest => "invalid_request",
            Self::EmptyResult => "empty_result",
            Self::InternalFailure => "internal_failure",
        }
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::InvalidRequest | Self::InternalFailure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultStatus {
    pub contract_version: u8,
    pub outcome_class: OutcomeClass,
}

impl ResultStatus {
    pub const fn new(outcome_class: OutcomeClass) -> Self {
        Self {
            contract_version: RESULT_STATUS_CONTRACT_VERSION,
            outcome_class,
        }
    }

    pub fn into_call_tool_result(self, human_text: impl Into<String>) -> CallToolResult {
        let mut meta = JsonObject::new();
        meta.insert(
            RESULT_STATUS_META_KEY.to_string(),
            serde_json::to_value(self).expect("ResultStatus must serialize to JSON"),
        );

        let content = vec![ContentBlock::text(human_text.into())];
        let result = if self.outcome_class.is_error() {
            CallToolResult::error(content)
        } else {
            CallToolResult::success(content)
        };
        result.with_meta(Some(Meta(meta)))
    }
}
