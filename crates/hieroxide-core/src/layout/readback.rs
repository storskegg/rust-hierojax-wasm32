//! Whether a browser's canvas can be believed.
//!
//! Hardened browsers (Firefox with `privacy.resistFingerprinting`, Tor and Mullvad Browser as far as
//! we know) return noise from `getImageData`. Upstream then counts every pixel as ink and lays out
//! garbage, with no error. The host draws one known pixel on a small canvas, reads it back, and asks
//! this. A browser that fails has corrupted readback: compat falls back to outlines where that
//! build is there, and otherwise goes on as upstream does (`js/hieroxide/compat-fallback.js`).

/// The side of the host's canvas.
pub const PROBE_SIDE: u32 = 8;
/// Where the host draws its pixel, with `fillRect(x, y, 1, 1)`, in the colour below.
pub const PROBE_X: u32 = 3;
pub const PROBE_Y: u32 = 2;
/// `rgb(10, 20, 30)`, opaque: a colour that survives any premultiplied alpha.
pub const PROBE_RGBA: [u8; 4] = [10, 20, 30, 255];

/// The pixels the host read (`getImageData(0, 0, PROBE_SIDE, PROBE_SIDE).data`): exact, or not sane.
pub fn readback_is_sane(data: &[u8]) -> bool {
    let side = PROBE_SIDE as usize;
    if data.len() != side * side * 4 {
        return false;
    }
    let at = (PROBE_Y as usize * side + PROBE_X as usize) * 4;
    data.chunks_exact(4).enumerate().all(|(i, px)| if i * 4 == at { px == PROBE_RGBA } else { px == [0, 0, 0, 0] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean() -> Vec<u8> {
        let mut d = vec![0u8; 8 * 8 * 4];
        d[(2 * 8 + 3) * 4..(2 * 8 + 3) * 4 + 4].copy_from_slice(&PROBE_RGBA);
        d
    }

    #[test]
    fn exact_is_sane() {
        assert!(readback_is_sane(&clean()));
    }

    #[test]
    fn anything_else_is_not() {
        assert!(!readback_is_sane(&[]));
        assert!(!readback_is_sane(&vec![0u8; 8 * 8 * 4]));
        let mut d = clean();
        d[0] = 1; // noise in an empty pixel
        assert!(!readback_is_sane(&d));
        let mut d = clean();
        d[(2 * 8 + 3) * 4] = 11; // noise in the drawn pixel
        assert!(!readback_is_sane(&d));
        assert!(!readback_is_sane(&vec![200u8; 8 * 8 * 4]));
    }
}
