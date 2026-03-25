mod mock_transport;

use monsgeek_driver::bridge_transport;
use monsgeek_protocol::ChecksumType;

/// Confirms bridge_transport remains device-agnostic and does NOT perform
/// bounds validation. That responsibility belongs to the service layer.
#[tokio::test]
async fn test_bridge_transport_forwards_any_command() {
    let mock = mock_transport::MockTransport::new();
    let mut msg = vec![0u8; 64];
    msg[0] = 0x09; // SET_KEYMATRIX for YiChip
    msg[1] = 0; // profile
    msg[2] = 200; // key_index (OOB -- but bridge_transport does NOT validate)
    msg[6] = 10; // layer (OOB)
    let result =
        bridge_transport::send_command_with(mock.clone(), msg, ChecksumType::Bit7).await;
    assert!(result.is_ok(), "bridge_transport must not validate bounds");
    assert_eq!(mock.sent_commands().len(), 1);
}
