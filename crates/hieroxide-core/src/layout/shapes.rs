//! Glyph measurement: `Shapes.measureGlyph`, its memo, `emSizeOf`, and
//! `PrintedAny.correctedMeasurement`.
//!
//! Upstream keeps the memo in a global (`shapes.measurements`). Here it is the
//! [`Engine`]'s, so two engines never share one. It is otherwise upstream's, key for
//! key: the scales are rounded to two decimals in the key, and the first measurement
//! stored under a key is the one every later lookup gets, so results can depend on the
//! order fragments are laid out in. That is kept on purpose.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use super::jsmath::js_max;
use super::raster::{DrawCmd, Plane, Raster};
use super::LayoutError;
use crate::jsnum::{js_round, number_to_string, to_fixed};

/// `Shapes.REFERENCE_GLYPH`, the sitting man.
pub const REFERENCE_GLYPH: char = '\u{13000}';
/// `Shapes.MEASURE_SIZE`.
pub const MEASURE_SIZE: f64 = 150.0;

/// The result of measuring one glyph, in pixels at the font size it was measured at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meas {
    pub width: f64,
    pub height: f64,
    pub width_scaled: f64,
    pub height_scaled: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// `{ w, h }` of `emSizeOf`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmSize {
    pub w: f64,
    pub h: f64,
}

/// What `PrintedAny.correctedMeasurement` reads of the `properties` argument of
/// `addSign` (and the printers read the rest).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SignProps {
    pub unselectable: bool,
    pub bracket: bool,
    pub extra: bool,
    pub x_as: Option<char>,
    pub y_as: Option<char>,
}

/// FNV-1a: a cheap, deterministic hasher for the memo keys.
#[derive(Default)]
pub struct Fnv(u64);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 { 0xcbf29ce484222325 } else { self.0 };
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        self.0 = h;
    }
}

/// Owns what upstream keeps in globals: the measurement memo (and the font URL that
/// `PrintedSVG.EXTERNAL_FONT` holds).
pub struct Engine {
    measurements: HashMap<String, Meas, BuildHasherDefault<Fnv>>,
    key: String,
    /// `PrintedSVG.EXTERNAL_FONT`, used by the standalone SVG style.
    pub external_font: String,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

impl Engine {
    pub fn new() -> Engine {
        Engine {
            measurements: HashMap::default(),
            key: String::new(),
            external_font: "NewGardiner.otf".to_string(),
        }
    }

    /// How many measurements the memo holds.
    pub fn memo_len(&self) -> usize {
        self.measurements.len()
    }

    /// Forget every measurement (upstream's memo is only ever cleared by a page reload).
    pub fn clear_memo(&mut self) {
        self.measurements.clear();
    }

    /// `Shapes.measureGlyph`.
    pub fn measure_glyph<R: Raster>(
        raster: &mut R,
        ch: char,
        font_size: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
    ) -> Result<Meas, LayoutError> {
        // const width = Math.max(1, Math.round(ctx.measureText(ch).width));
        let width = js_max(1.0, js_round(raster.advance(ch, font_size)?));
        let height = js_max(1.0, js_round(font_size));
        // Upstream quirk: measureText is called a second time for widthScaled.
        let width_scaled = js_max(1.0, js_round(x_scale * raster.advance(ch, font_size)?));
        let height_scaled = js_max(1.0, js_round(y_scale * font_size));
        let mut margin_h = 3.0;
        let mut margin_v = 3.0;
        if rotate % 180.0 != 0.0 && !(rotate % 180.0).is_nan() {
            let mut dim = js_max(width_scaled, height_scaled);
            if rotate % 90.0 != 0.0 && !(rotate % 90.0).is_nan() {
                dim *= 2f64.sqrt();
            }
            margin_h += ((dim - width_scaled) / 2.0).ceil();
            margin_v += ((dim - height_scaled) / 2.0).ceil();
        }
        let w_canvas = width_scaled + 2.0 * margin_h;
        let h_canvas = height_scaled + 2.0 * margin_v;
        let cmd = DrawCmd {
            ch,
            font_size,
            tx: margin_h + width_scaled / 2.0,
            ty: margin_v + height_scaled / 2.0,
            sx: if mirror { -x_scale } else { x_scale },
            sy: y_scale,
            rotate: rotate * core::f64::consts::PI / 180.0,
            x: -width / 2.0,
            y: height / 2.0,
        };
        // The sizes go as they are: a NaN makes the host's `getImageData` throw, as upstream's.
        let bitmap = raster.draw(w_canvas, h_canvas, &[cmd])?;
        let margins = Plane::new(&bitmap).margins();
        let x = margins.l as f64 - margin_h;
        let y = margin_v - margins.b as f64;
        let w = w_canvas - margins.l as f64 - margins.r as f64;
        let h = h_canvas - margins.t as f64 - margins.b as f64;
        Ok(Meas { width, height, width_scaled, height_scaled, x, y, w, h })
    }

    /// `Shapes.propertiesKey`, written into the engine's key buffer.
    fn properties_key(&mut self, ch: char, font_size: f64, x_scale: f64, y_scale: f64, rotate: f64, mirror: bool) {
        let k = &mut self.key;
        k.clear();
        k.push(ch);
        k.push(' ');
        to_fixed(font_size, 2, k);
        k.push(' ');
        to_fixed(x_scale, 2, k);
        k.push(' ');
        to_fixed(y_scale, 2, k);
        k.push(' ');
        number_to_string(rotate, k);
        k.push(' ');
        k.push_str(if mirror { "true" } else { "false" });
    }

