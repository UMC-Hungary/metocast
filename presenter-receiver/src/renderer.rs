use cairo::{Context, Format, ImageSurface};
use pango::{Alignment, FontDescription, WrapMode};
use pangocairo::functions as pc;
use resvg::{
    tiny_skia::{Pixmap, Transform},
    usvg,
};
use std::f64::consts::PI;

use crate::ws::PresenterTheme;

const FONT: &str = "Helvetica Neue";
const BG: (f64, f64, f64) = (0.05, 0.05, 0.08);
const FG: (f64, f64, f64) = (1.0, 1.0, 1.0);
const ACCENT: (f64, f64, f64) = (0.3, 0.3, 0.8);
const COUNTER_FONT_HEIGHT_RATIO: f64 = 0.030;
const COUNTER_FONT_MIN_SIZE: i32 = 18;
const COUNTER_FONT_MAX_SIZE: i32 = 54;
const COUNTER_BOTTOM_RATIO: f64 = 0.92;

type Paragraph<'a> = (&'a str, &'a str, f64);

/// Render slide paragraphs into a pixel buffer sized to the given display dimensions.
///
/// Each entry in `paragraphs` is `(text, align, font_size_pt)` where `align` is one of
/// `"left"`, `"center"`, `"right"`, or `"justify"` — matching the server's
/// `ParagraphContent.align` field, and `font_size_pt` is the font size from the PPTX
/// (0 when unknown / legacy format).
///
/// If the last paragraph is center-aligned and its `font_size_pt` is less than 85% of
/// the largest size in the slide, it is treated as a slide counter and rendered at the
/// bottom of the safe area instead of being part of the main vertically-centred block.
///
/// All proportions (padding, accent line, font size) scale with `width`/`height`
/// so the result looks identical whether the display is 720p, 1080p, or 4K.
///
/// Returns raw RGB24 bytes: `width × height × 3` bytes, row-major.
pub fn render_slide(
    paragraphs: &[(&str, &str, f64)],
    theme: PresenterTheme,
    current_slide: u32,
    total_slides: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    if theme == PresenterTheme::Editorial {
        return render_editorial_slide(paragraphs, current_slide, total_slides, width, height);
    }
    let (sw, sh) = (width as i32, height as i32);
    let mut surface = ImageSurface::create(Format::Rgb24, sw, sh).unwrap();
    let ctx = Context::new(&surface).unwrap();

    let w = width as f64;
    let h = height as f64;
    let pad_x = w * 0.10;

    // ── Background ───────────────────────────────────────────────────────────
    ctx.set_source_rgb(BG.0, BG.1, BG.2);
    ctx.paint().unwrap();

    // ── Top accent line — thickness scales with height ───────────────────────
    ctx.set_source_rgb(ACCENT.0, ACCENT.1, ACCENT.2);
    ctx.set_line_width((h * 0.006).max(2.0));
    ctx.move_to(pad_x, h * 0.06);
    ctx.line_to(w - pad_x, h * 0.06);
    ctx.stroke().unwrap();

    // ── Detect counter paragraph ─────────────────────────────────────────────
    // The counter (slide number / verse ref) is center-aligned and its
    // font_size_pt is < 85 % of the max on the slide.  In some PPTXes the
    // counter text box appears first in the XML, in others it is last — so we
    // check both ends.
    let non_empty: Vec<Paragraph<'_>> = paragraphs
        .iter()
        .copied()
        .filter(|(t, _, _)| !t.is_empty())
        .collect();

    let max_pt = non_empty
        .iter()
        .map(|(_, _, pt)| *pt)
        .fold(0.0f64, f64::max);
    let is_counter = |p: Paragraph<'_>| -> bool {
        max_pt > 0.0 && p.2 > 0.0 && p.2 < max_pt * 0.85 && p.1 == "center"
    };
    let (main_paras, counter_para): (Vec<Paragraph<'_>>, Option<Paragraph<'_>>) =
        if non_empty.len() >= 2 {
            let first = *non_empty.first().unwrap();
            let last = *non_empty.last().unwrap();
            if is_counter(first) {
                (non_empty[1..].to_vec(), Some(first))
            } else if is_counter(last) {
                (non_empty[..non_empty.len() - 1].to_vec(), Some(last))
            } else {
                (non_empty.clone(), None)
            }
        } else {
            (non_empty.clone(), None)
        };

    // ── Text — binary search for the largest bold size that fits ─────────────
    let max_w = (w - pad_x * 2.0) as i32;
    // Reserve space at the bottom when a counter is present.
    let main_max_h = if counter_para.is_some() {
        (h * 0.68) as i32
    } else {
        (h * 0.80) as i32
    };

    if !main_paras.is_empty() {
        let wrap_main = counter_para.is_some();
        let make_layouts = |font_size: i32| -> Vec<pango::Layout> {
            main_paras
                .iter()
                .map(|(text, align, _)| {
                    let layout = pc::create_layout(&ctx);
                    layout.set_font_description(Some(&FontDescription::from_string(&format!(
                        "{FONT} Bold {font_size}"
                    ))));
                    layout.set_alignment(parse_alignment(align));
                    if wrap_main {
                        layout.set_width(max_w * pango::SCALE);
                        layout.set_wrap(WrapMode::WordChar);
                    }
                    layout.set_text(text);
                    layout
                })
                .collect()
        };

        let fits = |layouts: &[pango::Layout], font_size: i32| -> bool {
            let gap = (font_size as f64 * 0.50) as i32;
            let text_h: i32 = layouts.iter().map(|l| l.pixel_size().1).sum();
            let gaps = gap * (layouts.len().saturating_sub(1) as i32);
            let max_line_w = layouts.iter().map(|l| l.pixel_size().0).max().unwrap_or(0);
            text_h + gaps <= main_max_h && max_line_w <= max_w
        };

        let mut lo = 8i32;
        let mut hi = (h * 0.40) as i32;

        while lo < hi - 1 {
            let mid = (lo + hi) / 2;
            if fits(&make_layouts(mid), mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }

        let layouts = make_layouts(lo);
        let gap = (lo as f64 * 0.50) as i32;
        let text_h: i32 = layouts.iter().map(|l| l.pixel_size().1).sum();
        let used_h = text_h + gap * (layouts.len().saturating_sub(1) as i32);

        // Vertically centre the main block in its safe area.
        let safe_top = h * 0.10;
        let safe_h = main_max_h as f64;
        let mut y = safe_top + (safe_h - used_h as f64) / 2.0;

        ctx.set_source_rgb(FG.0, FG.1, FG.2);
        for (index, layout) in layouts.iter().enumerate() {
            let line_width = layout.pixel_size().0 as f64;
            let align = main_paras[index].1;
            let x = if wrap_main || align == "left" || align == "justify" {
                pad_x
            } else if align == "right" {
                pad_x + max_w as f64 - line_width
            } else {
                pad_x + (max_w as f64 - line_width) / 2.0
            };
            ctx.move_to(x, y);
            pc::show_layout(&ctx, layout);
            y += layout.pixel_size().1 as f64 + gap as f64;
        }
    }

    // ── Counter paragraph at the bottom ──────────────────────────────────────
    if let Some((text, _align, _counter_pt)) = counter_para {
        let counter_size = ((h * COUNTER_FONT_HEIGHT_RATIO).round() as i32)
            .clamp(COUNTER_FONT_MIN_SIZE, COUNTER_FONT_MAX_SIZE);
        let layout = pc::create_layout(&ctx);
        layout.set_font_description(Some(&FontDescription::from_string(&format!(
            "{FONT} Bold {counter_size}"
        ))));
        layout.set_width(max_w * pango::SCALE);
        layout.set_alignment(Alignment::Center);
        layout.set_wrap(WrapMode::WordChar);
        layout.set_text(text);
        let (_, lh) = layout.pixel_size();
        let counter_x = pad_x;
        let counter_y = h * COUNTER_BOTTOM_RATIO - lh as f64;
        ctx.set_source_rgb(FG.0, FG.1, FG.2);
        ctx.move_to(counter_x, counter_y);
        pc::show_layout(&ctx, &layout);
    }

    // ── Extract RGB24 bytes ──────────────────────────────────────────────────
    drop(ctx); // must be dropped before surface.data()
    surface.flush();

    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let mut out = Vec::with_capacity((sw * sh * 3) as usize);
    for row in 0..sh as usize {
        for col in 0..sw as usize {
            let i = row * stride + col * 4;
            // Cairo RGB24: 0x00RRGGBB in native byte order → [B, G, R, X]
            out.push(data[i + 2]); // R
            out.push(data[i + 1]); // G
            out.push(data[i]); // B
        }
    }
    out
}

