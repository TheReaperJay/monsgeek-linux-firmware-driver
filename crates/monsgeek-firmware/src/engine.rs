use anyhow::Result;

use crate::progress::{ProgressEvent, ProgressPhase};

pub const CHUNK_SIZE: usize = 64;
pub const TRANSFER_START_MARKER: [u8; 2] = [0xBA, 0xC0];
pub const TRANSFER_COMPLETE_MARKER: [u8; 2] = [0xBA, 0xC2];

pub trait FirmwareIo {
    fn enter_bootloader(&mut self) -> Result<()>;
    fn wait_for_bootloader(&mut self) -> Result<()>;
    fn send_marker(&mut self, marker: [u8; 2]) -> Result<()>;
    fn send_chunk(&mut self, chunk_index: usize, chunk: &[u8]) -> Result<()>;
    fn post_verify(&mut self) -> Result<()>;
}

pub trait FirmwareEngine {
    fn execute(
        &mut self,
        image: &[u8],
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<FirmwareExecutionResult>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareExecutionResult {
    pub bytes_sent: usize,
    pub chunk_count: usize,
    pub checksum_24: u32,
}

pub struct DefaultFirmwareEngine<T: FirmwareIo> {
    io: T,
}

impl<T: FirmwareIo> DefaultFirmwareEngine<T> {
    pub fn new(io: T) -> Self {
        Self { io }
    }
}

impl<T: FirmwareIo> FirmwareEngine for DefaultFirmwareEngine<T> {
    fn execute(
        &mut self,
        image: &[u8],
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<FirmwareExecutionResult> {
        progress(ProgressEvent::new(ProgressPhase::EnterBootloader, 0.10));
        self.io.enter_bootloader()?;

        progress(ProgressEvent::new(ProgressPhase::WaitBootloader, 0.20));
        self.io.wait_for_bootloader()?;

        progress(ProgressEvent::new(ProgressPhase::TransferStart, 0.30));
        self.io.send_marker(TRANSFER_START_MARKER)?;

        let chunk_count = image.len().div_ceil(CHUNK_SIZE);
        if chunk_count == 0 {
            progress(
                ProgressEvent::new(ProgressPhase::Failed, 1.0).with_message("empty firmware image"),
            );
            anyhow::bail!("firmware image is empty");
        }

        for (chunk_index, chunk) in image.chunks(CHUNK_SIZE).enumerate() {
            let ratio = (chunk_index + 1) as f32 / chunk_count as f32;
            let progress_value = 0.30 + (0.55 * ratio);
            progress(ProgressEvent::new(
                ProgressPhase::TransferChunks,
                progress_value,
            ));
            self.io.send_chunk(chunk_index, chunk)?;
        }

        progress(ProgressEvent::new(ProgressPhase::TransferComplete, 0.90));
        self.io.send_marker(TRANSFER_COMPLETE_MARKER)?;

        progress(ProgressEvent::new(ProgressPhase::PostVerify, 0.95));
        self.io.post_verify()?;

        progress(ProgressEvent::new(ProgressPhase::Done, 1.0));
        Ok(FirmwareExecutionResult {
            bytes_sent: image.len(),
            chunk_count,
            checksum_24: lower_24_bits(padded_checksum_64(image)),
        })
    }
}

pub fn padded_checksum_64(image: &[u8]) -> u32 {
    let padded_len = image.len().next_multiple_of(CHUNK_SIZE);
    let mut total = 0u32;
    for index in 0..padded_len {
        let byte = image.get(index).copied().unwrap_or(0xFF);
        total = total.wrapping_add(byte as u32);
    }
    total
}

pub fn lower_24_bits(checksum: u32) -> u32 {
    checksum & 0x00FF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopIo;

    impl FirmwareIo for NoopIo {
        fn enter_bootloader(&mut self) -> Result<()> {
            Ok(())
        }

        fn wait_for_bootloader(&mut self) -> Result<()> {
            Ok(())
        }

        fn send_marker(&mut self, _marker: [u8; 2]) -> Result<()> {
            Ok(())
        }

        fn send_chunk(&mut self, _chunk_index: usize, _chunk: &[u8]) -> Result<()> {
            Ok(())
        }

        fn post_verify(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn transfer_markers_match_protocol() {
        assert_eq!(TRANSFER_START_MARKER, [0xBA, 0xC0]);
        assert_eq!(TRANSFER_COMPLETE_MARKER, [0xBA, 0xC2]);
    }

    #[test]
    fn checksum_includes_ff_padding() {
        let sum = padded_checksum_64(&[1, 2, 3]);
        let expected = 1u32 + 2 + 3 + (61u32 * 0xFF);
        assert_eq!(sum, expected);
    }

    #[test]
    fn engine_executes_chunks() {
        let mut engine = DefaultFirmwareEngine::new(NoopIo);
        let mut seen = Vec::new();
        let result = engine
            .execute(&vec![0xAB; 130], &mut |evt| seen.push(evt.phase))
            .expect("engine should execute");
        assert_eq!(result.chunk_count, 3);
        assert!(seen.contains(&ProgressPhase::Done));
    }
}
