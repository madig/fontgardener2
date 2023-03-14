use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    str::FromStr,
};

use norad::Codepoints;
use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};

use crate::errors::{LoadError, SaveError};

#[derive(Debug, Default, PartialEq)]
pub struct Fontgarden {
    pub glyphs: HashMap<String, Glyph>,
}

impl Fontgarden {
    pub fn new() -> Self {
        Self::default()
    }

    const SET_CSV_HEADER: [&str; 4] =
        ["name", "postscript_name", "codepoints", "opentype_category"];

    pub fn load(path: &Path) -> Result<Self, LoadError> {
        if !path.is_dir() {
            return Err(LoadError::NotAFontgarden);
        }

        let mut fontgarden = Self::new();
        let mut seen_glyph_names: HashSet<String> = HashSet::new();

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                // TODO: Figure out when this call is None and if we should deal
                // with it.
                if let Some(file_name) = path.file_name() {
                    if let Some(set_name) = file_name.to_string_lossy().strip_prefix("set.") {
                        todo!()
                    }
                }
            }
        }

        Ok(fontgarden)
    }

    pub fn save(&self, path: &Path) -> Result<(), SaveError> {
        if path.exists() {
            std::fs::remove_dir_all(path).map_err(SaveError::Cleanup)?;
        }
        std::fs::create_dir(path).map_err(SaveError::CreateDir)?;

        let common_name = "Common";
        let mut sorted_glyph_names: Vec<&str> = self.glyphs.keys().map(|n| n.as_str()).collect();
        sorted_glyph_names.sort();
        let mut glyphs_by_set: HashMap<&str, Vec<&str>> = HashMap::new();
        for name in sorted_glyph_names.iter() {
            let set_name = self.glyphs[*name].set.as_deref().unwrap_or(common_name);
            glyphs_by_set.entry(set_name).or_insert(vec![]).push(name);
        }

        for (set_name, glyph_names) in glyphs_by_set {
            let set_info_path = path.join(format!("set.{set_name}.csv"));
            let mut writer = csv::Writer::from_path(&set_info_path)
                .map_err(|e| SaveError::SaveSetData(set_name.into(), e))?;

            writer
                .write_record(Self::SET_CSV_HEADER)
                .map_err(|e| SaveError::SaveSetData(set_name.into(), e))?;
            for name in glyph_names {
                let glyph = &self.glyphs[name];
                let codepoints_str: String = glyph
                    .codepoints
                    .iter()
                    .map(|c| format!("{:04X}", c as usize))
                    .collect::<Vec<_>>()
                    .join(" ");
                writer
                    .serialize((
                        name,
                        &glyph.postscript_name,
                        codepoints_str,
                        &glyph.opentype_category,
                    ))
                    .map_err(|e| SaveError::SaveSetData(set_name.into(), e))?;
            }
            writer
                .flush()
                .map_err(|e| SaveError::SaveSetData(set_name.into(), e.into()))?;
        }

        let glyphs_dir = path.join("glyphs");
        self.glyphs
            .par_iter()
            .filter(|(_, glyph)| !glyph.is_empty())
            .map(|(name, glyph)| {
                let this_glyph_dir = glyphs_dir.join(name);
                std::fs::create_dir_all(&this_glyph_dir)
                    .map_err(|e| SaveError::CreateGlyphDir(name.clone(), e))?;
                for (layer_name, layer) in
                    glyph.layers.iter().filter(|(_, layer)| !layer.is_empty())
                {
                    let layer_path = this_glyph_dir.join(layer_name).with_extension("json");
                    let layer_file = std::fs::File::create(&layer_path)
                        .map_err(|e| SaveError::SaveLayer(name.clone(), layer_name.clone(), e))?;
                    serde_json::to_writer_pretty(layer_file, layer).map_err(|e| {
                        SaveError::SaveLayerJson(name.clone(), layer_name.clone(), e)
                    })?;
                }
                Ok(())
            })
            .collect::<Result<_, _>>()?;

        Ok(())
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Glyph {
    pub codepoints: Codepoints,
    pub layers: HashMap<String, Layer>,
    pub opentype_category: OpenTypeCategory,
    pub postscript_name: Option<String>,
    pub set: Option<String>,
}

impl Glyph {
    pub fn is_empty(&self) -> bool {
        self.layers.values().all(|layer| layer.is_empty())
    }
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub anchors: Vec<Anchor>,
    pub components: Vec<Component>,
    pub contours: Vec<Contour>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub vertical_origin: Option<f64>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub x_advance: Option<f64>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub y_advance: Option<f64>,
}

impl Layer {
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
            && self.components.is_empty()
            && self.contours.is_empty()
            && self.x_advance.is_none()
            && self.y_advance.is_none()
    }
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Contour {
    pub points: Vec<ContourPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContourPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub typ: PointType,
    #[serde(default, skip_serializing_if = "is_default")]
    pub smooth: bool,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointType {
    #[default]
    OffCurve,
    Move,
    Line,
    Curve,
    QCurve,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub transformation: AffineTransformation,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct AffineTransformation {
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub x_scale: f64,
    #[serde(default = "zero", skip_serializing_if = "is_zero")]
    pub xy_scale: f64,
    #[serde(default = "zero", skip_serializing_if = "is_zero")]
    pub yx_scale: f64,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub y_scale: f64,
    #[serde(default = "zero", skip_serializing_if = "is_zero")]
    pub x_offset: f64,
    #[serde(default = "zero", skip_serializing_if = "is_zero")]
    pub y_offset: f64,
}

fn zero() -> f64 {
    0.
}

fn one() -> f64 {
    1.
}

fn is_zero(f: &f64) -> bool {
    *f == 0.
}

fn is_one(f: &f64) -> bool {
    *f == 1.
}

impl AffineTransformation {
    ///  [1 0 0 1 0 0]; the identity transformation.
    fn identity() -> Self {
        Self {
            x_scale: 1.0,
            xy_scale: 0.,
            yx_scale: 0.,
            y_scale: 1.0,
            x_offset: 0.,
            y_offset: 0.,
        }
    }
}

impl Default for AffineTransformation {
    fn default() -> Self {
        Self::identity()
    }
}

#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenTypeCategory {
    #[default]
    Unassigned = 0,
    Base = 1,
    Ligature = 2,
    Mark = 3,
    Component = 4,
}

impl FromStr for OpenTypeCategory {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "base" => Ok(Self::Base),
            "component" => Ok(Self::Component),
            "ligature" => Ok(Self::Ligature),
            "mark" => Ok(Self::Mark),
            "unassigned" => Ok(Self::Unassigned),
            _ => Err("Category must be unassigned, base, ligature, mark or component"),
        }
    }
}

