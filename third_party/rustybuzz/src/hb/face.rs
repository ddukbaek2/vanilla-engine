#[cfg(not(feature = "std"))]
use core_maths::CoreFloat;

use crate::hb::paint_extents::hb_paint_extents_context_t;
use ttf_parser::gdef::GlyphClass;
use ttf_parser::opentype_layout::LayoutTable;
use alloc::vec::Vec;
use ttf_parser::{GlyphId, NormalizedCoordinate, RgbaColor, Tag};

use super::buffer::GlyphPropsFlags;
use super::ot_layout::TableIndex;
use super::ot_layout_common::{PositioningTable, SubstitutionTable};
use crate::Variation;

// https://docs.microsoft.com/en-us/typography/opentype/spec/cmap#windows-platform-platform-id--3
const WINDOWS_SYMBOL_ENCODING: u16 = 0;
const WINDOWS_UNICODE_BMP_ENCODING: u16 = 1;
const WINDOWS_UNICODE_FULL_ENCODING: u16 = 10;

// https://docs.microsoft.com/en-us/typography/opentype/spec/name#platform-specific-encoding-and-language-ids-unicode-platform-platform-id--0
const UNICODE_1_0_ENCODING: u16 = 0;
const UNICODE_1_1_ENCODING: u16 = 1;
const UNICODE_ISO_ENCODING: u16 = 2;
const UNICODE_2_0_BMP_ENCODING: u16 = 3;
const UNICODE_2_0_FULL_ENCODING: u16 = 4;
//const UNICODE_VARIATION_ENCODING: u16 = 5;
const UNICODE_FULL_ENCODING: u16 = 6;

/// Integer scale of `hb_font_t::em_mult` (vanilla-engine change).
#[derive(Clone, Copy)]
pub(crate) struct PositionScale {
    x_mult: i64,
    y_mult: i64,
}

impl PositionScale {
    #[inline]
    pub(crate) fn em_scale_x(self, value: i32) -> i32 {
        ((i64::from(value) * self.x_mult + 32768) >> 16) as i32
    }

    #[inline]
    pub(crate) fn em_scale_y(self, value: i32) -> i32 {
        ((i64::from(value) * self.y_mult + 32768) >> 16) as i32
    }
}

/// A font face handle.
#[derive(Clone)]
pub struct hb_font_t<'a> {
    pub(crate) ttfp_face: ttf_parser::Face<'a>,
    pub(crate) units_per_em: u16,
    pixels_per_em: Option<(u16, u16)>,
    pub(crate) points_per_em: Option<f32>,
    // Scale like `hb_font_set_scale` (vanilla-engine change). Defaults to units per EM.
    pub(crate) x_scale: i32,
    pub(crate) y_scale: i32,
    x_mult: i64,
    y_mult: i64,
    x_multf: f32,
    y_multf: f32,
    // Variation coordinates normalized like HarfBuzz (vanilla-engine change).
    variation_coords: Vec<NormalizedCoordinate>,
    prefered_cmap_encoding_subtable: Option<u16>,
    pub(crate) gsub: Option<SubstitutionTable<'a>>,
    pub(crate) gpos: Option<PositioningTable<'a>>,
}

impl<'a> AsRef<ttf_parser::Face<'a>> for hb_font_t<'a> {
    #[inline]
    fn as_ref(&self) -> &ttf_parser::Face<'a> {
        &self.ttfp_face
    }
}

impl<'a> AsMut<ttf_parser::Face<'a>> for hb_font_t<'a> {
    #[inline]
    fn as_mut(&mut self) -> &mut ttf_parser::Face<'a> {
        &mut self.ttfp_face
    }
}

impl<'a> core::ops::Deref for hb_font_t<'a> {
    type Target = ttf_parser::Face<'a>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.ttfp_face
    }
}

impl<'a> core::ops::DerefMut for hb_font_t<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ttfp_face
    }
}

impl<'a> hb_font_t<'a> {
    /// Creates a new `Face` from data.
    ///
    /// Data will be referenced, not owned.
    pub fn from_slice(data: &'a [u8], face_index: u32) -> Option<Self> {
        let face = ttf_parser::Face::parse(data, face_index).ok()?;
        Some(Self::from_face(face))
    }

