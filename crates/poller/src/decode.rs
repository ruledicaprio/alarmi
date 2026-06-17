//! Register decoders. Byte/word order matches the Eaton SC200/300 Modbus
//! server and the Python reference (modbus_working.py): big-endian word order,
//! IEEE-754 big-endian float (ABCD). Two 16-bit registers per 32-bit value.

/// 32-bit IEEE-754 float from two regs, big-endian word order (regs[0] high).
pub fn f32_be(hi: u16, lo: u16) -> Option<f32> {
    let bits = ((hi as u32) << 16) | (lo as u32);
    let v = f32::from_bits(bits);
    if v.is_nan() || v.is_infinite() { None } else { Some(v) }
}

/// Unsigned 32-bit from two regs, big-endian word order.
pub fn u32_be(hi: u16, lo: u16) -> u32 {
    ((hi as u32) << 16) | (lo as u32)
}

/// Signed 32-bit from two regs, big-endian word order.
pub fn i32_be(hi: u16, lo: u16) -> i32 {
    u32_be(hi, lo) as i32
}

/// Eaton summary status from the first four discrete inputs (1001..1004):
/// Critical / Major / Minor / Warning, else OK. Too few bits => comm error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus { Critical, Major, Minor, Warning, Ok, CommError }

pub fn status_from_summary(bits: &[bool]) -> DeviceStatus {
    if bits.len() < 4 { return DeviceStatus::CommError; }
    if bits[0] { DeviceStatus::Critical }
    else if bits[1] { DeviceStatus::Major }
    else if bits[2] { DeviceStatus::Minor }
    else if bits[3] { DeviceStatus::Warning }
    else { DeviceStatus::Ok }
}

/// Wire (PDU) address from a 1-based documented register, honoring base0.
/// All BHT SC300 devices use base0=false (doc address used directly).
pub fn wire_addr(doc: u16, base0: bool) -> u16 {
    if base0 { doc - 1 } else { doc }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_known_patterns() {
        // 53.5 V battery -> IEEE754 0x425C0000
        assert_eq!(f32_be(0x4256, 0x0000), Some(53.5));
        // 230.0 V -> 0x43660000
        assert_eq!(f32_be(0x4366, 0x0000), Some(230.0));
        // 0.0
        assert_eq!(f32_be(0x0000, 0x0000), Some(0.0));
        // NaN rejected
        assert_eq!(f32_be(0x7FC0, 0x0000), None);
    }

    #[test]
    fn u32_i32_word_order() {
        assert_eq!(u32_be(0x0001, 0x0000), 65536);
        assert_eq!(u32_be(0x0000, 0x0001), 1);
        assert_eq!(i32_be(0xFFFF, 0xFFFF), -1);
        assert_eq!(i32_be(0x8000, 0x0000), i32::MIN);
    }

    #[test]
    fn summary_bits_priority() {
        assert_eq!(status_from_summary(&[true,  true,  false, false]), DeviceStatus::Critical);
        assert_eq!(status_from_summary(&[false, true,  false, false]), DeviceStatus::Major);
        assert_eq!(status_from_summary(&[false, false, false, true ]), DeviceStatus::Warning);
        assert_eq!(status_from_summary(&[false, false, false, false]), DeviceStatus::Ok);
        assert_eq!(status_from_summary(&[true]),                       DeviceStatus::CommError);
    }

    #[test]
    fn addressing() {
        assert_eq!(wire_addr(1001, false), 1001);
        assert_eq!(wire_addr(1001, true), 1000);
    }
}
