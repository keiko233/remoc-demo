use remoc::prelude::*;
use std::time::Duration;
/// TCP port the server is listening on.
pub const PIPE_NAME: &str = r"\\.\pipe\counter_pipe";

/// Increasing the counter failed.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum IncreaseError {
    /// An overflow would occur.
    Overflow {
        /// The current value of the counter.
        current_value: u32,
    },
    /// The RTC call failed.
    Call(rtc::CallError),
}

impl From<rtc::CallError> for IncreaseError {
    fn from(err: rtc::CallError) -> Self {
        Self::Call(err)
    }
}

/// generate client and server code
#[rtc::remote]
pub trait Counter {
    /// Obtain the current value of the counter.
    async fn value(&self) -> Result<u32, rtc::CallError>;

    /// Watch the current value of the counter for immediate notification
    /// when it changes.
    async fn watch(&mut self) -> Result<rch::watch::Receiver<u32>, rtc::CallError>;

    /// Increase the counter's value by the provided number.
    async fn increase(&mut self, by: u32) -> Result<(), IncreaseError>;

    /// Counts to the current value of the counter with the specified
    /// delay between each step.
    async fn count_to_value(
        &self,
        step: u32,
        delay: Duration,
    ) -> Result<rch::mpsc::Receiver<u32>, rtc::CallError>;
}
