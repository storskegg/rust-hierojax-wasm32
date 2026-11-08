//! `PrintedCanvas`: turns a [`Plan`] into the flat buffer of canvas operations the host
//! replays on real canvases in the page.
//!
//! Upstream draws on a canvas it appended to the element, with the same few calls for every
//! sign. What those calls are, and their numbers, are decided here (the measurements go
//! through the memo in the order of the plan, as upstream's `addSign` does them); a loop in
//! the host makes the calls. So the canvas, the text of its `fillText`s and the pixels are
//! upstream's, in the page's own engine.
//!
//! The buffer is a list of `f64`, one unit (a `PrintedCanvas`: the whole fragment, or one
//! group in separated mode) after the other:
//!
//! ```text
//! unit  = width height flags op* OP_END
//!         width, height  the numbers assigned to canvas.width and canvas.height
//!         flags          1 separated, 2 border (drawn on the canvas), 4 the hidden text follows
//! op    = OP_SIGN color font tx ty sx sy rotate x y char
//!       | OP_UNIFORM x y w h
//!       | OP_HATCH x y w h wAccum hAccum
//! ```
//!
//! `color` is 0 for `options.signcolor` and 1 for `options.bracketcolor`.

use super::options::Options;
use super::plan::{Item, Plan, Rect, Unit};
use super::raster::Raster;
use super::shapes::{Engine, SignProps};
use super::LayoutError;
use crate::jsnum::js_round;

pub const OP_END: f64 = 0.0;
pub const OP_SIGN: f64 = 1.0;
pub const OP_UNIFORM: f64 = 2.0;
pub const OP_HATCH: f64 = 3.0;

pub const FLAG_SEPARATED: f64 = 1.0;
pub const FLAG_BORDER: f64 = 2.0;
pub const FLAG_TEXT: f64 = 4.0;

/// What to replay, and the messages for `console.error` (for `options.log`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasOutput {
    pub ops: Vec<f64>,
    /// `this.toString()` of the fragment, for the hidden `div` after the canvas.
    pub text: String,
    pub signcolor: String,
    pub bracketcolor: String,
    pub logs: Vec<String>,
}

struct Printer<'a, Ra: Raster> {
    eng: &'a mut Engine,
    raster: &'a mut Ra,
    opts: &'a Options,
    plan: &'a Plan,
    out: CanvasOutput,
}

impl<Ra: Raster> Printer<'_, Ra> {
    fn unit(&mut self, unit: &Unit) -> Result<(), LayoutError> {
        let font = self.opts.layout.fontsize;
        let separated = self.opts.layout.separated;
        let mut flags = 0.0;
        if separated {
            flags += FLAG_SEPARATED;
        } else {
            // `printed.addText(this.toString())` follows the constructor in a fragment that is
            // not separated.
            flags += FLAG_TEXT;
            if self.opts.border {
                flags += FLAG_BORDER;
            }
        }
        // Math.ceil(this.wPx), Math.ceil(this.hPx)
        self.out.ops.push((font * unit.w).ceil());
        self.out.ops.push((font * unit.h).ceil());
        self.out.ops.push(flags);
        let mut failure = Ok(());
        for item in &unit.items {
            let r = match item {
                Item::Sign { ch, scale, x_scale, y_scale, rotate, mirror, rect, props } => {
                    self.add_sign(unit, *ch, *scale, *x_scale, *y_scale, *rotate, *mirror, rect, props)
                }
                // `PrintedCanvas.addHidden` does nothing.
                Item::Hidden { .. } => Ok(()),
                Item::Shade(r) => {
                    self.add_shading(unit, r);
                    Ok(())
                }
                Item::Log { start, len } => {
                    self.out.logs.push(self.plan.str(*start, *len).to_string());
                    Ok(())
                }
            };
            if r.is_err() {
                failure = r;
                break;
            }
        }
        self.out.ops.push(OP_END);
        failure
    }

    /// `PrintedCanvas.addSign`
    #[allow(clippy::too_many_arguments)]
    fn add_sign(
        &mut self,
        unit: &Unit,
        ch: char,
        scale: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
        rect: &Rect,
        props: &SignProps,
    ) -> Result<(), LayoutError> {
        let font = self.opts.layout.fontsize;
        let x = if unit.reversed { unit.w - (rect.x + rect.w) } else { rect.x };
        let mirror = unit.reversed != mirror;
        let x_px = js_round(font * x);
        let y_px = js_round(font * rect.y);
        let font_size = (font * scale).floor();
        let meas = self
            .eng
            .corrected_measurement(self.raster, ch, font_size, x_scale, y_scale, rotate, mirror, props)?;
        self.out.ops.extend_from_slice(&[
            OP_SIGN,
            if props.bracket { 1.0 } else { 0.0 },
            font_size,
            x_px + meas.width_scaled / 2.0 - meas.x,
            y_px - meas.height_scaled / 2.0 + meas.h - meas.y,
            if mirror { -x_scale } else { x_scale },
            y_scale,
            rotate * core::f64::consts::PI / 180.0,
            -meas.width / 2.0,
            meas.height / 2.0,
            f64::from(ch as u32),
        ]);
        Ok(())
    }

    /// `PrintedCanvas.addShading`
    fn add_shading(&mut self, unit: &Unit, r: &Rect) {
        let font = self.opts.layout.fontsize;
        let x = if unit.reversed { unit.w - (r.x + r.w) } else { r.x };
        let x_min = js_round(font * x);
        let y_min = js_round(font * r.y);
        let x_max = js_round(font * (x + r.w));
        let y_max = js_round(font * (r.y + r.h));
        let width = x_max - x_min;
        let height = y_max - y_min;
        if self.opts.uniform_shade {
            self.out.ops.extend_from_slice(&[OP_UNIFORM, x_min, y_min, width, height]);
        } else {
            self.out.ops.extend_from_slice(&[OP_HATCH, x_min, y_min, width, height, unit.w_accum, unit.h_accum]);
        }
    }
}

/// Print a plan on canvases. `text` is `Fragment.toString()`. On an exception it also gives
/// what upstream would have left in the element by then.
pub fn print_canvas_partial<Ra: Raster>(
    eng: &mut Engine,
    raster: &mut Ra,
    plan: &Plan,
    opts: &Options,
    text: &str,
) -> (CanvasOutput, Option<LayoutError>) {
    let out = CanvasOutput {
        ops: Vec::new(),
        text: if opts.layout.separated { String::new() } else { text.to_string() },
        signcolor: opts.signcolor.clone(),
        bracketcolor: opts.bracketcolor.clone(),
        logs: Vec::new(),
    };
    let mut p = Printer { eng, raster, opts, plan, out };
    let mut failure = None;
    for unit in &plan.units {
        if let Err(e) = p.unit(unit) {
            failure = Some(e);
            break;
        }
    }
    (p.out, failure)
}
