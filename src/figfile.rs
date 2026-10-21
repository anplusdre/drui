// Reading a .fig file's container: a ZIP (canvas.fig, images/<hash>, meta.json, thumbnail.png), or a bare canvas.fig. canvas.fig is
// "fig-kiwi", a version, and length-prefixed chunks: the Kiwi schema, then the message, each deflated (newer files: zstd).
use std::collections::HashMap;
use std::io::Read;

use crate::kiwi::{Schema, Val};

pub struct FigFile {
    pub message: Val,                       // the decoded `Message`: nodeChanges, blobs, ...
    pub images: HashMap<String, Vec<u8>>,   // image files by their hash (lower-case hex)
    pub version: u32,
}

fn inflate(chunk: &[u8]) -> Result<Vec<u8>, String> {
    if chunk.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let mut dec = ruzstd::decoding::StreamingDecoder::new(chunk).map_err(|e| format!("zstd: {e}"))?;
        let mut out = vec![];
        dec.read_to_end(&mut out).map_err(|e| format!("zstd: {e}"))?;
        Ok(out)
    } else {
        miniz_oxide::inflate::decompress_to_vec(chunk).map_err(|e| format!("deflate: {e:?}"))
    }
}

pub fn read(bytes: &[u8]) -> Result<FigFile, String> {
    let mut images = HashMap::new();
    let canvas: Vec<u8> = if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("not a Figma file: {e}"))?;
        let mut canvas = None;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = f.name().to_owned();
            if name == "canvas.fig" {
                let mut v = vec![];
                f.read_to_end(&mut v).map_err(|e| e.to_string())?;
                canvas = Some(v);
            } else if let Some(hash) = name.strip_prefix("images/").filter(|h| !h.is_empty()) {
                let mut v = vec![];
                f.read_to_end(&mut v).map_err(|e| e.to_string())?;
                images.insert(hash.to_lowercase(), v);
            }
        }
        canvas.ok_or("this archive has no canvas.fig")?
    } else {
        bytes.to_vec()
    };
    if canvas.starts_with(b"fig-jam") {
        return Err("FigJam files are not supported (only Figma Design files)".into());
    }
    if !canvas.starts_with(b"fig-kiwi") || canvas.len() < 12 {
        return Err("this is not a Figma file".into());
    }
    let version = u32::from_le_bytes([canvas[8], canvas[9], canvas[10], canvas[11]]);
    let mut chunks: Vec<&[u8]> = vec![];
    let mut at = 12;
    while at + 4 <= canvas.len() {
        let n = u32::from_le_bytes([canvas[at], canvas[at + 1], canvas[at + 2], canvas[at + 3]]) as usize;
        at += 4;
        let end = at.checked_add(n).filter(|&e| e <= canvas.len()).ok_or("the file is cut short")?;
        chunks.push(&canvas[at..end]);
        at = end;
    }
    let (Some(schema), Some(data)) = (chunks.first(), chunks.get(1)) else { return Err("the file has no data".into()) };
    let schema = Schema::parse(&inflate(schema)?)?;
    let message = schema.decode("Message", &inflate(data)?)?;
    Ok(FigFile { message, images, version })
}
