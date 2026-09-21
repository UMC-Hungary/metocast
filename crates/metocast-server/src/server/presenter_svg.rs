use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use zip::ZipArchive;

const DEFAULT_WIDTH_EMU: i64 = 9_144_000;
const DEFAULT_HEIGHT_EMU: i64 = 6_858_000;
const EMU_PER_PT: f64 = 12_700.0;

#[derive(Clone, Debug)]
struct Theme {
    colors: HashMap<String, String>,
    clr_map: HashMap<String, String>,
}

impl Default for Theme {
    fn default() -> Self {
        let colors = HashMap::from([
            ("dk1".to_string(), "000000".to_string()),
            ("lt1".to_string(), "FFFFFF".to_string()),
            ("dk2".to_string(), "000000".to_string()),
            ("lt2".to_string(), "919191".to_string()),
            ("accent1".to_string(), "618FFD".to_string()),
            ("accent2".to_string(), "00AE00".to_string()),
            ("accent3".to_string(), "FFFFFF".to_string()),
            ("accent4".to_string(), "000000".to_string()),
            ("accent5".to_string(), "B7C6FE".to_string()),
            ("accent6".to_string(), "009D00".to_string()),
        ]);
        let clr_map = HashMap::from([
            ("bg1".to_string(), "lt1".to_string()),
            ("tx1".to_string(), "dk1".to_string()),
            ("bg2".to_string(), "lt2".to_string()),
            ("tx2".to_string(), "dk2".to_string()),
            ("accent1".to_string(), "accent1".to_string()),
            ("accent2".to_string(), "accent2".to_string()),
            ("accent3".to_string(), "accent3".to_string()),
            ("accent4".to_string(), "accent4".to_string()),
            ("accent5".to_string(), "accent5".to_string()),
            ("accent6".to_string(), "accent6".to_string()),
        ]);
        Self { colors, clr_map }
    }
}