    /// Creates a new [`Face`] from [`ttf_parser::Face`].
    ///
    /// Data will be referenced, not owned.
    pub fn from_face(face: ttf_parser::Face<'a>) -> Self {
        let units_per_em = face.units_per_em();
        hb_font_t {
            units_per_em,
            pixels_per_em: None,
            points_per_em: None,
            x_scale: i32::from(units_per_em),
            y_scale: i32::from(units_per_em),
            x_mult: 0x10000,
            y_mult: 0x10000,
            x_multf: 1.0,
            y_multf: 1.0,
            variation_coords: face.variation_coordinates().to_vec(),
            prefered_cmap_encoding_subtable: find_best_cmap_subtable(&face),
            gsub: face.tables().gsub.map(SubstitutionTable::new),
            gpos: face.tables().gpos.map(PositioningTable::new),
            ttfp_face: face,
        }
    }

    // TODO: remove
    /// Returns face’s units per EM.
    #[inline]
    pub fn units_per_em(&self) -> i32 {
        self.units_per_em as i32
    }

    #[inline]
    pub(crate) fn pixels_per_em(&self) -> Option<(u16, u16)> {
        self.pixels_per_em
    }

    /// Sets pixels per EM.
    ///
    /// Used during raster glyphs processing and hinting.
    ///
    /// `None` by default.
    #[inline]
    pub fn set_pixels_per_em(&mut self, ppem: Option<(u16, u16)>) {
        self.pixels_per_em = ppem;
    }

    /// Sets point size per EM.
    ///
    /// Used for optical-sizing in Apple fonts.
    ///
    /// `None` by default.
    #[inline]
    pub fn set_points_per_em(&mut self, ptem: Option<f32>) {
        self.points_per_em = ptem;
    }

    /// Sets the scale of positions, like `hb_font_set_scale` (vanilla-engine change).
    ///
    /// Font units are scaled into this space as HarfBuzz does (`em_scale` for integers,
    /// `em_scalef` for variation deltas), so a large scale keeps fractional variation deltas.
    /// Units per EM by default, which gives font units.
    pub fn set_scale(&mut self, x_scale: i32, y_scale: i32) {
        let units_per_em = self.units_per_em.max(1);
        self.x_scale = x_scale;
        self.y_scale = y_scale;
        self.x_multf = x_scale as f32 / f32::from(units_per_em);
        self.y_multf = y_scale as f32 / f32::from(units_per_em);
        self.x_mult = i64::from(x_scale) * 0x10000 / i64::from(units_per_em);
        self.y_mult = i64::from(y_scale) * 0x10000 / i64::from(units_per_em);
    }

    /// Returns the scale set by `set_scale`.
    #[inline]
    pub fn scale(&self) -> (i32, i32) {
        (self.x_scale, self.y_scale)
    }

    #[inline]
    pub(crate) fn em_scale_x(&self, value: i32) -> i32 {
        self.position_scale().em_scale_x(value)
    }

    #[inline]
    pub(crate) fn em_scale_y(&self, value: i32) -> i32 {
        self.position_scale().em_scale_y(value)
    }

    #[inline]
    pub(crate) fn position_scale(&self) -> PositionScale {
        PositionScale {
            x_mult: self.x_mult,
            y_mult: self.y_mult,
        }
    }

    #[inline]
    pub(crate) fn x_multf(&self) -> f32 {
        self.x_multf
    }

    #[inline]
    pub(crate) fn y_multf(&self) -> f32 {
        self.y_multf
    }

    /// Sets a font variation (vanilla-engine change).
    ///
    /// The coordinate used for shaping is normalized like HarfBuzz `hb_ot_var_normalize_coords`:
    /// 16.16 normalization, `avar` mapped in floats, then rounded to 2.14.
    /// `ttf_parser::Face::set_variation` truncates to 2.14 first and maps `avar` in integers,
    /// which can differ by one 2.14 unit.
    pub fn set_variation(&mut self, axis: Tag, value: f32) -> Option<()> {
        self.ttfp_face.set_variation(axis, value)?;
        let segment_maps = self.ttfp_face.tables().avar.map(|avar| avar.segment_maps);
        for (axis_index, variation_axis) in self.ttfp_face.variation_axes().into_iter().enumerate() {
            if variation_axis.tag != axis {
                continue;
            }
            let segment_map = segment_maps.and_then(|segment_maps| segment_maps.into_iter().nth(axis_index));
            let coordinate = normalize_variation_like_harfbuzz(&variation_axis, value, segment_map);
            if let Some(variation_coord) = self.variation_coords.get_mut(axis_index) {
                *variation_coord = NormalizedCoordinate::from(coordinate);
            }
        }
        Some(())
    }

