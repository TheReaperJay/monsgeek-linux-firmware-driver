#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    Preflight,
    EnterBootloader,
    WaitBootloader,
    TransferStart,
    TransferChunks,
    TransferComplete,
    PostVerify,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressEvent {
    pub phase: ProgressPhase,
    pub progress: f32,
    pub message: Option<String>,
}

impl ProgressEvent {
    pub fn new(phase: ProgressPhase, progress: f32) -> Self {
        Self {
            phase,
            progress: progress.clamp(0.0, 1.0),
            message: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}