impl Theme {
    fn resolve_scheme(&self, key: &str) -> String {
        let mapped = self.clr_map.get(key).map(String::as_str).unwrap_or(key);
        self.colors
            .get(mapped)
            .or_else(|| self.colors.get(key))
            .cloned()
            .unwrap_or_else(|| {
                if key == "bg1" || key == "lt1" {
                    "FFFFFF".to_string()
                } else {
                    "000000".to_string()
                }
            })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

#[derive(Clone, Debug)]
struct RunStyle {
    size_pt: f64,
    fill: String,
    bold: bool,
    italic: bool,
    font_family: String,
}

impl Default for RunStyle {
    fn default() -> Self {
        Self {
            size_pt: 28.0,
            fill: "FFFFFF".to_string(),
            bold: true,
            italic: false,
            font_family: "Calibri".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
struct TextRun {
    text: String,
    style: RunStyle,
}

#[derive(Clone, Debug)]
struct TextLine {
    align: Align,
    runs: Vec<TextRun>,
}

#[derive(Clone, Copy, Debug)]
enum Align {
    Left,
    Center,
    Right,
    Justify,
}

impl Align {
    fn from_ppt(raw: &str) -> Self {
        match raw {
            "ctr" => Self::Center,
            "r" => Self::Right,
            "just" | "dist" => Self::Justify,
            _ => Self::Left,
        }
    }
}

#[derive(Clone, Debug)]
struct TextBox {
    rect: Rect,
    lines: Vec<TextLine>,
    body_anchor: BodyAnchor,
    fill: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
enum BodyAnchor {
    #[default]
    Top,
    Center,
    Bottom,
}

impl BodyAnchor {
    fn from_ppt(raw: &str) -> Self {
        match raw {
            "ctr" => Self::Center,
            "b" => Self::Bottom,
            _ => Self::Top,
        }
    }
}

#[derive(Clone, Debug)]
struct ImageBox {
    rect: Rect,
    href: String,
}

#[derive(Clone, Debug)]
enum SvgItem {
    Text(TextBox),
    Image(ImageBox),
    Rect { rect: Rect, fill: String },
}

#[derive(Clone, Debug, Default)]
struct ShapeState {
    rect: Rect,
    fill: Option<String>,
    no_fill: bool,
    lines: Vec<TextLine>,
    body_anchor: BodyAnchor,
}

#[derive(Clone, Debug, Default)]
struct PicState {
    rect: Rect,
    embed_id: Option<String>,
}

#[derive(Debug)]
pub struct InlineSvgDeck {
    pub slides: Vec<InlineSvgSlide>,
    pub width_emu: u64,
    pub height_emu: u64,
}

#[derive(Debug)]
pub struct InlineSvgSlide {
    pub index: u32,
    pub svg: String,
    pub width_px: u32,
    pub height_px: u32,
}

pub fn convert_pptx_to_inline_svg(
    input: &Path,
) -> Result<InlineSvgDeck, Box<dyn std::error::Error>> {
    let file = fs::File::open(input)?;
    let mut archive = ZipArchive::new(file)?;

    let mut theme = Theme::default();
    if let Some(bytes) = read_zip_entry_optional(&mut archive, "ppt/theme/theme1.xml")? {
        theme.colors.extend(parse_theme_colors(&bytes)?);
    }
    if let Some(bytes) = read_zip_entry_optional(&mut archive, "ppt/slideMasters/slideMaster1.xml")?
    {
        theme.clr_map.extend(parse_clr_map(&bytes)?);
    }

    let background = read_zip_entry_optional(&mut archive, "ppt/slideMasters/slideMaster1.xml")?
        .and_then(|bytes| parse_background(&bytes, &theme).ok().flatten())
        .unwrap_or_else(|| "000000".to_string());
    eprintln!(
        "[svg-gen] theme: dk1={:?} lt1={:?} tx1-resolves={:?} bg=#{background}",
        theme.colors.get("dk1"),
        theme.colors.get("lt1"),
        theme.resolve_scheme("tx1"),
    );

    let (width, height) = read_zip_entry_optional(&mut archive, "ppt/presentation.xml")?
        .map(|bytes| parse_slide_size(&bytes).unwrap_or((DEFAULT_WIDTH_EMU, DEFAULT_HEIGHT_EMU)))
        .unwrap_or((DEFAULT_WIDTH_EMU, DEFAULT_HEIGHT_EMU));
    let (width_px, height_px) = svg_pixel_size(width, height);
    let media_data_uris = read_media_data_uris(&mut archive)?;
    let slide_names = slide_entry_names(&mut archive)?;

    let flatten_options = build_flatten_options();

    let mut slides = Vec::with_capacity(slide_names.len());
    for (idx, slide_name) in slide_names.iter().enumerate() {
        let slide_xml = read_zip_entry(&mut archive, slide_name)?;
        let rels_name = slide_name.replace("ppt/slides/", "ppt/slides/_rels/") + ".rels";
        let rels = read_zip_entry_optional(&mut archive, &rels_name)?
            .map(|bytes| parse_relationships(&bytes).unwrap_or_default())
            .unwrap_or_default();
        let items = parse_slide(&slide_xml, &rels, &theme, Some(&media_data_uris))?;
        let text_svg = render_svg(width, height, &background, &items);
        slides.push(InlineSvgSlide {
            index: idx as u32 + 1,
            svg: flatten_text_to_paths(&text_svg, &flatten_options),
            width_px,
            height_px,
        });
    }

    Ok(InlineSvgDeck {
        slides,
        width_emu: width.max(1) as u64,
        height_emu: height.max(1) as u64,
    })
}
fn read_media_data_uris(
    archive: &mut ZipArchive<fs::File>,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut media = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if !name.starts_with("ppt/media/") || name.ends_with('/') {
            continue;
        }

        let Some(mime) = mime_for_zip_path(&name) else {
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        media.insert(
            name,
            format!("data:{mime};base64,{}", BASE64_STANDARD.encode(bytes)),
        );
    }
    Ok(media)
}

fn mime_for_zip_path(path: &str) -> Option<&'static str> {
    match Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("bmp") => Some("image/bmp"),
        Some("webp") => Some("image/webp"),
        Some("svg") => Some("image/svg+xml"),
        _ => None,
    }
}
fn slide_entry_names(
    archive: &mut ZipArchive<fs::File>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name();
        if name.starts_with("ppt/slides/")
            && name.ends_with(".xml")
            && !name.contains("_rels")
            && slide_number(name).is_some()
        {
            names.push(name.to_string());
        }
    }
    names.sort_by_key(|name| slide_number(name).unwrap_or(0));
    Ok(names)
}

fn slide_number(name: &str) -> Option<u32> {
    Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("slide"))
        .and_then(|n| n.parse::<u32>().ok())
}

fn read_zip_entry(
    archive: &mut ZipArchive<fs::File>,
    name: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut entry = archive.by_name(name)?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_zip_entry_optional(
    archive: &mut ZipArchive<fs::File>,
    name: &str,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    match archive.by_name(name) {
        Ok(mut entry) => {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn parse_slide_size(xml: &[u8]) -> Result<(i64, i64), Box<dyn std::error::Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) | Event::Empty(e) if local_name(e.name().into_inner()) == b"sldSz" => {
                let cx = attr_i64(&e, b"cx").unwrap_or(DEFAULT_WIDTH_EMU);
                let cy = attr_i64(&e, b"cy").unwrap_or(DEFAULT_HEIGHT_EMU);
                return Ok((cx, cy));
            }
            Event::Eof => return Ok((DEFAULT_WIDTH_EMU, DEFAULT_HEIGHT_EMU)),
            _ => {}
        }
        buf.clear();
    }
}

fn parse_theme_colors(xml: &[u8]) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut current_slot: Option<String> = None;
    let mut colors = HashMap::new();
    let mut in_main_scheme = false;
    let mut saw_main_scheme = false;
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let name = local_name(e.name().into_inner());
                if name == b"clrScheme" && !saw_main_scheme {
                    in_main_scheme = true;
                    saw_main_scheme = true;
                } else if in_main_scheme && is_theme_color_slot(name) {
                    current_slot = Some(String::from_utf8_lossy(name).to_string());
                } else if in_main_scheme && name == b"srgbClr" {
                    if let (Some(slot), Some(color)) =
                        (current_slot.as_ref(), attr_value(&e, b"val"))
                    {
                        colors.insert(slot.clone(), color);
                    }
                } else if in_main_scheme
                    && name == b"sysClr"
                    && let (Some(slot), Some(color)) =
                        (current_slot.as_ref(), attr_value(&e, b"lastClr"))
                {
                    colors.insert(slot.clone(), color);
                }
            }
            Event::Empty(e) => {
                let name = local_name(e.name().into_inner());
                if in_main_scheme && name == b"srgbClr" {
                    if let (Some(slot), Some(color)) =
                        (current_slot.as_ref(), attr_value(&e, b"val"))
                    {
                        colors.insert(slot.clone(), color);
                    }
                } else if in_main_scheme
                    && name == b"sysClr"
                    && let (Some(slot), Some(color)) =
                        (current_slot.as_ref(), attr_value(&e, b"lastClr"))
                {
                    colors.insert(slot.clone(), color);
                }
            }
            Event::End(e) => {
                let name = local_name(e.name().into_inner());
                if name == b"clrScheme" && in_main_scheme {
                    in_main_scheme = false;
                    current_slot = None;
                } else if current_slot
                    .as_deref()
                    .is_some_and(|slot| name == slot.as_bytes())
                {
                    current_slot = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(colors)
}

fn is_theme_color_slot(name: &[u8]) -> bool {
    matches!(
        name,
        b"dk1"
            | b"lt1"
            | b"dk2"
            | b"lt2"
            | b"accent1"
            | b"accent2"
            | b"accent3"
            | b"accent4"
            | b"accent5"
            | b"accent6"
            | b"hlink"
            | b"folHlink"
    )
}

fn parse_clr_map(xml: &[u8]) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) | Event::Empty(e) if local_name(e.name().into_inner()) == b"clrMap" => {
                let mut map = HashMap::new();
                for key in [
                    b"bg1".as_slice(),
                    b"tx1",
                    b"bg2",
                    b"tx2",
                    b"accent1",
                    b"accent2",
                    b"accent3",
                    b"accent4",
                    b"accent5",
                    b"accent6",
                    b"hlink",
                    b"folHlink",
                ] {
                    if let Some(value) = attr_value(&e, key) {
                        map.insert(String::from_utf8_lossy(key).to_string(), value);
                    }
                }
                return Ok(map);
            }
            Event::Eof => return Ok(HashMap::new()),
            _ => {}
        }
        buf.clear();
    }
}

