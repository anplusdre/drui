// Video fills. A video is kept in the file (`Doc::videos`, base64 like images) and a fill that uses one is an image fill with a
// poster frame, so everything that draws or exports images (canvas, PNG / SVG / PDF, code exports, the browser player) shows the
// poster. In present mode the frames play, looping: the clip is cut into frames by ffmpeg in the background and the layer's
// texture is swapped for the frame at the current time.
//
// ffmpeg is not part of drui's own code: it is looked for in `DRUI_FFMPEG`, then `ffmpeg(.exe)` next to drui, then on PATH. Without
// it the Video paint type is greyed out and files with videos still open (they show their poster frames).
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
const FPS: f32 = 12.0; // frames cut out per second of video
const MAX_SECONDS: f32 = 30.0; // only this much of a long video is cut (and it loops)
const MAX_WIDTH: u32 = 720; // frames are scaled down to this width

static FFMPEG: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn ffmpeg() -> Option<&'static PathBuf> {
    FFMPEG.get_or_init(find_ffmpeg).as_ref()
}

fn find_ffmpeg() -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut candidates: Vec<PathBuf> = vec![];
    if let Some(p) = std::env::var_os("DRUI_FFMPEG") {
        candidates.push(PathBuf::from(p));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join(name));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|d| d.join(name)));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    c.stdin(Stdio::null()).stderr(Stdio::piped()).stdout(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // no console window
    }
    c
}

fn work_dir(id: u64) -> PathBuf {
    std::env::temp_dir().join(format!("drui-video-{}-{id}", std::process::id()))
}

// the first frame of a video, as PNG bytes, and its length in seconds (0 when ffmpeg does not say)
pub fn poster(bytes: &[u8], ext: &str) -> Result<(Vec<u8>, f32), String> {
    let exe = ffmpeg().ok_or("Video fills need ffmpeg: put ffmpeg next to drui, or set DRUI_FFMPEG")?;
    if bytes.len() > MAX_BYTES {
        return Err(format!("that video is over {} MB", MAX_BYTES / 1024 / 1024));
    }
    let dir = std::env::temp_dir().join(format!("drui-poster-{}-{}", std::process::id(), bytes.len()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let input = dir.join(format!("in.{}", if ext.is_empty() { "mp4" } else { ext }));
    std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
    let out = command(exe).args(["-v", "info", "-i"]).arg(&input).args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"]).output().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    if out.stdout.len() < 8 || &out.stdout[1..4] != b"PNG" {
        return Err("ffmpeg could not read that video".into());
    }
    Ok((out.stdout, duration_of(&String::from_utf8_lossy(&out.stderr))))
}

// "Duration: 00:00:05.12" in ffmpeg's log
fn duration_of(log: &str) -> f32 {
    log.split("Duration:").nth(1).and_then(|r| r.trim_start().split(',').next()).map(str::trim).and_then(|t| {
        let mut p = t.split(':');
        let (h, m, s): (f32, f32, f32) = (p.next()?.parse().ok()?, p.next()?.parse().ok()?, p.next()?.parse().ok()?);
        Some(h * 3600.0 + m * 60.0 + s)
    }).unwrap_or(0.0)
}

// MP4 (H.264) or WebM (VP9) from the PNG frames `f_00000.png ...` in `dir`, written to `out`
pub fn encode_frames(exe: &Path, dir: &Path, fps: f32, out: &Path, webm: bool) -> Result<(), String> {
    let mut cmd = command(exe);
    cmd.args(["-v", "error", "-y", "-framerate", &format!("{fps}"), "-i"]).arg(dir.join("f_%05d.png"));
    // yuv420p wants even sides
    cmd.args(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"]);
    if webm {
        cmd.args(["-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p", "-b:v", "0", "-crf", "30"]);
    } else {
        cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-movflags", "+faststart"]);
    }
    let res = cmd.arg(out).output().map_err(|e| e.to_string())?;
    if res.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&res.stderr).lines().last().unwrap_or("ffmpeg failed").to_owned()) }
}

// a video cut into frames on disk. The cutting runs in the background; `ready` counts the frames written so far.
pub struct Clip {
    dir: PathBuf,
    pub seconds: f32,
    ready: Arc<AtomicUsize>,
    pub last: Option<(usize, egui::TextureHandle)>, // the frame now shown
}

impl Clip {
    // starts cutting `bytes` (the file's content) into frames; None when ffmpeg is not there
    pub fn start(id: u64, bytes: &[u8], seconds: f32) -> Option<Clip> {
        let exe = ffmpeg()?.clone();
        let dir = work_dir(id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).ok()?;
        let input = dir.join("in.bin");
        std::fs::write(&input, bytes).ok()?;
        let ready = Arc::new(AtomicUsize::new(0));
        let (counter, out_dir) = (ready.clone(), dir.clone());
        std::thread::spawn(move || {
            let pattern = out_dir.join("f_%05d.png");
            let filter = format!("fps={FPS},scale='min({MAX_WIDTH},iw)':-2");
            let child = command(&exe).args(["-v", "error", "-y", "-t", &format!("{MAX_SECONDS}"), "-i"]).arg(&input).args(["-vf", &filter]).arg(&pattern).spawn();
            let Ok(mut child) = child else { return };
            // frames appear one after the other: count them while ffmpeg works, and once more when it has finished
            loop {
                let done = child.try_wait().ok().flatten().is_some();
                let n = std::fs::read_dir(&out_dir).map_or(0, |d| d.filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().starts_with("f_")).count());
                counter.store(if done { n } else { n.saturating_sub(1) }, Ordering::Relaxed); // the newest file may be half written
                if done {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        Some(Clip { dir, seconds, ready, last: None })
    }

    pub fn frames(&self) -> usize {
        self.ready.load(Ordering::Relaxed)
    }

    // the file of the frame to show `t` seconds into the (looping) video, once that frame exists
    pub fn frame_at(&self, t: f32) -> Option<(usize, PathBuf)> {
        let n = self.frames();
        if n == 0 {
            return None;
        }
        // the clip's own length when known and the whole of it has been cut, else however much there is
        let len = if self.seconds > 0.0 { ((self.seconds.min(MAX_SECONDS) * FPS).round() as usize).clamp(1, n) } else { n };
        let k = ((t.max(0.0) * FPS) as usize) % len;
        Some((k, self.dir.join(format!("f_{:05}.png", k + 1))))
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// Play / pause video: where a video is now when it has been paused or started by an interaction. A video with no entry plays on from
// the moment the frame appeared.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct VideoCtl {
    pub playing: bool,
    pub base: f32,  // seconds into the video when `since` began
    pub since: f64, // the clock (egui time) at that moment
}

#[cfg(test)]
mod tests {
    #[test]
    fn ffmpegs_duration_line_is_read() {
        assert_eq!(super::duration_of("  Duration: 00:00:05.12, start: 0.000000, bitrate: 1 kb/s"), 5.12);
        assert_eq!(super::duration_of("Duration: 01:02:03.50, start"), 3723.5);
        assert_eq!(super::duration_of("nothing here"), 0.0);
        assert_eq!(super::duration_of("Duration: N/A, start"), 0.0);
    }
}