fn render_editorial_slide(
    paragraphs: &[(&str, &str, f64)],
    current_slide: u32,
    total_slides: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let (sw, sh) = (width as i32, height as i32);
    let mut surface = ImageSurface::create(Format::Rgb24, sw, sh).unwrap();
    let ctx = Context::new(&surface).unwrap();
    let (w, h) = (width as f64, height as f64);

    ctx.set_source_rgb(0.067, 0.063, 0.055);
    ctx.paint().unwrap();

    let non_empty: Vec<(&str, &str, f64)> = paragraphs
        .iter()
        .copied()
        .filter(|(text, _, _)| !text.trim().is_empty())
        .collect();
    if non_empty.is_empty() {
        drop(ctx);
        surface.flush();
        return surface_to_rgb(&mut surface, sw, sh);
    }

    ctx.set_source_rgba(0.81, 0.67, 0.41, 0.035);
    ctx.move_to(w * 0.68, 0.0);
    ctx.line_to(w * 0.86, 0.0);
    ctx.line_to(w * 0.63, h);
    ctx.line_to(w * 0.47, h);
    ctx.close_path();
    ctx.fill().unwrap();

    let max_pt = non_empty
        .iter()
        .map(|(_, _, pt)| *pt)
        .fold(0.0f64, f64::max);
    let is_counter = |paragraph: (&str, &str, f64)| {
        max_pt > 0.0 && paragraph.2 > 0.0 && paragraph.2 < max_pt * 0.85 && paragraph.1 == "center"
    };
    let counter = non_empty
        .first()
        .copied()
        .filter(|paragraph| is_counter(*paragraph))
        .or_else(|| {
            non_empty
                .last()
                .copied()
                .filter(|paragraph| is_counter(*paragraph))
        });
    let main: Vec<&str> = non_empty
        .iter()
        .filter(|paragraph| counter != Some(**paragraph))
        .map(|(text, _, _)| *text)
        .collect();
    let is_bible = counter
        .map(|(text, _, _)| text.starts_with("Textus") || text.starts_with("Lekció"))
        .unwrap_or(false);

    if is_bible {
        let label = counter
            .map(|(text, _, _)| clean_bible_label(text))
            .unwrap_or_default();
        show_text(
            &ctx,
            &label.to_uppercase(),
            "monospace",
            (h * 0.018).max(14.0) as i32,
            (w * 0.31, h * 0.27, w * 0.58),
            Alignment::Left,
            (0.81, 0.67, 0.41),
        );

        let number = counter
            .and_then(|(text, _, _)| bible_verse_number(text))
            .unwrap_or_default();
        show_text(
            &ctx,
            &number,
            "serif",
            (h * 0.18) as i32,
            (w * 0.13, h * 0.36, w * 0.13),
            Alignment::Right,
            (0.81, 0.67, 0.41),
        );

        let text = main.join("\n\n");
        let size = fitted_font_size(&ctx, &text, "serif", w * 0.56, h * 0.48, h * 0.095, true);
        show_text(
            &ctx,
            &text,
            "serif",
            size,
            (w * 0.31, h * 0.34, w * 0.56),
            Alignment::Left,
            (0.957, 0.937, 0.89),
        );
    } else {
        let is_title = current_slide == 1;
        show_text(
            &ctx,
            "ÉNEK",
            "monospace",
            (h * 0.018).max(14.0) as i32,
            (w * 0.1, h * 0.14, w * 0.8),
            Alignment::Center,
            (0.81, 0.67, 0.41),
        );

        let text = main.join("\n\n");
        let preferred = if is_title { h * 0.12 } else { h * 0.095 };
        let size = fitted_font_size(&ctx, &text, "serif", w * 0.82, h * 0.54, preferred, false);
        let layout = text_layout(
            &ctx,
            &text,
            "serif",
            size,
            w * 0.82,
            Alignment::Center,
            false,
        );
        let (text_width, text_height) = layout.pixel_size();
        ctx.set_source_rgb(0.957, 0.937, 0.89);
        ctx.move_to(
            (w - text_width as f64) / 2.0,
            (h - text_height as f64) / 2.0,
        );
        pc::show_layout(&ctx, &layout);

        if !is_title {
            show_text(
                &ctx,
                &format!("{current_slide}/{total_slides} DIA"),
                "monospace",
                (h * 0.015).max(12.0) as i32,
                (w * 0.1, h * 0.82, w * 0.8),
                Alignment::Center,
                (0.81, 0.67, 0.41),
            );
        }
    }

    show_text(
        &ctx,
        "METOCAST",
        "monospace",
        (h * 0.013).max(11.0) as i32,
        (w * 0.78, h * 0.91, w * 0.17),
        Alignment::Right,
        (0.42, 0.41, 0.38),
    );

    drop(ctx);
    surface.flush();
    surface_to_rgb(&mut surface, sw, sh)
}