fn parse_background(
    xml: &[u8],
    theme: &Theme,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut in_bg = false;
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let name = local_name(e.name().into_inner());
                if name == b"bg" {
                    in_bg = true;
                } else if in_bg && name == b"srgbClr" {
                    return Ok(attr_value(&e, b"val"));
                } else if in_bg && name == b"sysClr" {
                    return Ok(attr_value(&e, b"lastClr"));
                } else if in_bg
                    && name == b"schemeClr"
                    && let Some(value) = attr_value(&e, b"val")
                {
                    return Ok(Some(theme.resolve_scheme(&value)));
                }
            }
            Event::Empty(e) => {
                let name = local_name(e.name().into_inner());
                if in_bg && name == b"srgbClr" {
                    return Ok(attr_value(&e, b"val"));
                } else if in_bg && name == b"sysClr" {
                    return Ok(attr_value(&e, b"lastClr"));
                } else if in_bg
                    && name == b"schemeClr"
                    && let Some(value) = attr_value(&e, b"val")
                {
                    return Ok(Some(theme.resolve_scheme(&value)));
                }
            }
            Event::End(e) if local_name(e.name().into_inner()) == b"bg" => {
                in_bg = false;
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
        buf.clear();
    }
}

fn parse_relationships(xml: &[u8]) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut rels = HashMap::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) | Event::Empty(e)
                if local_name(e.name().into_inner()) == b"Relationship" =>
            {
                if let (Some(id), Some(target)) = (attr_value(&e, b"Id"), attr_value(&e, b"Target"))
                {
                    rels.insert(id, target);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(rels)
}

fn parse_slide(
    xml: &[u8],
    rels: &HashMap<String, String>,
    theme: &Theme,
    media_data_uris: Option<&HashMap<String, String>>,
) -> Result<Vec<SvgItem>, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut items = Vec::new();

    let mut shape: Option<ShapeState> = None;
    let mut pic: Option<PicState> = None;
    let mut in_text = false;
    let mut in_rpr = false;
    let mut in_text_value = false;
    let mut text_value = String::new();
    let mut current_style = RunStyle::default();
    let mut editing_style: Option<RunStyle> = None;
    let mut para_align = Align::Left;
    let mut line_runs: Vec<TextRun> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => {
                let name = local_name(e.name().into_inner());
                match name {
                    b"sp" => shape = Some(ShapeState::default()),
                    b"pic" => pic = Some(PicState::default()),
                    b"txBody" => in_text = true,
                    b"bodyPr" if in_text => {
                        if let Some(anchor) = attr_value(&e, b"anchor")
                            && let Some(s) = shape.as_mut()
                        {
                            s.body_anchor = BodyAnchor::from_ppt(&anchor);
                        }
                    }
                    b"p" if in_text => {
                        para_align = Align::Left;
                        line_runs.clear();
                    }
                    b"br" if in_text => {
                        push_line(&mut shape, para_align, &mut line_runs);
                    }
                    b"pPr" if in_text => {
                        if let Some(algn) = attr_value(&e, b"algn") {
                            para_align = Align::from_ppt(&algn);
                        }
                    }
                    b"rPr" if in_text => {
                        in_rpr = true;
                        let mut style = current_style.clone();
                        if let Some(sz) = attr_value(&e, b"sz").and_then(|v| v.parse::<f64>().ok())
                        {
                            style.size_pt = sz / 100.0;
                        }
                        if let Some(bold) = attr_value(&e, b"b") {
                            style.bold = bold == "1" || bold.eq_ignore_ascii_case("true");
                        }
                        if let Some(italic) = attr_value(&e, b"i") {
                            style.italic = italic == "1" || italic.eq_ignore_ascii_case("true");
                        }
                        editing_style = Some(style);
                    }
                    b"t" if in_text => {
                        in_text_value = true;
                        text_value.clear();
                    }
                    _ => handle_common_start(
                        &e,
                        name,
                        &mut shape,
                        &mut pic,
                        (in_text, in_rpr),
                        &mut editing_style,
                        theme,
                    ),
                }
            }
            Event::Empty(e) => {
                let name = local_name(e.name().into_inner());
                match name {
                    b"bodyPr" if in_text => {
                        if let Some(anchor) = attr_value(&e, b"anchor")
                            && let Some(s) = shape.as_mut()
                        {
                            s.body_anchor = BodyAnchor::from_ppt(&anchor);
                        }
                    }
                    b"pPr" if in_text => {
                        if let Some(algn) = attr_value(&e, b"algn") {
                            para_align = Align::from_ppt(&algn);
                        }
                    }
                    b"br" if in_text => {
                        push_line(&mut shape, para_align, &mut line_runs);
                    }
                    b"rPr" if in_text => {
                        let mut style = current_style.clone();
                        if let Some(sz) = attr_value(&e, b"sz").and_then(|v| v.parse::<f64>().ok())
                        {
                            style.size_pt = sz / 100.0;
                        }
                        if let Some(bold) = attr_value(&e, b"b") {
                            style.bold = bold == "1" || bold.eq_ignore_ascii_case("true");
                        }
                        if let Some(italic) = attr_value(&e, b"i") {
                            style.italic = italic == "1" || italic.eq_ignore_ascii_case("true");
                        }
                        current_style = style;
                    }
                    _ => handle_common_start(
                        &e,
                        name,
                        &mut shape,
                        &mut pic,
                        (in_text, in_rpr),
                        &mut editing_style,
                        theme,
                    ),
                }
            }
            Event::Text(e) if in_text_value => {
                let decoded = e.decode()?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| decoded.into_owned());
                text_value.push_str(&unescaped);
            }
            Event::End(e) => {
                let name = local_name(e.name().into_inner());
                match name {
                    b"t" => {
                        if !text_value.is_empty() {
                            line_runs.push(TextRun {
                                text: text_value.clone(),
                                style: current_style.clone(),
                            });
                        }
                        in_text_value = false;
                        text_value.clear();
                    }
                    b"rPr" if in_rpr => {
                        if let Some(style) = editing_style.take() {
                            current_style = style;
                        }
                        in_rpr = false;
                    }
                    b"p" if in_text => {
                        push_line(&mut shape, para_align, &mut line_runs);
                    }
                    b"txBody" => in_text = false,
                    b"sp" => {
                        if let Some(s) = shape.take() {
                            if !s.no_fill
                                && let Some(fill) = s.fill.clone()
                            {
                                items.push(SvgItem::Rect { rect: s.rect, fill });
                            }
                            if !s.lines.is_empty() {
                                items.push(SvgItem::Text(TextBox {
                                    rect: s.rect,
                                    lines: s.lines,
                                    body_anchor: s.body_anchor,
                                    fill: if s.no_fill { None } else { s.fill },
                                }));
                            }
                        }
                    }
                    b"pic" => {
                        if let Some(p) = pic.take()
                            && let Some(id) = p.embed_id
                            && let Some(target) = rels.get(&id)
                            && let Some(href) =
                                image_href_for_target("ppt/slides", target, media_data_uris)
                        {
                            items.push(SvgItem::Image(ImageBox { rect: p.rect, href }));
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(items)
}

fn handle_common_start(
    e: &BytesStart<'_>,
    name: &[u8],
    shape: &mut Option<ShapeState>,
    pic: &mut Option<PicState>,
    text_context: (bool, bool),
    editing_style: &mut Option<RunStyle>,
    theme: &Theme,
) {
    let (in_text, in_rpr) = text_context;
    match name {
        b"off" => {
            let x = attr_i64(e, b"x").unwrap_or(0);
            let y = attr_i64(e, b"y").unwrap_or(0);
            if let Some(s) = shape.as_mut() {
                s.rect.x = x;
                s.rect.y = y;
            }
            if let Some(p) = pic.as_mut() {
                p.rect.x = x;
                p.rect.y = y;
            }
        }
        b"ext" => {
            let w = attr_i64(e, b"cx").unwrap_or(0);
            let h = attr_i64(e, b"cy").unwrap_or(0);
            if let Some(s) = shape.as_mut() {
                s.rect.w = w;
                s.rect.h = h;
            }
            if let Some(p) = pic.as_mut() {
                p.rect.w = w;
                p.rect.h = h;
            }
        }
        b"noFill" if shape.is_some() && !in_text => {
            if let Some(s) = shape.as_mut() {
                s.no_fill = true;
                s.fill = None;
            }
        }
        b"srgbClr" => {
            if let Some(color) = attr_value(e, b"val") {
                if in_rpr {
                    if let Some(style) = editing_style.as_mut() {
                        style.fill = color;
                    }
                } else if !in_text && let Some(s) = shape.as_mut() {
                    s.fill = Some(color);
                }
            }
        }
        b"schemeClr" => {
            if let Some(value) = attr_value(e, b"val") {
                let color = theme.resolve_scheme(&value);
                if in_rpr {
                    if let Some(style) = editing_style.as_mut() {
                        style.fill = color;
                    }
                } else if !in_text && let Some(s) = shape.as_mut() {
                    s.fill = Some(color);
                }
            }
        }
        b"latin" | b"cs" | b"ea" if in_rpr => {
            if let Some(typeface) = attr_value(e, b"typeface")
                && !typeface.starts_with('+')
                && !typeface.is_empty()
                && let Some(style) = editing_style.as_mut()
            {
                style.font_family = typeface;
            }
        }
        b"blip" => {
            if let Some(embed) = attr_value(e, b"embed")
                && let Some(p) = pic.as_mut()
            {
                p.embed_id = Some(embed);
            }
        }
        _ => {}
    }
}

fn push_line(shape: &mut Option<ShapeState>, align: Align, runs: &mut Vec<TextRun>) {
    if runs.iter().any(|run| !run.text.trim().is_empty()) {
        if let Some(s) = shape.as_mut() {
            s.lines.push(TextLine {
                align,
                runs: std::mem::take(runs),
            });
        }
    } else {
        runs.clear();
    }
}

fn image_href_for_target(
    base_dir: &str,
    target: &str,
    media_data_uris: Option<&HashMap<String, String>>,
) -> Option<String> {
    if target.starts_with("http://") || target.starts_with("https://") {
        return None;
    }

    let resolved = resolve_zip_path(base_dir, target);
    if let Some(media) = media_data_uris {
        return media.get(&resolved).cloned();
    }

    Path::new(target)
        .file_name()
        .map(|file_name| format!("../media/{}", escape_attr(&file_name.to_string_lossy())))
}

fn build_flatten_options() -> usvg::Options<'static> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    options.font_family = "Helvetica".to_string();
    options
}

fn flatten_text_to_paths(svg: &str, options: &usvg::Options) -> String {
    match usvg::Tree::from_data(svg.as_bytes(), options) {
        Ok(tree) => tree.to_string(&usvg::WriteOptions::default()),
        Err(err) => {
            eprintln!("[svg-gen] text-to-path flatten failed, sending text SVG: {err}");
            svg.to_string()
        }
    }
}

fn render_svg(width: i64, height: i64, background: &str, items: &[SvgItem]) -> String {
    let (pixel_width, pixel_height) = svg_pixel_size(width, height);
    let sx = pixel_width as f64 / width.max(1) as f64;
    let sy = pixel_height as f64 / height.max(1) as f64;
    let mut svg = String::new();
    svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    svg.push_str(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" ",
    );
    svg.push_str(&format!(
        "viewBox=\"0 0 {pixel_width} {pixel_height}\" width=\"{pixel_width}\" height=\"{pixel_height}\">\n"
    ));
    svg.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{pixel_width}\" height=\"{pixel_height}\" fill=\"#{}\"/>\n",
        escape_attr(background)
    ));
    for item in items {
        match item {
            SvgItem::Rect { rect, fill } => render_rect(&mut svg, rect, fill, sx, sy),
            SvgItem::Image(image) => render_image(&mut svg, image, sx, sy),
            SvgItem::Text(text) => render_text_box(&mut svg, text, sx, sy),
        }
    }
    svg.push_str("</svg>\n");
    svg
}