    /// Returns the variation coordinates normalized like HarfBuzz (vanilla-engine change).
    #[inline]
    pub fn variation_coordinates(&self) -> &[NormalizedCoordinate] {
        &self.variation_coords
    }

    /// Sets font variations.
    pub fn set_variations(&mut self, variations: &[Variation]) {
        for variation in variations {
            self.set_variation(variation.tag, variation.value);
        }
    }

    pub(crate) fn has_glyph(&self, c: u32) -> bool {
        self.get_nominal_glyph(c).is_some()
    }

    pub(crate) fn get_nominal_glyph(&self, mut c: u32) -> Option<GlyphId> {
        let subtable_idx = self.prefered_cmap_encoding_subtable?;
        let subtable = self.tables().cmap?.subtables.get(subtable_idx)?;

        if subtable.platform_id == ttf_parser::PlatformId::Macintosh && c > 0x7F {
            c = unicode_to_macroman(c);
        }

        match subtable.glyph_index(c) {
            Some(gid) => Some(gid),
            None => {
                // Special case for Windows Symbol fonts.
                // TODO: add tests
                if subtable.platform_id == ttf_parser::PlatformId::Windows
                    && subtable.encoding_id == WINDOWS_SYMBOL_ENCODING
                {
                    if c <= 0x00FF {
                        // For symbol-encoded OpenType fonts, we duplicate the
                        // U+F000..F0FF range at U+0000..U+00FF.  That's what
                        // Windows seems to do, and that's hinted about at:
                        // https://docs.microsoft.com/en-us/typography/opentype/spec/recom
                        // under "Non-Standard (Symbol) Fonts".
                        return self.get_nominal_glyph(0xF000 + c);
                    }
                }

                None
            }
        }
    }

    pub(crate) fn glyph_h_advance(&self, glyph: GlyphId) -> i32 {
        self.em_scale_x(self.glyph_advance(glyph, false) as i32)
    }

    pub(crate) fn glyph_v_advance(&self, glyph: GlyphId) -> i32 {
        self.em_scale_y(-(self.glyph_advance(glyph, true) as i32))
    }

    fn glyph_advance(&self, glyph: GlyphId, is_vertical: bool) -> u32 {
        let face = &self.ttfp_face;
        if face.is_variable()
            && face.has_non_default_variation_coordinates()
            && face.tables().hvar.is_none()
            && face.tables().vvar.is_none()
            && face.glyph_phantom_points(glyph).is_none()
        {
            return match face.glyph_bounding_box(glyph) {
                Some(bbox) => {
                    (if is_vertical {
                        bbox.y_max + bbox.y_min
                    } else {
                        bbox.x_max + bbox.x_min
                    }) as u32
                }
                None => 0,
            };
        }

        if is_vertical {
            if face.tables().vmtx.is_some() {
                face.glyph_ver_advance(glyph).unwrap_or(0) as u32
            } else {
                // TODO: Original code calls `h_extents_with_fallback`
                (face.ascender() - face.descender()) as u32
            }
        } else if !is_vertical && face.tables().hmtx.is_some() {
            // Like HarfBuzz `get_advance_with_var_unscaled` (vanilla-engine change).
            if face.is_variable() {
                if let Some(hvar) = face.tables().hvar {
                    let advance = face.tables().hmtx.and_then(|hmtx| hmtx.advance(glyph)).unwrap_or(0);
                    if let Some(offset) = hvar.advance_offset(glyph, &self.variation_coords) {
                        return (f32::from(advance) + offset.round()).max(0.0) as u32;
                    }
                    return u32::from(advance);
                }
            }
            face.glyph_hor_advance(glyph).unwrap_or(0) as u32
        } else {
            face.units_per_em() as u32
        }
    }

    pub(crate) fn glyph_h_origin(&self, glyph: GlyphId) -> i32 {
        self.glyph_h_advance(glyph) / 2
    }

    pub(crate) fn glyph_v_origin(&self, glyph: GlyphId) -> i32 {
        self.em_scale_y(self.glyph_v_origin_unscaled(glyph))
    }

