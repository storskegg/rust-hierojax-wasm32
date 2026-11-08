//! `PrintedCanvasWithoutExtras`: the canvas upstream prints a group on to look at its pixels.
//!
//! The canvas is the list of its `fillText`s and its size, both as upstream's calls would
//! leave them. Each read hands the list to [`Raster::draw`], which makes a fresh canvas and
//! replays them: the pixels are those of upstream's canvas at that moment, because each
//! `addSign` is wrapped in `save` and `restore` and so owes nothing to the signs before it.
//! A read with nothing drawn since the last gives the same pixels, and cannot newly fail, so
//! it is not repeated (`Shapes.plane` calls `getImageData` again; the JavaScript cannot tell).

use super::plan::Rect;
use super::raster::{canvas_dim, Bitmap, DrawCmd, Raster};
use super::shapes::{Engine, SignProps};
use super::LayoutError;
use crate::jsnum::js_round;

pub struct MeasCanvas {
    /// `this.fontSize`
    font_size: f64,
    /// `canvas.width`, `canvas.height`
    pub width: u32,
    pub height: u32,
    cmds: Vec<DrawCmd>,
    read: Option<Bitmap>,
}

impl MeasCanvas {
    /// `new PrintedCanvasWithoutExtras(fontSize, w, h)`:
    /// `canvas.width = Math.ceil(fontSize * w)` and the same for the height.
    pub fn new(font_size: f64, w: f64, h: f64) -> MeasCanvas {
        MeasCanvas {
            font_size,
            width: canvas_dim((font_size * w).ceil(), 300),
            height: canvas_dim((font_size * h).ceil(), 150),
            cmds: Vec::new(),
            read: None,
        }
    }

    /// `PrintedCanvasWithoutExtras.addSign`, then `PrintedCanvas.addSign`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_sign<Ra: Raster>(
        &mut self,
        eng: &mut Engine,
        raster: &mut Ra,
        ch: char,
        scale: f64,
        x_scale: f64,
        y_scale: f64,
        rotate: f64,
        mirror: bool,
        rect: &Rect,
        props: &SignProps,
    ) -> Result<(), LayoutError> {
        if props.extra {
            return Ok(());
        }
        // `reversed` is false on these canvases, so `reverse` is the identity and
        // `this.reversed ^ mirror` is `mirror`.
        let x_px = js_round(self.font_size * rect.x);
        let y_px = js_round(self.font_size * rect.y);
        let font_size = (self.font_size * scale).floor();
        let meas = eng.corrected_measurement(raster, ch, font_size, x_scale, y_scale, rotate, mirror, props)?;
        self.cmds.push(DrawCmd {
            ch,
            font_size,
            tx: x_px + meas.width_scaled / 2.0 - meas.x,
            ty: y_px - meas.height_scaled / 2.0 + meas.h - meas.y,
            sx: if mirror { -x_scale } else { x_scale },
            sy: y_scale,
            rotate: rotate * core::f64::consts::PI / 180.0,
            x: -meas.width / 2.0,
            y: meas.height / 2.0,
        });
        self.read = None;
        Ok(())
    }

    /// `ctx.getImageData(0, 0, canvas.width, canvas.height).data`.
    pub fn pixels<Ra: Raster>(&mut self, raster: &mut Ra) -> Result<&Bitmap, LayoutError> {
        if self.read.is_none() {
            let b = raster.draw(f64::from(self.width), f64::from(self.height), &self.cmds)?;
            self.read = Some(b);
        }
        self.read.as_ref().ok_or(LayoutError::Internal("canvas without pixels"))
    }

    /// The `fillText`s so far, for tests.
    pub fn cmds(&self) -> &[DrawCmd] {
        &self.cmds
    }
}