fn svg_pixel_size(width: i64, height: i64) -> (u32, u32) {
    let max_side = 1280.0;
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    if w >= h {
        let pixel_width = max_side;
        let pixel_height = (max_side * h / w).round().max(1.0);
        (pixel_width as u32, pixel_height as u32)
    } else {
        let pixel_height = max_side;
        let pixel_width = (max_side * w / h).round().max(1.0);
        (pixel_width as u32, pixel_height as u32)
    }
}

fn render_rect(svg: &mut String, rect: &Rect, fill: &str, sx: f64, sy: f64) {
    if rect.w <= 0 || rect.h <= 0 {
        return;
    }
    svg.push_str(&format!(
        "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#{}\"/>\n",
        rect.x as f64 * sx,
        rect.y as f64 * sy,
        rect.w as f64 * sx,
        rect.h as f64 * sy,
        escape_attr(fill)
    ));
}

fn render_image(svg: &mut String, image: &ImageBox, sx: f64, sy: f64) {
    if image.rect.w <= 0 || image.rect.h <= 0 {
        return;
    }
    svg.push_str(&format!(
        "<image x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" href=\"{}\" xlink:href=\"{}\" preserveAspectRatio=\"none\"/>\n",
        image.rect.x as f64 * sx,
        image.rect.y as f64 * sy,
        image.rect.w as f64 * sx,
        image.rect.h as f64 * sy,
        image.href,
        image.href
    ));
}

