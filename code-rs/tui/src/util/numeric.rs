/// Saturating conversion from `usize` to `u16`, clamping at `u16::MAX`.
#[inline]
pub(crate) fn clamp_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}