fn text_layout(
    ctx: &Context,
    text: &str,
    family: &str,
    size: i32,
    width: f64,
    alignment: Alignment,
    wrap: bool,
) -> pango::Layout {
    let layout = pc::create_layout(ctx);
    layout.set_font_description(Some(&FontDescription::from_string(&format!(
        "{family} {size}"
    ))));
    if wrap {
        layout.set_width(width as i32 * pango::SCALE);
        layout.set_wrap(WrapMode::WordChar);
    }
    layout.set_alignment(alignment);
    layout.set_spacing((size as f64 * 0.12) as i32 * pango::SCALE);
    layout.set_text(text);
    layout
}

fn fitted_font_size(
    ctx: &Context,
    text: &str,
    family: &str,
    width: f64,
    height: f64,
    preferred: f64,
    wrap: bool,
) -> i32 {
    let mut size = preferred.max(18.0) as i32;
    while size > 18 {
        let layout = text_layout(ctx, text, family, size, width, Alignment::Left, wrap);
        let (layout_width, layout_height) = layout.pixel_size();
        if layout_width as f64 <= width && layout_height as f64 <= height {
            break;
        }
        size -= 2;
    }
    size
}

fn show_text(
    ctx: &Context,
    text: &str,
    family: &str,
    size: i32,
    position: (f64, f64, f64),
    alignment: Alignment,
    color: (f64, f64, f64),
) {
    let (x, y, width) = position;
    let layout = text_layout(ctx, text, family, size, width, alignment, true);
    ctx.set_source_rgb(color.0, color.1, color.2);
    ctx.move_to(x, y);
    pc::show_layout(ctx, &layout);
}