fn render_text_box(svg: &mut String, text: &TextBox, sx: f64, sy: f64) {
    if text.rect.w <= 0 || text.rect.h <= 0 || text.lines.is_empty() {
        return;
    }
    if let Some(fill) = &text.fill {
        render_rect(svg, &text.rect, fill, sx, sy);
    }

    let rect_x = text.rect.x as f64 * sx;
    let rect_y = text.rect.y as f64 * sy;
    let rect_w = text.rect.w as f64 * sx;
    let rect_h = text.rect.h as f64 * sy;
    let lines = wrap_text_lines(&text.lines, rect_w * 0.98, sy);
    let mut line_heights = lines
        .iter()
        .map(|line| {
            line.runs
                .iter()
                .map(|run| run.style.size_pt * EMU_PER_PT * sy * 1.18)
                .fold(28.0 * EMU_PER_PT * sy * 1.18, f64::max)
        })
        .collect::<Vec<_>>();
    let total_h: f64 = line_heights.iter().sum();
    let max_line_w = lines
        .iter()
        .map(|line| estimate_line_width(line, sy, 1.0))
        .fold(0.0, f64::max);
    let available_h = if lines.len() > text.lines.len() {
        rect_h * 0.82
    } else {
        rect_h
    };
    let fit_scale = [
        if total_h > available_h && total_h > 0.0 {
            (available_h / total_h) * 0.96
        } else {
            1.0
        },
        if max_line_w > rect_w && max_line_w > 0.0 {
            (rect_w / max_line_w) * 0.98
        } else {
            1.0
        },
    ]
    .into_iter()
    .fold(1.0, f64::min)
    .clamp(0.45, 1.0);
    for line_h in &mut line_heights {
        *line_h *= fit_scale;
    }
    let total_h: f64 = line_heights.iter().sum();
    let mut y = match text.body_anchor {
        BodyAnchor::Top => rect_y + line_heights.first().copied().unwrap_or(0.0),
        BodyAnchor::Center => {
            rect_y + (rect_h - total_h) / 2.0 + line_heights.first().copied().unwrap_or(0.0)
        }
        BodyAnchor::Bottom => {
            rect_y + rect_h - total_h + line_heights.first().copied().unwrap_or(0.0)
        }
    };

    for (line, line_h) in lines.iter().zip(line_heights) {
        let (x, anchor) = match line.align {
            Align::Left | Align::Justify => (rect_x, "start"),
            Align::Center => (rect_x + rect_w / 2.0, "middle"),
            Align::Right => (rect_x + rect_w, "end"),
        };
        svg.push_str(&format!(
            "<text x=\"{:.3}\" y=\"{:.3}\" text-anchor=\"{}\" dominant-baseline=\"alphabetic\">",
            x, y, anchor
        ));
        for run in &line.runs {
            let size = run.style.size_pt * EMU_PER_PT * sy * fit_scale;
            let weight = if run.style.bold { "700" } else { "400" };
            let style = if run.style.italic { "italic" } else { "normal" };
            svg.push_str(&format!(
                "<tspan font-family=\"{}\" font-size=\"{:.3}\" font-weight=\"{}\" font-style=\"{}\" fill=\"#{}\">{}</tspan>",
                escape_attr(&run.style.font_family),
                size,
                weight,
                style,
                escape_attr(&run.style.fill),
                escape_text(&run.text)
            ));
        }
        svg.push_str("</text>\n");
        y += line_h;
    }
}

