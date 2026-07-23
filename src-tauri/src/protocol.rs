use std::collections::{BTreeMap, HashSet};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_PAYLOAD_BYTES: usize = 256_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskPayload {
    pub questions: Vec<Question>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AnswerValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnswerPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answers: Option<BTreeMap<String, AnswerValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Answered,
    Canceled,
}

impl RequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Answered => "answered",
            Self::Canceled => "canceled",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "answered" => Ok(Self::Answered),
            "canceled" => Ok(Self::Canceled),
            _ => bail!("unknown request status: {value}"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredRequest {
    pub request_id: String,
    pub sequence: i64,
    pub status: RequestStatus,
    pub payload: AskPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<AnswerPayload>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QueueSummary {
    pub pending: u64,
    pub active_request_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Ask {
        version: u16,
        request_id: String,
        payload: AskPayload,
    },
    Wait {
        version: u16,
        request_id: String,
    },
    Status {
        version: u16,
        request_id: String,
    },
    Cancel {
        version: u16,
        request_id: String,
    },
}

impl ClientMessage {
    pub fn validate_version(&self) -> Result<()> {
        let version = match self {
            Self::Ask { version, .. }
            | Self::Wait { version, .. }
            | Self::Status { version, .. }
            | Self::Cancel { version, .. } => *version,
        };
        if version != PROTOCOL_VERSION {
            bail!("unsupported protocol version {version}");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Ack {
        version: u16,
        request_id: String,
        status: RequestStatus,
    },
    Result {
        version: u16,
        request_id: String,
        status: RequestStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<AnswerPayload>,
    },
    Status {
        version: u16,
        request: Option<StoredRequest>,
    },
    HostShutdown {
        version: u16,
        request_id: String,
    },
    Error {
        version: u16,
        code: String,
        message: String,
    },
}

impl AskPayload {
    pub fn validate(&self) -> Result<()> {
        let encoded = serde_json::to_vec(self).context("failed to encode AUQ payload")?;
        if encoded.len() > MAX_PAYLOAD_BYTES {
            bail!("AUQ payload exceeds 256 KB");
        }
        if !(1..=4).contains(&self.questions.len()) {
            bail!("questions must contain between 1 and 4 items");
        }

        let mut question_texts = HashSet::new();
        for question in &self.questions {
            if question.question.trim().is_empty() {
                bail!("question text cannot be empty");
            }
            if !question_texts.insert(question.question.trim()) {
                bail!("question text must be unique");
            }
            if question.header.trim().is_empty() || question.header.chars().count() > 12 {
                bail!("question header must contain between 1 and 12 characters");
            }
            if !(2..=4).contains(&question.options.len()) {
                bail!("each question must contain between 2 and 4 options");
            }

            let mut labels = HashSet::new();
            for option in &question.options {
                if option.label.trim().is_empty() || option.description.trim().is_empty() {
                    bail!("option labels and descriptions cannot be empty");
                }
                if !labels.insert(option.label.trim()) {
                    bail!("option labels must be unique within a question");
                }
            }
        }
        Ok(())
    }

    pub fn hash(&self) -> Result<String> {
        let encoded = serde_json::to_vec(self).context("failed to encode AUQ payload")?;
        Ok(hex::encode(Sha256::digest(encoded)))
    }

    pub fn validate_answer(&self, result: &AnswerPayload) -> Result<()> {
        let has_response = result
            .response
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty());
        let has_answers = result
            .answers
            .as_ref()
            .is_some_and(|answers| !answers.is_empty());

        if has_response == has_answers {
            bail!("provide either answers or a general response");
        }
        if has_response {
            return Ok(());
        }

        let answers = result.answers.as_ref().context("answers are required")?;
        for question in &self.questions {
            let answer = answers
                .get(&question.question)
                .with_context(|| format!("missing answer for {}", question.question))?;
            match answer {
                AnswerValue::Single(value) if value.trim().is_empty() => {
                    bail!("answers cannot be empty")
                }
                AnswerValue::Multiple(values) => {
                    if !question.multi_select {
                        bail!("multiple answers are not allowed for {}", question.question);
                    }
                    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
                        bail!("multi-select answers cannot be empty");
                    }
                }
                AnswerValue::Single(_) => {}
            }
        }
        if answers.len() != self.questions.len() {
            bail!("answers contain unknown questions");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> AskPayload {
        AskPayload {
            questions: vec![Question {
                question: "Which runtime?".into(),
                header: "Runtime".into(),
                options: vec![
                    QuestionOption {
                        label: "Tauri".into(),
                        description: "Native shell".into(),
                        preview: None,
                    },
                    QuestionOption {
                        label: "Web".into(),
                        description: "Browser only".into(),
                        preview: None,
                    },
                ],
                multi_select: false,
            }],
        }
    }

    #[test]
    fn validates_a_normal_payload() {
        payload().validate().unwrap();
    }

    #[test]
    fn rejects_duplicate_question_text_but_allows_duplicate_headers() {
        let mut value = payload();
        value.questions.push(value.questions[0].clone());
        assert!(value.validate().unwrap_err().to_string().contains("unique"));

        value.questions[1].question = "Which UI?".into();
        value.validate().unwrap();
    }

    #[test]
    fn validates_general_and_structured_answers() {
        let value = payload();
        value
            .validate_answer(&AnswerPayload {
                answers: Some(BTreeMap::from([(
                    "Which runtime?".into(),
                    AnswerValue::Single("Tauri".into()),
                )])),
                response: None,
            })
            .unwrap();
        value
            .validate_answer(&AnswerPayload {
                answers: None,
                response: Some("Use your best judgment".into()),
            })
            .unwrap();
    }
}
