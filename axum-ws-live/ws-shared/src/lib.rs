use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Msg {
    pub room: String,
    pub username: String,
    pub timestamp: u64,
    pub data: MsgData,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MsgData {
    Join,
    Leave,
    Message(String),
}

impl TryFrom<&str> for Msg {
    type Error = serde_json::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

impl TryFrom<&Msg> for String {
    type Error = serde_json::Error;

    fn try_from(value: &Msg) -> Result<Self, Self::Error> {
        serde_json::to_string(value)
    }
}

impl Msg {
    pub fn new(room: String, username: String, data: MsgData) -> Self {
        Self {
            room,
            username,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            data,
        }
    }

    pub fn join(room: &str, username: &str) -> Self {
        Self::new(room.to_string(), username.to_string(), MsgData::Join)
    }

    pub fn leave(room: &str, username: &str) -> Self {
        Self::new(room.to_string(), username.to_string(), MsgData::Leave)
    }

    pub fn message(room: &str, username: &str, message: &str) -> Self {
        Self::new(
            room.to_string(),
            username.to_string(),
            MsgData::Message(message.to_string()),
        )
    }
}