fn wrap_text_lines(lines: &[TextLine], max_width: f64, sy: f64) -> Vec<TextLine> {
    let mut wrapped = Vec::new();
    for line in lines {
        let mut current = TextLine {
            align: line.align,
            runs: Vec::new(),
        };
        let mut current_w = 0.0;

        for run in &line.runs {
            for segment in text_segments(&run.text) {
                let segment = if current.runs.is_empty() {
                    segment.trim_start().to_string()
                } else {
                    segment
                };
                if segment.is_empty() {
                    continue;
                }

                let mut segment_run = run.clone();
                segment_run.text = segment;
                let segment_w = estimate_run_width(&segment_run, sy, 1.0);
                if current_w > 0.0 && current_w + segment_w > max_width {
                    trim_line_end(&mut current);
                    if !current.runs.is_empty() {
                        wrapped.push(current);
                    }
                    current = TextLine {
                        align: line.align,
                        runs: Vec::new(),
                    };
                    current_w = 0.0;
                    segment_run.text = segment_run.text.trim_start().to_string();
                }

                current_w += estimate_run_width(&segment_run, sy, 1.0);
                current.runs.push(segment_run);
            }
        }

        trim_line_end(&mut current);
        if !current.runs.is_empty() {
            wrapped.push(current);
        }
    }

    if wrapped.is_empty() {
        lines.to_vec()
    } else {
        wrapped
    }
}