    fn glyph_v_origin_unscaled(&self, glyph: GlyphId) -> i32 {
        match self.ttfp_face.glyph_y_origin(glyph) {
            Some(y) => i32::from(y),
            None => {
                let mut extents = hb_glyph_extents_t::default();
                if self.glyph_extents_unscaled(glyph, &mut extents) {
                    if self.ttfp_face.tables().vmtx.is_some() {
                        extents.y_bearing + self.glyph_side_bearing(glyph, true)
                    } else {
                        let advance = self.ttfp_face.ascender() - self.ttfp_face.descender();
                        let diff = advance as i32 - -extents.height;
                        extents.y_bearing + (diff >> 1)
                    }
                } else {
                    // TODO: Original code calls `h_extents_with_fallback`
                    self.ttfp_face.ascender() as i32
                }
            }
        }
    }

    pub(crate) fn glyph_side_bearing(&self, glyph: GlyphId, is_vertical: bool) -> i32 {
        let face = &self.ttfp_face;
        if face.is_variable() && face.tables().hvar.is_none() && face.tables().vvar.is_none() {
            return match face.glyph_bounding_box(glyph) {
                Some(bbox) => (if is_vertical { bbox.x_min } else { bbox.y_min }) as i32,
                None => 0,
            };
        }

        if is_vertical {
            face.glyph_ver_side_bearing(glyph).unwrap_or(0) as i32
        } else {
            face.glyph_hor_side_bearing(glyph).unwrap_or(0) as i32
        }
    }

    pub(crate) fn glyph_extents(
        &self,
        glyph: GlyphId,
        glyph_extents: &mut hb_glyph_extents_t,
    ) -> bool {
        let result = self.glyph_extents_unscaled(glyph, glyph_extents);
        // Like `hb_font_t::scale_glyph_extents`.
        let x1 = glyph_extents.x_bearing as f32 * self.x_multf;
        let y1 = glyph_extents.y_bearing as f32 * self.y_multf;
        let x2 = x1 + glyph_extents.width as f32 * self.x_multf;
        let y2 = y1 + glyph_extents.height as f32 * self.y_multf;
        glyph_extents.x_bearing = x1.floor() as i32;
        glyph_extents.y_bearing = y1.floor() as i32;
        glyph_extents.width = x2.ceil() as i32 - glyph_extents.x_bearing;
        glyph_extents.height = y2.ceil() as i32 - glyph_extents.y_bearing;
        result
    }

    fn glyph_extents_unscaled(
        &self,
        glyph: GlyphId,
        glyph_extents: &mut hb_glyph_extents_t,
    ) -> bool {
        let pixels_per_em = self.pixels_per_em.map_or(u16::MAX, |ppem| ppem.0);

        if let Some(img) = self.ttfp_face.glyph_raster_image(glyph, pixels_per_em) {
            // HarfBuzz also supports only PNG.
            if img.format == ttf_parser::RasterImageFormat::PNG {
                let scale = self.units_per_em as f32 / img.pixels_per_em as f32;
                glyph_extents.x_bearing = (f32::from(img.x) * scale).round() as i32;
                glyph_extents.y_bearing =
                    ((f32::from(img.y) + f32::from(img.height)) * scale).round() as i32;
                glyph_extents.width = (f32::from(img.width) * scale).round() as i32;
                glyph_extents.height = (-f32::from(img.height) * scale).round() as i32;
                return true;
            }
        // TODO: Add tests for this. We should use all glyphs from
        // https://github.com/googlefonts/color-fonts/blob/main/fonts/test_glyphs-glyf_colr_1_no_cliplist.ttf
        // and test their output against harfbuzz.
        } else if let Some(colr) = self.ttfp_face.tables().colr {
            if colr.is_simple() {
                return false;
            }

            if let Some(clip_box) = colr.clip_box(glyph, self.variation_coordinates()) {
                // Floor
                glyph_extents.x_bearing = (clip_box.x_min).round() as i32;
                glyph_extents.y_bearing = (clip_box.y_max).round() as i32;
                glyph_extents.width = (clip_box.x_max - clip_box.x_min).round() as i32;
                glyph_extents.height = (clip_box.y_min - clip_box.y_max).round() as i32;
                return true;
            }

            let mut extents_data = hb_paint_extents_context_t::new(&self.ttfp_face);
            let ret = colr
                .paint(
                    glyph,
                    0,
                    &mut extents_data,
                    self.variation_coordinates(),
                    RgbaColor::new(0, 0, 0, 0),
                )
                .is_some();

            let e = extents_data.get_extents();
            if e.is_void() {
                glyph_extents.x_bearing = 0;
                glyph_extents.y_bearing = 0;
                glyph_extents.width = 0;
                glyph_extents.height = 0;
            } else {
                glyph_extents.x_bearing = e.x_min as i32;
                glyph_extents.y_bearing = e.y_max as i32;
                glyph_extents.width = (e.x_max - e.x_min) as i32;
                glyph_extents.height = (e.y_min - e.y_max) as i32;
            }

            return ret;
        }

        let mut bbox = None;

        if let Some(glyf) = self.ttfp_face.tables().glyf {
            bbox = glyf.bbox(glyph);
        }

        // See https://github.com/harfbuzz/rustybuzz/pull/98#issuecomment-1948430785
        if self.ttfp_face.tables().glyf.is_some() && bbox.is_none() {
            // Empty glyph; zero extents.
            return true;
        }

        let Some(bbox) = bbox else {
            return false;
        };

        glyph_extents.x_bearing = i32::from(bbox.x_min);
        glyph_extents.y_bearing = i32::from(bbox.y_max);
        glyph_extents.width = i32::from(bbox.width());
        glyph_extents.height = i32::from(bbox.y_min - bbox.y_max);

        true
    }