// TODO: Derive Deserialize and deal with the `parse()` call elsewhere differently.
impl<'de> Deserialize<'de> for OpenTypeCategory {
    fn deserialize<D>(deserializer: D) -> Result<OpenTypeCategory, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: &str = Deserialize::deserialize(deserializer)?;
        OpenTypeCategory::from_str(value).map_err(serde::de::Error::custom)
    }
}

impl From<&norad::Glyph> for Layer {
    fn from(glyph: &norad::Glyph) -> Self {
        // A glyph's "height" (y_advance) makes little sense unless there is also a
        // vertical origin in its lib.
        let vertical_origin = glyph
            .lib
            .get("public.verticalOrigin")
            .and_then(|o| o.as_real());
        let y_advance = vertical_origin.map(|_| glyph.height);

        Self {
            anchors: glyph.anchors.iter().map(|x| x.into()).collect(),
            components: glyph.components.iter().map(|x| x.into()).collect(),
            contours: glyph.contours.iter().map(|x| x.into()).collect(),
            vertical_origin,
            x_advance: glyph.width.into(),
            y_advance,
        }
    }
}

impl From<&norad::Anchor> for Anchor {
    fn from(anchor: &norad::Anchor) -> Self {
        Self {
            name: anchor
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_default(),
            x: anchor.x,
            y: anchor.y,
        }
    }
}

impl From<&norad::Contour> for Contour {
    fn from(value: &norad::Contour) -> Self {
        Self {
            points: value.points.iter().map(|x| x.into()).collect(),
        }
    }
}

impl From<&norad::ContourPoint> for ContourPoint {
    fn from(value: &norad::ContourPoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
            typ: value.typ.clone().into(),
            smooth: value.smooth,
        }
    }
}

impl From<norad::PointType> for PointType {
    fn from(value: norad::PointType) -> Self {
        match value {
            norad::PointType::Curve => Self::Curve,
            norad::PointType::Line => Self::Line,
            norad::PointType::Move => Self::Move,
            norad::PointType::OffCurve => Self::OffCurve,
            norad::PointType::QCurve => Self::QCurve,
        }
    }
}

impl From<&norad::Component> for Component {
    fn from(component: &norad::Component) -> Self {
        Self {
            name: component.base.to_string(),
            transformation: component.transform.into(),
        }
    }
}

impl From<norad::AffineTransform> for AffineTransformation {
    fn from(transform: norad::AffineTransform) -> Self {
        Self {
            x_scale: transform.x_scale,
            xy_scale: transform.xy_scale,
            yx_scale: transform.yx_scale,
            y_scale: transform.y_scale,
            x_offset: transform.x_offset,
            y_offset: transform.y_offset,
        }
    }
}
