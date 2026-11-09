//! og-shape for wasm32 (`docs/design.md` §3 and §5): the model program
//! API behind the TypeScript binding in `ts/` (the private `og-shape`
//! npm package).
//!
//! Entry: [`Model`], a [`shape_model::program::Session`] behind
//! wasm-bindgen. The binding sends each call as one JSON operation
//! ([`Model::apply`]) and reads the answer back as JSON; files come back
//! as bytes. The boundary is strings and byte arrays only, so the
//! TypeScript side holds no geometry and the kernel's determinism rules
//! hold unchanged: frames are turned here, with `trig`, never in
//! JavaScript.

use shape_model::program::Session;
use shape_model::{ModelError, Preview};
use wasm_bindgen::prelude::*;

fn js(e: ModelError) -> JsError {
    JsError::new(&e.to_string())
}

/// A model being built, and the program that built it.
#[wasm_bindgen]
pub struct Model {
    session: Session,
}

#[wasm_bindgen]
impl Model {
    /// An empty model named `name`.
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str) -> Model {
        Model {
            session: Session::new(name),
        }
    }

    /// A model replayed from a recorded program.
    pub fn replay(program: &str) -> Result<Model, JsError> {
        Ok(Model {
            session: Session::replay(program).map_err(js)?,
        })
    }

    /// Applies one JSON operation (`shape_model::program`); answers JSON.
    pub fn apply(&mut self, op: &str) -> Result<String, JsError> {
        self.session.apply(op).map_err(js)
    }

    /// The operations that changed the model, as a program.
    pub fn program(&self) -> String {
        self.session.program()
    }

    /// The posed model as GLB.
    pub fn glb(&self) -> Result<Vec<u8>, JsError> {
        self.session.model().glb().map_err(js)
    }

    /// The model's meshes as OBJ text.
    pub fn obj(&self) -> Result<String, JsError> {
        self.session.model().obj().map_err(js)
    }

    /// The model's meshes as binary STL.
    pub fn stl(&self) -> Result<Vec<u8>, JsError> {
        self.session.model().stl().map_err(js)
    }

    /// Measures as JSON.
    pub fn measure(&self) -> Result<String, JsError> {
        shape_model::program::measures_json(self.session.model()).map_err(js)
    }

    /// A turntable of the posed model as one PNG, `columns` views a row
    /// (0 for all in one). Zeros take the preview's defaults; a negative
    /// `elevation` is the default 25°.
    #[allow(clippy::too_many_arguments)]
    pub fn preview(
        &self,
        width: u32,
        height: u32,
        views: u32,
        columns: u32,
        elevation: f64,
        azimuth: f64,
        no_edges: bool,
        no_ground: bool,
    ) -> Result<Vec<u8>, JsError> {
        let d = Preview {
            width,
            height,
            views,
            elevation: (elevation >= 0.0).then_some(elevation),
            azimuth,
            no_edges,
            no_ground,
        };
        let images = self.session.model().preview(&d).map_err(js)?;
        let columns = if columns == 0 {
            images.len() as u32
        } else {
            columns
        };
        Ok(shape_tools::png(&shape_tools::sheet(&images, columns)))
    }
}