fn clean_bible_label(counter: &str) -> String {
    let without_page = counter
        .rsplit_once(" (")
        .map(|(label, _)| label)
        .unwrap_or(counter);
    without_page.replace('|', "·")
}

fn bible_verse_number(counter: &str) -> Option<String> {
    counter
        .split_whitespace()
        .filter_map(|token| {
            let (_, verse) = token.split_once(':')?;
            let number: String = verse
                .chars()
                .take_while(|char| char.is_ascii_digit())
                .collect();
            (!number.is_empty()).then_some(number)
        })
        .next_back()
}

fn surface_to_rgb(surface: &mut ImageSurface, width: i32, height: i32) -> Vec<u8> {
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let mut out = Vec::with_capacity((width * height * 3) as usize);
    for row in 0..height as usize {
        for col in 0..width as usize {
            let index = row * stride + col * 4;
            out.push(data[index + 2]);
            out.push(data[index + 1]);
            out.push(data[index]);
        }
    }
    out
}

/// Render a self-contained SVG slide into a framebuffer-sized 0x00RRGGBB frame.
pub fn render_svg_slide(svg: &str, width: u32, height: u32) -> Result<Vec<u32>, String> {
    eprintln!(
        "[svg] parsing {} bytes for {}x{} display",
        svg.len(),
        width,
        height
    );
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    eprintln!("[svg] fontdb has {} faces", options.fontdb.len());

    eprintln!("[svg] content: {}", &svg[..svg.len().min(600)]);
    let tree = usvg::Tree::from_data(svg.as_bytes(), &options)
        .map_err(|err| format!("failed to parse SVG: {err}"))?;
    let svg_size = tree.size();
    let scale = (width as f32 / svg_size.width()).min(height as f32 / svg_size.height());
    let render_width = ((svg_size.width() * scale).round().max(1.0) as u32).min(width);
    let render_height = ((svg_size.height() * scale).round().max(1.0) as u32).min(height);
    eprintln!(
        "[svg] SVG {}x{} → display scale={:.3} → render {}x{}",
        svg_size.width(),
        svg_size.height(),
        scale,
        render_width,
        render_height
    );

    let mut pixmap =
        Pixmap::new(render_width, render_height).ok_or("failed to allocate SVG pixmap")?;
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let mut frame = vec![0u32; (width * height) as usize];
    let offset_x = ((width - render_width) / 2) as usize;
    let offset_y = ((height - render_height) / 2) as usize;
    let frame_width = width as usize;

    // pixmap.data() is premultiplied RGBA. Compositing over a black background
    // means premultiplied RGB values are the correct output directly.
    for (row, chunk) in pixmap
        .data()
        .chunks_exact((render_width * 4) as usize)
        .enumerate()
    {
        for (col, rgba) in chunk.as_chunks::<4>().0.iter().enumerate() {
            let r = rgba[0] as u32;
            let g = rgba[1] as u32;
            let b = rgba[2] as u32;
            frame[(offset_y + row) * frame_width + offset_x + col] = (r << 16) | (g << 8) | b;
        }
    }

    Ok(frame)
}

