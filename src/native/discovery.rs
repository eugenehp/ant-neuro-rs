// ── Endpoint discovery and alt-setting selection ─────────────────────────────

use crate::protocol;

/// Choose the best USB alternate setting for isochronous streaming.
///
/// Iterates alt settings in reverse order and picks the one whose
/// isochronous IN endpoint provides the smallest bandwidth that still
/// meets `required_bandwidth`. The bandwidth is calculated as
/// `max_packet_size * usb_version_multiplier`.
pub(crate) fn select_alt_setting(
    device: &rusb::Device<rusb::GlobalContext>,
    usb_version: u16,
    required_bandwidth: u32,
) -> u8 {
    let multiplier = if usb_version >= protocol::USB_VERSION_2_0 {
        if usb_version >= 0x0300 {
            protocol::USB3_BANDWIDTH_MULTIPLIER
        } else {
            protocol::USB2_BANDWIDTH_MULTIPLIER
        }
    } else {
        protocol::USB2_BANDWIDTH_MULTIPLIER
    };

    let config = match device.config_descriptor(0) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let mut best_alt = 0u8;
    let mut best_bandwidth = 0u32;

    for iface in config.interfaces() {
        let descs: Vec<_> = iface.descriptors().collect();
        for iface_desc in descs.iter().rev() {
            for ep in iface_desc.endpoint_descriptors() {
                if ep.transfer_type() == rusb::TransferType::Isochronous
                    && ep.direction() == rusb::Direction::In
                {
                    let bandwidth = ep.max_packet_size() as u32 * multiplier;
                    if bandwidth >= required_bandwidth && (best_bandwidth == 0 || bandwidth <= best_bandwidth) {
                        best_alt = iface_desc.setting_number();
                        best_bandwidth = bandwidth;
                    }
                }
            }
        }
    }

    log::debug!("usb alt interface {} selected (bandwidth: {})", best_alt, best_bandwidth);
    best_alt
}

// ── Endpoint discovery ──────────────────────────────────────────────────────

/// Discovered USB endpoint addresses for an eego device.
pub(crate) struct EndpointInfo {
    pub(crate) bulk_in: u8,
    pub(crate) bulk_out: u8,
    pub(crate) iso_in: Option<u8>,
}

/// Scan the device's configuration descriptor to find bulk and isochronous endpoints.
pub(crate) fn find_endpoints(device: &rusb::Device<rusb::GlobalContext>) -> EndpointInfo {
    let mut info = EndpointInfo {
        bulk_in: 0x81,
        bulk_out: 0x01,
        iso_in: None,
    };

    if let Ok(config) = device.config_descriptor(0) {
        for iface in config.interfaces() {
            for iface_desc in iface.descriptors() {
                for ep in iface_desc.endpoint_descriptors() {
                    match (ep.direction(), ep.transfer_type()) {
                        (rusb::Direction::In, rusb::TransferType::Bulk) => {
                            info.bulk_in = ep.address();
                        }
                        (rusb::Direction::Out, rusb::TransferType::Bulk) => {
                            info.bulk_out = ep.address();
                        }
                        (rusb::Direction::In, rusb::TransferType::Isochronous) => {
                            info.iso_in = Some(ep.address());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    info
}
