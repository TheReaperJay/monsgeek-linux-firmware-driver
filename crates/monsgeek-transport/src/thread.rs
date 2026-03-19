//! Dedicated transport thread for serialized HID I/O with throttling and hot-plug.
//!
//! The transport thread owns the `UsbSession` and processes `CommandRequest`s
//! from a bounded channel, enforcing a minimum 100ms inter-command delay.

// Placeholder — tests drive implementation.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_request_fields() {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let req = CommandRequest {
            cmd: 0x8F,
            data: vec![0x01, 0x02],
            checksum: monsgeek_protocol::ChecksumType::Bit7,
            is_query: true,
            response_tx: tx,
        };
        assert_eq!(req.cmd, 0x8F);
        assert_eq!(req.data, vec![0x01, 0x02]);
        assert!(req.is_query);
    }

    #[test]
    fn test_transport_event_device_arrived() {
        let event = TransportEvent::DeviceArrived {
            vid: 0x3141,
            pid: 0x4005,
            bus: 1,
            address: 5,
        };
        match event {
            TransportEvent::DeviceArrived { vid, pid, bus, address } => {
                assert_eq!(vid, 0x3141);
                assert_eq!(pid, 0x4005);
                assert_eq!(bus, 1);
                assert_eq!(address, 5);
            }
            _ => panic!("expected DeviceArrived"),
        }
    }

    #[test]
    fn test_transport_event_device_left() {
        let event = TransportEvent::DeviceLeft {
            bus: 1,
            address: 5,
        };
        match event {
            TransportEvent::DeviceLeft { bus, address } => {
                assert_eq!(bus, 1);
                assert_eq!(address, 5);
            }
            _ => panic!("expected DeviceLeft"),
        }
    }

    #[test]
    fn test_transport_event_is_clone() {
        let event = TransportEvent::DeviceArrived {
            vid: 0x3141,
            pid: 0x4005,
            bus: 1,
            address: 5,
        };
        let cloned = event.clone();
        match cloned {
            TransportEvent::DeviceArrived { vid, .. } => assert_eq!(vid, 0x3141),
            _ => panic!("expected DeviceArrived"),
        }
    }

    #[test]
    fn test_transport_event_is_debug() {
        let event = TransportEvent::DeviceArrived {
            vid: 0x3141,
            pid: 0x4005,
            bus: 1,
            address: 5,
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("DeviceArrived"));
    }

    #[test]
    fn test_spawn_transport_thread_exists_with_correct_signature() {
        let _fn_ptr: fn(
            crossbeam_channel::Receiver<CommandRequest>,
            crossbeam_channel::Sender<TransportEvent>,
            crate::usb::UsbSession,
        ) -> std::thread::JoinHandle<()> = spawn_transport_thread;
    }
}