/// Composite a small status indicator (coloured dot + optional version text)
/// onto the top-right corner of `frame` using alpha blending.
///
/// - Green  = Connected (also shows the binary version)
/// - Orange = Connecting / Reconnecting
/// - Red    = Failed (5+ consecutive connection errors)
pub fn draw_status_overlay(
    frame: &mut [u32],
    width: u32,
    height: u32,
    state: crate::ConnectionState,
) {
    let h = height as f64;

    let dot_r = (h * 0.013).max(9.0);
    let padding = dot_r * 0.9;
    let font_size = ((dot_r * 1.05) as i32).max(10);

    let (dr, dg, db) = match state {
        crate::ConnectionState::Connected => (0.0f64, 0.78, 0.38),
        crate::ConnectionState::Connecting | crate::ConnectionState::Reconnecting => {
            (1.0, 0.55, 0.0)
        }
        crate::ConnectionState::Failed => (1.0, 0.22, 0.22),
    };

    // Version label shown only when connected.
    let version_text: Option<String> = if matches!(state, crate::ConnectionState::Connected) {
        Some(format!("v{}", env!("CARGO_PKG_VERSION")))
    } else {
        None
    };

    // Measure version text width using a throw-away surface.
    let text_w = if let Some(ref text) = version_text {
        let tmp = ImageSurface::create(Format::ARgb32, 1, 1).unwrap();
        let tmp_ctx = Context::new(&tmp).unwrap();
        let layout = pc::create_layout(&tmp_ctx);
        layout.set_font_description(Some(&FontDescription::from_string(&format!(
            "{FONT} {font_size}"
        ))));
        layout.set_text(text);
        layout.pixel_size().0 as f64
    } else {
        0.0
    };

    let gap = dot_r * 0.65;
    let content_w = dot_r * 2.0 + if text_w > 0.0 { gap + text_w } else { 0.0 };
    let pill_w = content_w + padding * 2.0;
    let pill_h = dot_r * 2.0 + padding * 2.0;
    let ow = (pill_w + padding).ceil() as i32 + 2;
    let oh = (pill_h + padding).ceil() as i32 + 2;

    let mut surface = ImageSurface::create(Format::ARgb32, ow, oh).unwrap();
    let ctx = Context::new(&surface).unwrap();

    // Transparent background.
    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    ctx.paint().unwrap();

    // Semi-transparent pill behind the indicator.
    let px = padding / 2.0;
    let py = padding / 2.0;
    let radius = pill_h / 2.0;
    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    ctx.new_path();
    ctx.arc(px + radius, py + radius, radius, PI, 3.0 * PI / 2.0);
    ctx.arc(px + pill_w - radius, py + radius, radius, -PI / 2.0, 0.0);
    ctx.arc(
        px + pill_w - radius,
        py + pill_h - radius,
        radius,
        0.0,
        PI / 2.0,
    );
    ctx.arc(px + radius, py + pill_h - radius, radius, PI / 2.0, PI);
    ctx.close_path();
    ctx.fill().unwrap();

    // Dot sits on the right side of the pill.
    let dot_cx = px + pill_w - padding - dot_r;
    let dot_cy = py + pill_h / 2.0;

    // Version text to the left of the dot.
    if let Some(ref text) = version_text {
        let layout = pc::create_layout(&ctx);
        layout.set_font_description(Some(&FontDescription::from_string(&format!(
            "{FONT} {font_size}"
        ))));
        layout.set_text(text);
        let text_h = layout.pixel_size().1 as f64;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.9);
        ctx.move_to(dot_cx - dot_r - gap - text_w, dot_cy - text_h / 2.0);
        pc::show_layout(&ctx, &layout);
    }

    // Filled dot.
    ctx.set_source_rgb(dr, dg, db);
    ctx.arc(dot_cx, dot_cy, dot_r, 0.0, 2.0 * PI);
    ctx.fill().unwrap();

    drop(ctx);
    surface.flush();

    // Alpha-composite overlay onto the frame (top-right corner).
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let frame_w = width as usize;
    let frame_h = height as usize;
    let start_x = (frame_w as i32 - ow).max(0) as usize;

    for row in 0..oh as usize {
        for col in 0..ow as usize {
            let fx = start_x + col;
            let fy = row;
            if fx >= frame_w || fy >= frame_h {
                continue;
            }
            let i = row * stride + col * 4;
            // Cairo ARgb32 (little-endian): B G R A
            let a = data[i + 3] as f64 / 255.0;
            if a < 0.01 {
                continue;
            }
            let sr = data[i + 2] as f64;
            let sg = data[i + 1] as f64;
            let sb = data[i] as f64;
            let dst = frame[fy * frame_w + fx];
            let dr_dst = ((dst >> 16) & 0xff) as f64;
            let dg_dst = ((dst >> 8) & 0xff) as f64;
            let db_dst = (dst & 0xff) as f64;
            let inv = 1.0 - a;
            let r = (sr * a + dr_dst * inv) as u32;
            let g = (sg * a + dg_dst * inv) as u32;
            let b = (sb * a + db_dst * inv) as u32;
            frame[fy * frame_w + fx] = (r << 16) | (g << 8) | b;
        }
    }
}