    pub(crate) fn glyph_name(&self, glyph: GlyphId) -> Option<&str> {
        self.ttfp_face.glyph_name(glyph)
    }

    pub(crate) fn glyph_props(&self, glyph: GlyphId) -> u16 {
        let table = match self.tables().gdef {
            Some(v) => v,
            None => return 0,
        };

        match table.glyph_class(glyph) {
            Some(GlyphClass::Base) => GlyphPropsFlags::BASE_GLYPH.bits(),
            Some(GlyphClass::Ligature) => GlyphPropsFlags::LIGATURE.bits(),
            Some(GlyphClass::Mark) => {
                let class = table.glyph_mark_attachment_class(glyph);
                (class << 8) | GlyphPropsFlags::MARK.bits()
            }
            _ => 0,
        }
    }

    pub(crate) fn layout_table(&self, table_index: TableIndex) -> Option<&LayoutTable<'a>> {
        match table_index {
            TableIndex::GSUB => self.gsub.as_ref().map(|table| &table.inner),
            TableIndex::GPOS => self.gpos.as_ref().map(|table| &table.inner),
        }
    }

    pub(crate) fn layout_tables(
        &self,
    ) -> impl Iterator<Item = (TableIndex, &LayoutTable<'a>)> + '_ {
        TableIndex::iter().filter_map(move |idx| self.layout_table(idx).map(|table| (idx, table)))
    }
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct hb_glyph_extents_t {
    pub x_bearing: i32,
    pub y_bearing: i32,
    pub width: i32,
    pub height: i32,
}

unsafe impl bytemuck::Zeroable for hb_glyph_extents_t {}
unsafe impl bytemuck::Pod for hb_glyph_extents_t {}

fn find_best_cmap_subtable(face: &ttf_parser::Face) -> Option<u16> {
    use ttf_parser::PlatformId;

    // Symbol subtable.
    // Prefer symbol if available.
    // https://github.com/harfbuzz/harfbuzz/issues/1918
    find_cmap_subtable(face, PlatformId::Windows, WINDOWS_SYMBOL_ENCODING)
        // 32-bit subtables:
        .or_else(|| find_cmap_subtable(face, PlatformId::Windows, WINDOWS_UNICODE_FULL_ENCODING))
        .or_else(|| find_cmap_subtable(face, PlatformId::Unicode, UNICODE_FULL_ENCODING))
        .or_else(|| find_cmap_subtable(face, PlatformId::Unicode, UNICODE_2_0_FULL_ENCODING))
        // 16-bit subtables:
        .or_else(|| find_cmap_subtable(face, PlatformId::Windows, WINDOWS_UNICODE_BMP_ENCODING))
        .or_else(|| find_cmap_subtable(face, PlatformId::Unicode, UNICODE_2_0_BMP_ENCODING))
        .or_else(|| find_cmap_subtable(face, PlatformId::Unicode, UNICODE_ISO_ENCODING))
        .or_else(|| find_cmap_subtable(face, PlatformId::Unicode, UNICODE_1_1_ENCODING))
        .or_else(|| find_cmap_subtable(face, PlatformId::Unicode, UNICODE_1_0_ENCODING))
        // MacRoman subtable:
        .or_else(|| find_cmap_subtable(face, PlatformId::Macintosh, 0))
}

fn find_cmap_subtable(
    face: &ttf_parser::Face,
    platform_id: ttf_parser::PlatformId,
    encoding_id: u16,
) -> Option<u16> {
    for (i, subtable) in face.tables().cmap?.subtables.into_iter().enumerate() {
        if subtable.platform_id == platform_id && subtable.encoding_id == encoding_id {
            return Some(i as u16);
        }
    }

    None
}

#[rustfmt::skip]
static UNICODE_TO_MACROMAN: &[u16] = &[
    0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1,
    0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8,
    0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3,
    0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC,
    0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF,
    0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8,
    0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211,
    0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x03A9, 0x00E6, 0x00F8,
    0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB,
    0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153,
    0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA,
    0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02,
    0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1,
    0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC, 0x00D3, 0x00D4,
    0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC,
    0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7,
];

fn unicode_to_macroman(c: u32) -> u32 {
    let u = c as u16;
    let Some(index) = UNICODE_TO_MACROMAN.iter().position(|m| *m == u) else {
        return 0;
    };
    (0x80 + index) as u32
}

// Like HarfBuzz `hb_ot_var_normalize_coords` with an `avar` version 1 map (vanilla-engine change).
fn normalize_variation_like_harfbuzz(
    variation_axis: &ttf_parser::VariationAxis,
    value: f32,
    segment_map: Option<ttf_parser::LazyArray16<ttf_parser::avar::AxisValueMap>>,
) -> i16 {
    let default_value = variation_axis.def_value;
    let min_value = variation_axis.min_value.min(default_value);
    let max_value = variation_axis.max_value.max(default_value);
    let value = value.max(min_value).min(max_value);
    let normalized = if value == default_value {
        0.0
    } else if value < default_value {
        (value - default_value) / (default_value - min_value)
    } else {
        (value - default_value) / (max_value - default_value)
    };
    let mut fixed = (normalized * 65536.0).round() as i32;
    if let Some(segment_map) = segment_map {
        fixed = (map_avar_like_harfbuzz(segment_map, fixed as f32 / 65536.0) * 65536.0).round() as i32;
    }
    ((fixed + 2) >> 2).clamp(-16384, 16384) as i16
}

// Like HarfBuzz `SegmentMaps::map_float` (vanilla-engine change).
fn map_avar_like_harfbuzz(
    segment_map: ttf_parser::LazyArray16<ttf_parser::avar::AxisValueMap>,
    value: f32,
) -> f32 {
    let map: Vec<(f32, f32)> = segment_map
        .into_iter()
        .map(|axis_value_map| {
            (
                f32::from(axis_value_map.from_coordinate) / 16384.0,
                f32::from(axis_value_map.to_coordinate) / 16384.0,
            )
        })
        .collect();
    if map.is_empty() {
        return value;
    }
    if map.len() == 1 {
        return value - map[0].0 + map[0].1;
    }
    let mut start = 0;
    let mut end = map.len();
    if map[start].0 == -1.0 && map[start].1 == -1.0 && map[start + 1].0 == -1.0 {
        start += 1;
    }
    if map[end - 1].0 == 1.0 && map[end - 1].1 == 1.0 && map[end - 2].0 == 1.0 {
        end -= 1;
    }
    if let Some(i) = (start..end).find(|&i| value == map[i].0) {
        let mut j = i;
        while j + 1 < end && value == map[j + 1].0 {
            j += 1;
        }
        if i == j {
            return map[i].1;
        }
        if i + 2 == j {
            return map[i + 1].1;
        }
        if value < 0.0 {
            return map[j].1;
        }
        if value > 0.0 {
            return map[i].1;
        }
        return if map[i].1.abs() < map[j].1.abs() {
            map[i].1
        } else {
            map[j].1
        };
    }
    let i = (start..end).find(|&i| value < map[i].0).unwrap_or(end);
    if i == start {
        return value - map[start].0 + map[start].1;
    }
    if i == end {
        return value - map[end - 1].0 + map[end - 1].1;
    }
    let (before, after) = (map[i - 1], map[i]);
    let denom = after.0 - before.0;
    before.1 + ((after.1 - before.1) * (value - before.0)) / denom
}
