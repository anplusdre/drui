// Colour profile (File > Color profile): sRGB or Display P3. The numbers in the file are the same either way; the profile says what
// they mean. A Display P3 file exports PNGs that carry the Display P3 ICC profile (so a colour-managed viewer shows the wider
// gamut) and gives Dev Mode's CSS as `color(display-p3 ...)`.
use std::sync::atomic::{AtomicBool, Ordering};

static P3: AtomicBool = AtomicBool::new(false);

pub fn set_p3(on: bool) {
    P3.store(on, Ordering::Relaxed);
}

pub fn is_p3() -> bool {
    P3.load(Ordering::Relaxed)
}

fn fixed(v: f64) -> [u8; 4] {
    ((v * 65536.0).round() as i32).to_be_bytes()
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    for v in [x, y, z] {
        t.extend(fixed(v));
    }
    t
}

fn text_tag(s: &str) -> Vec<u8> {
    let mut t = b"text\0\0\0\0".to_vec();
    t.extend(s.as_bytes());
    t.push(0);
    t
}

fn desc_tag(s: &str) -> Vec<u8> {
    let mut t = b"desc\0\0\0\0".to_vec();
    t.extend(((s.len() + 1) as u32).to_be_bytes());
    t.extend(s.as_bytes());
    t.push(0);
    t.extend([0u8; 4 + 4 + 2 + 1 + 67]); // no Unicode or Macintosh description
    t
}

// the sRGB transfer curve (which Display P3 shares) as a 1024 entry table
fn curve_tag() -> Vec<u8> {
    let mut t = b"curv\0\0\0\0".to_vec();
    t.extend(1024u32.to_be_bytes());
    for i in 0..1024 {
        let v = i as f64 / 1023.0;
        let lin = if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        t.extend(((lin * 65535.0).round() as u16).to_be_bytes());
    }
    t
}

// a matrix / TRC ICC v2 profile for Display P3 (D65 white, colorants adapted to the D50 profile connection space)
pub fn display_p3_profile() -> Vec<u8> {
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc_tag("Display P3")),
        (b"cprt", text_tag("No copyright, use freely")),
        (b"wtpt", xyz_tag(0.9642, 1.0, 0.8249)),
        (b"rXYZ", xyz_tag(0.5151, 0.2412, -0.0011)),
        (b"gXYZ", xyz_tag(0.2920, 0.6922, 0.0419)),
        (b"bXYZ", xyz_tag(0.1571, 0.0666, 0.7841)),
        (b"rTRC", curve_tag()),
    ];
    // the three curves are one table
    let table_len = 4 + 12 * (tags.len() + 2);
    let mut body: Vec<u8> = vec![];
    let mut dir: Vec<([u8; 4], usize, usize)> = vec![];
    for (sig, data) in &tags {
        while body.len() % 4 != 0 {
            body.push(0);
        }
        dir.push((**sig, 128 + table_len + body.len(), data.len()));
        body.extend(data);
    }
    let trc = dir.last().copied().unwrap();
    dir.push((*b"gTRC", trc.1, trc.2));
    dir.push((*b"bTRC", trc.1, trc.2));
    while body.len() % 4 != 0 {
        body.push(0);
    }
    let mut out = vec![0u8; 128];
    out.extend((dir.len() as u32).to_be_bytes());
    for (sig, off, len) in &dir {
        out.extend(sig);
        out.extend((*off as u32).to_be_bytes());
        out.extend((*len as u32).to_be_bytes());
    }
    out.extend(body);
    let size = out.len() as u32;
    out[0..4].copy_from_slice(&size.to_be_bytes());
    out[8..12].copy_from_slice(&0x0240_0000u32.to_be_bytes()); // version 2.4
    out[12..16].copy_from_slice(b"mntr");
    out[16..20].copy_from_slice(b"RGB ");
    out[20..24].copy_from_slice(b"XYZ ");
    out[24..26].copy_from_slice(&2024u16.to_be_bytes());
    out[26..28].copy_from_slice(&1u16.to_be_bytes());
    out[28..30].copy_from_slice(&1u16.to_be_bytes());
    out[36..40].copy_from_slice(b"acsp");
    out[64..68].copy_from_slice(&1u32.to_be_bytes()); // relative colorimetric
    out[68..72].copy_from_slice(&fixed(0.9642));
    out[72..76].copy_from_slice(&fixed(1.0));
    out[76..80].copy_from_slice(&fixed(0.8249));
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, t) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    !data.iter().fold(!0u32, |c, &b| table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8))
}

// `png` with an iCCP chunk (the profile, zlib-compressed) right after its header
pub fn embed_profile(png: Vec<u8>, name: &str, profile: &[u8]) -> Vec<u8> {
    const SIG: usize = 8;
    if png.len() < SIG + 25 || &png[1..4] != b"PNG" {
        return png;
    }
    let mut data = name.as_bytes().to_vec();
    data.extend([0, 0]); // end of name, compression method 0 (zlib)
    data.extend(miniz_oxide::deflate::compress_to_vec_zlib(profile, 6));
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    let mut typed = b"iCCP".to_vec();
    typed.extend(&data);
    chunk.extend(&typed);
    chunk.extend(crc32(&typed).to_be_bytes());
    let at = SIG + 25; // signature, then the 25 bytes of the IHDR chunk
    let mut out = png[..at].to_vec();
    out.extend(chunk);
    out.extend(&png[at..]);
    out
}

// a PNG as the file's colour profile wants it
pub fn tag_png_in(png: Vec<u8>, p3: bool) -> Vec<u8> {
    if p3 { embed_profile(png, "Display P3", &display_p3_profile()) } else { png }
}