    /// `shapes.measureGlyphMemo`: the key is built with `MEASURE_SIZE`, the first
    /// measurement under a key is kept, and the result is scaled to `font_size`.
    pub fn measure_glyph_memo<R: Raster>(
        &mut self,
        raster: &mut R,
        ch: char,
        font_size: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
    ) -> Result<Meas, LayoutError> {
        self.properties_key(ch, MEASURE_SIZE, x_scale, y_scale, rotate, mirror);
        let meas = match self.measurements.get(self.key.as_str()) {
            Some(m) => *m,
            None => {
                let m = Engine::measure_glyph(raster, ch, MEASURE_SIZE, x_scale, y_scale, rotate, mirror)?;
                self.measurements.insert(self.key.clone(), m);
                m
            }
        };
        let scale_down = font_size / MEASURE_SIZE;
        Ok(Meas {
            width: meas.width * scale_down,
            height: meas.height * scale_down,
            width_scaled: meas.width_scaled * scale_down,
            height_scaled: meas.height_scaled * scale_down,
            x: meas.x * scale_down,
            y: meas.y * scale_down,
            w: meas.w * scale_down,
            h: meas.h * scale_down,
        })
    }

    /// `shapes.emSizeOf`.
    pub fn em_size_of<R: Raster>(
        &mut self,
        raster: &mut R,
        ch: char,
        font_size: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
    ) -> Result<EmSize, LayoutError> {
        let ref_height = self.measure_glyph_memo(raster, REFERENCE_GLYPH, font_size, 1.0, 1.0, 0.0, false)?.h;
        let meas = self.measure_glyph_memo(raster, ch, font_size, x_scale, y_scale, rotate, mirror)?;
        Ok(EmSize { w: meas.w / ref_height, h: meas.h / ref_height })
    }

    /// `PrintedAny.correctedMeasurement`.
    pub fn corrected_measurement<R: Raster>(
        &mut self,
        raster: &mut R,
        ch: char,
        font_size: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
        props: &SignProps,
    ) -> Result<Meas, LayoutError> {
        let mut meas = self.measure_glyph_memo(raster, ch, font_size, x_scale, y_scale, rotate, mirror)?;
        if let Some(x_as) = props.x_as {
            let m = self.measure_glyph_memo(raster, x_as, font_size, 1.0, 1.0, rotate, mirror)?;
            meas.x = m.x;
            meas.width = m.width;
            meas.width_scaled = m.width_scaled;
        } else if let Some(y_as) = props.y_as {
            let m = self.measure_glyph_memo(raster, y_as, font_size, 1.0, 1.0, rotate, mirror)?;
            meas.y = m.y;
            meas.h = m.h;
            meas.height_scaled = m.height_scaled;
        } else if x_scale != 1.0 {
            let m = self.measure_glyph_memo(raster, ch, font_size, 1.0, 1.0, rotate, mirror)?;
            meas.y = m.y;
            meas.h = m.h;
            meas.height_scaled = m.height_scaled;
        } else if y_scale != 1.0 {
            let m = self.measure_glyph_memo(raster, ch, font_size, 1.0, 1.0, rotate, mirror)?;
            meas.x = m.x;
            meas.width = m.width;
            meas.width_scaled = m.width_scaled;
        }
        Ok(meas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::raster::BoxRaster;

    #[test]
    fn measures_a_box() {
        let mut r = BoxRaster::default();
        let m = Engine::measure_glyph(&mut r, 'a', 150.0, 1.0, 1.0, 0.0, false).unwrap();
        assert_eq!((m.width, m.height), (75.0, 150.0));
        assert_eq!((m.width_scaled, m.height_scaled), (75.0, 150.0));
        assert_eq!((m.w, m.h), (75.0, 150.0));
        assert_eq!((m.x, m.y), (0.0, 0.0));
    }

    #[test]
    fn memo_keeps_the_first_value_for_a_rounded_key() {
        let mut e = Engine::new();
        let mut r = BoxRaster::default();
        // 0.504 and 0.496 are both "0.50" in the key; their own widths would be 38 and 37.
        let a = e.measure_glyph_memo(&mut r, 'a', 150.0, 0.504, 1.0, 0.0, false).unwrap();
        let b = e.measure_glyph_memo(&mut r, 'a', 150.0, 0.496, 1.0, 0.0, false).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.width_scaled, 38.0);
        assert_eq!(a.w, 38.0);
        assert_eq!(e.memo_len(), 1);
        assert_eq!(r.draws, 1);
        // The other order gives the other value.
        let mut e = Engine::new();
        let b2 = e.measure_glyph_memo(&mut r, 'a', 150.0, 0.496, 1.0, 0.0, false).unwrap();
        assert_eq!(b2.w, 37.0);
    }

    #[test]
    fn em_size_is_relative_to_the_reference_glyph() {
        let mut e = Engine::new();
        let mut r = BoxRaster::default();
        let s = e.em_size_of(&mut r, 'a', 22.0, 1.0, 1.0, 0.0, false).unwrap();
        assert_eq!((s.w, s.h), (0.5, 1.0));
    }
}
