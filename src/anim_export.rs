// More export formats: WebP, an animated GIF, MP4 and WebM of the Motion timeline, and a PDF with one page per selected frame.
// The animations are the page played from 0 to its length at 30 frames a second, each frame drawn like a PNG export. GIF is written
// here; MP4 and WebM are cut by ffmpeg (found as for video fills) from a folder of PNG frames.
use std::io::Cursor;
use pdf_writer::Finish;

use egui::Rect;
use image::RgbaImage;

use crate::model::ExportFormat;
use crate::App;

pub const FPS: f32 = 30.0;
const MAX_FRAMES: usize = 900; // thirty seconds

pub fn encode_webp(png: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?.to_rgba8();
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out).encode(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(out)
}

// one PDF page per SVG, each as big as its picture
pub fn pdf_pages(svgs: &[String]) -> Result<Vec<u8>, String> {
    use std::collections::HashMap;
    use svg2pdf::usvg;
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    opt.fontdb_mut().load_font_data(include_bytes!("../assets/fonts/Inter.ttf").to_vec());
    opt.fontdb_mut().load_font_data(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").to_vec());
    let mut alloc = pdf_writer::Ref::new(1);
    let (catalog_id, tree_id) = (alloc.bump(), alloc.bump());
    let mut pdf = pdf_writer::Pdf::new();
    let mut kids = vec![];
    let mut chunks = vec![];
    for svg in svgs {
        let svg = svg.replace("mix-blend-mode:plus-lighter", "mix-blend-mode:screen").replace("mix-blend-mode:plus-darker", "mix-blend-mode:multiply");
        let tree = usvg::Tree::from_str(&svg, &opt).map_err(|e| e.to_string())?;
        let (chunk, xobj) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default()).map_err(|e| e.to_string())?;
        let mut map = HashMap::new();
        let chunk = chunk.renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));
        let xobj = map[&xobj];
        let (page_id, content_id) = (alloc.bump(), alloc.bump());
        let (w, h) = (tree.size().width(), tree.size().height());
        let name = pdf_writer::Name(b"S1");
        let mut page = pdf.page(page_id);
        page.media_box(pdf_writer::Rect::new(0.0, 0.0, w, h));
        page.parent(tree_id);
        page.contents(content_id);
        page.resources().x_objects().pair(name, xobj);
        page.finish();
        let mut content = pdf_writer::Content::new();
        content.transform([w, 0.0, 0.0, h, 0.0, 0.0]).x_object(name);
        pdf.stream(content_id, &content.finish());
        kids.push(page_id);
        chunks.push(chunk);
    }
    pdf.catalog(catalog_id).pages(tree_id);
    pdf.pages(tree_id).kids(kids.iter().copied()).count(kids.len() as i32);
    for c in &chunks {
        pdf.extend(c);
    }
    Ok(pdf.finish())
}

impl App {
    // the page played from the start to its end: every frame as an RGBA picture of the same size, the layers' union over time
    pub fn animation_frames(&mut self, ctx: &egui::Context, roots: &[u64], scale: f32) -> Result<Vec<RgbaImage>, String> {
        let len = self.anim_len().max(1.0);
        let n = ((len / 1000.0 * FPS).ceil() as usize).clamp(2, MAX_FRAMES);
        // pass one: the picture at each time and where it sits
        let base = self.shapes.clone();
        let mut svgs: Vec<(String, Rect)> = vec![];
        for k in 0..n {
            let t = len * k as f32 / (n - 1) as f32;
            self.shapes = crate::anim::evaluate(&base, t);
            if let Some(s) = self.svg_for(ctx, roots) {
                svgs.push(s);
            }
        }
        self.shapes = base;
        let Some(union) = svgs.iter().map(|s| s.1).reduce(|a, b| a.union(b)) else { return Err("nothing to export".into()) };
        let (w, h) = ((union.width() * scale).ceil().max(1.0) as u32, (union.height() * scale).ceil().max(1.0) as u32);
        // pass two: every picture drawn into a frame of the union's size
        let mut frames = vec![];
        for (svg, rect) in svgs {
            let png = crate::export::render_png(&svg, scale)?;
            let img = image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgba8();
            let mut frame = RgbaImage::new(w, h);
            let (dx, dy) = (((rect.min.x - union.min.x) * scale).round() as i64, ((rect.min.y - union.min.y) * scale).round() as i64);
            image::imageops::overlay(&mut frame, &img, dx, dy);
            frames.push(frame);
        }
        Ok(frames)
    }

    pub fn export_gif(&mut self, ctx: &egui::Context, roots: &[u64], scale: f32) -> Result<Vec<u8>, String> {
        use image::codecs::gif::{GifEncoder, Repeat};
        let frames = self.animation_frames(ctx, roots, scale)?;
        let mut out = Cursor::new(Vec::new());
        {
            let mut enc = GifEncoder::new_with_speed(&mut out, 10);
            enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
            let delay = image::Delay::from_numer_denom_ms(1000, FPS as u32);
            for f in frames {
                enc.encode_frame(image::Frame::from_parts(f, 0, 0, delay)).map_err(|e| e.to_string())?;
            }
        }
        Ok(out.into_inner())
    }

    // MP4 (H.264) or WebM (VP9) through ffmpeg
    pub fn export_video(&mut self, ctx: &egui::Context, roots: &[u64], scale: f32, webm: bool) -> Result<Vec<u8>, String> {
        let exe = crate::video::ffmpeg().ok_or("MP4 and WebM need ffmpeg: put ffmpeg next to drui, or set DRUI_FFMPEG")?.clone();
        let frames = self.animation_frames(ctx, roots, scale)?;
        let dir = std::env::temp_dir().join(format!("drui-anim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (k, f) in frames.iter().enumerate() {
            // video has no transparency: the frames go on white
            let mut flat = RgbaImage::from_pixel(f.width(), f.height(), image::Rgba([255, 255, 255, 255]));
            image::imageops::overlay(&mut flat, f, 0, 0);
            flat.save(dir.join(format!("f_{k:05}.png"))).map_err(|e| e.to_string())?;
        }
        let out = dir.join(if webm { "out.webm" } else { "out.mp4" });
        let result = crate::video::encode_frames(&exe, &dir, FPS, &out, webm).and_then(|_| std::fs::read(&out).map_err(|e| e.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    // the formats beyond PNG / SVG / PDF / Lottie; None for those
    pub fn export_extra(&mut self, ctx: &egui::Context, roots: &[u64], fmt: ExportFormat, scale: f32) -> Option<Result<Vec<u8>, String>> {
        Some(match fmt {
            ExportFormat::Webp => self.svg_for(ctx, roots).ok_or_else(|| "nothing to export".to_string()).and_then(|(svg, _)| crate::export::render_png(&svg, scale)).and_then(|png| encode_webp(&png)),
            ExportFormat::Gif => self.export_gif(ctx, roots, scale),
            ExportFormat::Mp4 => self.export_video(ctx, roots, scale, false),
            ExportFormat::Webm => self.export_video(ctx, roots, scale, true),
            _ => return None,
        })
    }
}
