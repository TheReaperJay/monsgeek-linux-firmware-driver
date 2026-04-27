use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use monsgeek_driver::bridge_transport::BridgeTransport;
use monsgeek_protocol::ChecksumType;
use monsgeek_transport::TransportError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentCommand {
    pub cmd: u8,
    pub payload: Vec<u8>,
    pub checksum: ChecksumType,
}

#[derive(Clone, Default)]
pub struct MockTransport {
    sent: Arc<Mutex<Vec<SentCommand>>>,
    reads: Arc<Mutex<VecDeque<[u8; 64]>>>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_responses(responses: Vec<[u8; 64]>) -> Self {
        Self {
            sent: Arc::new(Mutex::new(Vec::new())),
            reads: Arc::new(Mutex::new(VecDeque::from(responses))),
        }
    }

    pub fn sent_commands(&self) -> Vec<SentCommand> {
        self.sent.lock().expect("sent poisoned").clone()
    }
}

impl BridgeTransport for MockTransport {
    fn send_fire_and_forget(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<(), TransportError> {
        self.sent.lock().expect("sent poisoned").push(SentCommand {
            cmd,
            payload: data.to_vec(),
            checksum,
        });
        Ok(())
    }

    fn read_feature_report(&self) -> Result<[u8; 64], TransportError> {
        self.reads
            .lock()
            .expect("reads poisoned")
            .pop_front()
            .ok_or_else(|| TransportError::Usb("mock read queue empty".to_string()))
    }

    fn query_command(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        self.sent.lock().expect("sent poisoned").push(SentCommand {
            cmd,
            payload: data.to_vec(),
            checksum,
        });
        self.read_feature_report()
    }

    fn query_raw(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        self.sent.lock().expect("sent poisoned").push(SentCommand {
            cmd,
            payload: data.to_vec(),
            checksum,
        });
        self.read_feature_report()
    }
}
