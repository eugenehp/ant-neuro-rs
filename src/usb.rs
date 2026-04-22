//! Native USB backend for ANT Neuro eego amplifiers.
//!
//! This module provides direct USB communication with eego devices on platforms
//! where the native SDK shared library is not available (e.g., macOS).
//!
//! USB device identification:
//! - Vendor ID:  0x2a56
//! - Product ID: 0xee01
//!
//! The eego amplifiers use USB bulk and isochronous transfers for data streaming.
//! The USB protocol is proprietary to ANT Neuro / eemagine Medical Imaging Solutions.

use crate::error::{AntNeuroError, Result};
use crate::types::AmplifierInfo;

/// ANT Neuro / eemagine USB Vendor ID.
pub const EEGO_VID: u16 = 0x2a56;
/// ANT Neuro eego USB Product ID.
pub const EEGO_PID: u16 = 0xee01;

/// Discover connected eego USB devices without loading the native SDK library.
/// Works on all platforms (Linux, Windows, macOS) via libusb.
pub fn find_eego_devices() -> Result<Vec<UsbDeviceInfo>> {
    let mut devices = Vec::new();

    let usb_devices = match rusb::devices() {
        Ok(d) => d,
        Err(e) => {
            log::warn!("Failed to enumerate USB devices: {}", e);
            return Ok(devices);
        }
    };

    for device in usb_devices.iter() {
        let desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };

        if desc.vendor_id() == EEGO_VID && desc.product_id() == EEGO_PID {
            let handle = device.open().ok();
            let serial = handle.as_ref().and_then(|h| {
                h.read_string_descriptor_ascii(desc.serial_number_string_index()?)
                    .ok()
            });
            let product = handle.as_ref().and_then(|h| {
                h.read_string_descriptor_ascii(desc.product_string_index()?)
                    .ok()
            });
            let manufacturer = handle.as_ref().and_then(|h| {
                h.read_string_descriptor_ascii(desc.manufacturer_string_index()?)
                    .ok()
            });

            devices.push(UsbDeviceInfo {
                bus: device.bus_number(),
                address: device.address(),
                vendor_id: desc.vendor_id(),
                product_id: desc.product_id(),
                serial: serial.unwrap_or_default(),
                product: product.unwrap_or_default(),
                manufacturer: manufacturer.unwrap_or_default(),
            });
        }
    }

    Ok(devices)
}

/// Information about a discovered eego USB device.
#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    pub bus: u8,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: String,
    pub product: String,
    pub manufacturer: String,
}

impl From<&UsbDeviceInfo> for AmplifierInfo {
    fn from(usb: &UsbDeviceInfo) -> Self {
        // The amplifier id is assigned by index at enumeration time to
        // match libeego's 0-based numbering. `get_amplifiers_info`
        // overrides this after collecting the list.
        AmplifierInfo {
            id: 0,
            serial: usb.serial.clone(),
        }
    }
}

/// Print detailed USB descriptor information for all connected eego devices.
/// Useful for protocol analysis and debugging.
pub fn dump_eego_descriptors() -> Result<()> {
    let usb_devices = rusb::devices()
        .map_err(|e| AntNeuroError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    for device in usb_devices.iter() {
        let desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };

        if desc.vendor_id() != EEGO_VID || desc.product_id() != EEGO_PID {
            continue;
        }

        println!("=== eego USB Device ===");
        println!(
            "  Bus {:03} Device {:03}: {:04x}:{:04x}",
            device.bus_number(),
            device.address(),
            desc.vendor_id(),
            desc.product_id()
        );
        println!("  USB version: {}", desc.usb_version());
        println!("  Device class: {}", desc.class_code());
        println!("  Subclass: {}", desc.sub_class_code());
        println!("  Protocol: {}", desc.protocol_code());
        println!("  Max packet size (EP0): {}", desc.max_packet_size());
        println!("  Num configurations: {}", desc.num_configurations());

        if let Ok(handle) = device.open() {
            if let Some(idx) = desc.manufacturer_string_index() {
                if let Ok(s) = handle.read_string_descriptor_ascii(idx) {
                    println!("  Manufacturer: {}", s);
                }
            }
            if let Some(idx) = desc.product_string_index() {
                if let Ok(s) = handle.read_string_descriptor_ascii(idx) {
                    println!("  Product: {}", s);
                }
            }
            if let Some(idx) = desc.serial_number_string_index() {
                if let Ok(s) = handle.read_string_descriptor_ascii(idx) {
                    println!("  Serial: {}", s);
                }
            }
        }

        for cfg_idx in 0..desc.num_configurations() {
            if let Ok(config) = device.config_descriptor(cfg_idx) {
                println!("  Configuration {}:", config.number());
                println!("    Num interfaces: {}", config.num_interfaces());
                println!(
                    "    Max power: {} mA",
                    config.max_power() as u32 * 2
                );

                for interface in config.interfaces() {
                    for iface_desc in interface.descriptors() {
                        println!(
                            "    Interface {} Alt {}:",
                            iface_desc.interface_number(),
                            iface_desc.setting_number()
                        );
                        println!("      Class: {}", iface_desc.class_code());
                        println!("      Subclass: {}", iface_desc.sub_class_code());
                        println!("      Protocol: {}", iface_desc.protocol_code());
                        println!("      Num endpoints: {}", iface_desc.num_endpoints());

                        for ep in iface_desc.endpoint_descriptors() {
                            let dir = match ep.direction() {
                                rusb::Direction::In => "IN",
                                rusb::Direction::Out => "OUT",
                            };
                            let transfer = match ep.transfer_type() {
                                rusb::TransferType::Control => "Control",
                                rusb::TransferType::Isochronous => "Isochronous",
                                rusb::TransferType::Bulk => "Bulk",
                                rusb::TransferType::Interrupt => "Interrupt",
                            };
                            println!(
                                "      EP 0x{:02x}: {} {} (max packet: {})",
                                ep.address(),
                                dir,
                                transfer,
                                ep.max_packet_size()
                            );
                        }
                    }
                }
            }
        }
        println!();
    }

    Ok(())
}