fn text_segments(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if ch.is_whitespace() {
            segments.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

fn trim_line_end(line: &mut TextLine) {
    while let Some(last) = line.runs.last_mut() {
        let trimmed = last.text.trim_end().to_string();
        if trimmed.is_empty() {
            line.runs.pop();
        } else {
            last.text = trimmed;
            break;
        }
    }
}

fn estimate_line_width(line: &TextLine, sy: f64, scale: f64) -> f64 {
    line.runs
        .iter()
        .map(|run| estimate_run_width(run, sy, scale))
        .sum()
}

fn estimate_run_width(run: &TextRun, sy: f64, scale: f64) -> f64 {
    let size = run.style.size_pt * EMU_PER_PT * sy * scale;
    let bold_extra = if run.style.bold { 0.03 } else { 0.0 };
    run.text
        .chars()
        .map(|ch| char_width_factor(ch) + bold_extra)
        .sum::<f64>()
        * size
}

fn char_width_factor(ch: char) -> f64 {
    if ch.is_whitespace() {
        0.32
    } else if matches!(
        ch,
        'i' | 'l' | 'I' | '!' | '|' | ':' | ';' | '.' | ',' | '\''
    ) {
        0.30
    } else if matches!(ch, 'm' | 'w' | 'M' | 'W') {
        0.82
    } else if ch.is_uppercase() {
        0.66
    } else {
        0.55
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    name.iter()
        .position(|b| *b == b':')
        .map(|idx| &name[idx + 1..])
        .unwrap_or(name)
}

fn attr_value(e: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if local_name(attr.key.into_inner()) == name {
            return Some(String::from_utf8_lossy(attr.value.as_ref()).to_string());
        }
    }
    None
}

fn attr_i64(e: &BytesStart<'_>, name: &[u8]) -> Option<i64> {
    attr_value(e, name).and_then(|v| v.parse::<i64>().ok())
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(value: &str) -> String {
    escape_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn resolve_zip_path(base_dir: &str, target: &str) -> String {
    let mut parts = PathBuf::from(base_dir);
    parts.push(target);
    let mut normalized = PathBuf::new();
    for component in parts.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            _ => {}
        }
    }
    normalized.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_placeholder_end_paragraph_fill_does_not_render_shape() {
        let slide = br#"
            <p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                   xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
              <p:cSld>
                <p:spTree>
                  <p:sp>
                    <p:nvSpPr>
                      <p:cNvPr id="2" name="Cim 1"/>
                      <p:cNvSpPr/>
                      <p:nvPr><p:ph type="title"/></p:nvPr>
                    </p:nvSpPr>
                    <p:spPr>
                      <a:xfrm>
                        <a:off x="0" y="0"/>
                        <a:ext cx="8929718" cy="3933056"/>
                      </a:xfrm>
                    </p:spPr>
                    <p:txBody>
                      <a:bodyPr anchor="t"/>
                      <a:lstStyle/>
                      <a:p>
                        <a:pPr algn="l"/>
                        <a:endParaRPr lang="hu-HU" b="1">
                          <a:solidFill><a:schemeClr val="bg1"/></a:solidFill>
                        </a:endParaRPr>
                      </a:p>
                    </p:txBody>
                  </p:sp>
                </p:spTree>
              </p:cSld>
            </p:sld>
        "#;

        let items = parse_slide(slide, &HashMap::new(), &Theme::default(), None).unwrap();

        assert!(items.is_empty(), "{items:#?}");
    }
}