fn parse_alignment(align: &str) -> Alignment {
    match align {
        "center" => Alignment::Center,
        "right" => Alignment::Right,
        _ => Alignment::Left, // "left", "justify", or anything unknown
    }
}

/// Convert RGB24 bytes → 0x00RRGGBB u32 per pixel (minifb format).
pub fn rgb_to_u32(rgb: &[u8]) -> Vec<u32> {
    rgb.as_chunks::<3>()
        .0
        .iter()
        .map(|p| ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editorial_theme_renders_bible_text_and_extracts_the_verse() {
        let paragraphs = [
            (
                "Nincs tehát most már semmiféle kárhoztató ítélet.",
                "left",
                38.0,
            ),
            ("Textus Róma 8:1 | 8:1 (1/1)", "center", 18.0),
        ];

        let frame = render_slide(&paragraphs, PresenterTheme::Editorial, 1, 1, 640, 360);

        assert_eq!(frame.len(), 640 * 360 * 3);
        assert_eq!(bible_verse_number(paragraphs[1].0).as_deref(), Some("1"));
        assert!(frame.as_chunks::<3>().0.iter().any(|pixel| pixel[0] > 150));
    }

    #[test]
    fn svg_white_text_on_black_produces_white_pixels() {
        // This SVG matches exactly what the server generates: black background rect,
        // white text in the center. If resvg renders correctly, the center of the
        // frame must contain white (or near-white) pixels.
        let svg = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1280 720" width="1280" height="720">
<rect x="0" y="0" width="1280" height="720" fill="#000000"/>
<text x="640" y="360" text-anchor="middle" dominant-baseline="alphabetic">
  <tspan font-family="Calibri" font-size="100" font-weight="700" font-style="normal" fill="#FFFFFF">Hello</tspan>
</text>
</svg>"##;

        let frame = render_svg_slide(svg, 1920, 1080).expect("render_svg_slide failed");

        assert_eq!(frame.len(), 1920 * 1080);

        let non_zero = frame.iter().filter(|&&p| p != 0).count();
        println!("non-zero pixels: {}/{}", non_zero, frame.len());

        // The center pixel should be non-zero (white text region or at least
        // the background rect should produce opaque pixels).
        // We check a broader region around center to account for font variation.
        let any_white_near_center = (400..600).any(|y| {
            (800..1100).any(|x| {
                let p = frame[y * 1920 + x];
                let r = (p >> 16) & 0xff;
                r > 200
            })
        });

        assert!(
            any_white_near_center,
            "expected white text near center but frame has {} non-zero pixels total",
            non_zero
        );
    }
}
