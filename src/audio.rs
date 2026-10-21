// Audio on the Motion timeline: a sound file (wav, mp3, ogg, flac, aac) is kept in the page, drawn as a waveform in the timeline,
// and plays with the animation (Play, scrubbing, present mode). Playback goes through rodio; with no sound device the animation
// still plays and the status line says so.
use std::io::Cursor;

use base64::Engine;
use rodio::Source;

use crate::model::AudioClip;
use crate::App;

pub const PEAKS: usize = 600;

// a decoded clip's loudness in `PEAKS` slices, and its length in ms
pub fn peaks_of(bytes: &[u8]) -> Option<(Vec<f32>, f32)> {
    let dec = rodio::Decoder::try_from(Cursor::new(bytes.to_vec())).ok()?;
    let (channels, rate) = (dec.channels().get() as usize, dec.sample_rate().get() as f32);
    let samples: Vec<f32> = dec.collect();
    if samples.is_empty() || channels == 0 {
        return None;
    }
    let frames = samples.len() / channels;
    let ms = frames as f32 / rate * 1000.0;
    let per = (frames as f32 / PEAKS as f32).max(1.0);
    let mut peaks = vec![0.0f32; PEAKS];
    for (f, chunk) in samples.chunks(channels).enumerate() {
        let k = ((f as f32 / per) as usize).min(PEAKS - 1);
        let m = chunk.iter().fold(0.0f32, |a, s| a.max(s.abs()));
        peaks[k] = peaks[k].max(m);
    }
    Some((peaks, ms))
}

impl App {
    fn clip_bytes(clip: &AudioClip) -> Option<Vec<u8>> {
        base64::engine::general_purpose::STANDARD.decode(&clip.data).ok()
    }

    // "Add audio" in the timeline bar: pick a file and put it at the start of the page's timeline
    pub fn add_audio(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("audio", &["wav", "mp3", "ogg", "flac", "m4a", "aac"]).pick_file() else { return };
        let Ok(bytes) = std::fs::read(&path) else { return };
        let name = path.file_stem().map_or_else(|| "audio".to_owned(), |n| n.to_string_lossy().into_owned());
        self.add_audio_bytes(&name, &bytes);
    }

    pub fn add_audio_bytes(&mut self, name: &str, bytes: &[u8]) -> bool {
        let Some((peaks, ms)) = peaks_of(bytes) else {
            self.status = format!("can't read the sound {name}");
            return false;
        };
        let id = self.new_id();
        self.audio_peaks.insert(id, (peaks, ms));
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        self.pages[self.cur].audio.push(AudioClip { id, name: name.to_owned(), data, start: 0.0, gain: 1.0 });
        // the timeline is at least as long as the sound
        let page = &mut self.pages[self.cur];
        page.anim_ms = page.anim_ms.max(ms.ceil());
        true
    }

    // the clip's waveform, decoded the first time it is asked for
    pub fn clip_peaks(&mut self, id: u64) -> Option<&(Vec<f32>, f32)> {
        if !self.audio_peaks.contains_key(&id) {
            let clip = self.pages[self.cur].audio.iter().find(|c| c.id == id)?;
            let peaks = peaks_of(&Self::clip_bytes(clip)?)?;
            self.audio_peaks.insert(id, peaks);
        }
        self.audio_peaks.get(&id)
    }

    // starts every clip of the page, `from` ms into the timeline (a clip that begins later is delayed, one that began earlier is joined)
    pub fn audio_play(&mut self, from: f32) {
        self.audio_stop();
        if self.pages[self.cur].audio.is_empty() {
            return;
        }
        let Ok(sink) = rodio::DeviceSinkBuilder::open_default_sink() else {
            self.status = "no sound device: the animation plays without sound".into();
            return;
        };
        let mut players: Vec<rodio::Player> = vec![];
        for clip in &self.pages[self.cur].audio {
            let player = rodio::Player::connect_new(sink.mixer());
            let Some(bytes) = Self::clip_bytes(clip) else { continue };
            let Ok(src) = rodio::Decoder::try_from(Cursor::new(bytes)) else { continue };
            let src = src.amplify(clip.gain.max(0.0));
            let at = clip.start - from;
            if at >= 0.0 {
                player.append(src.delay(std::time::Duration::from_secs_f32(at / 1000.0)));
            } else {
                player.append(src.skip_duration(std::time::Duration::from_secs_f32(-at / 1000.0)));
            }
            player.play();
            players.push(player);
        }
        if !players.is_empty() {
            self.audio_out = Some((sink, players));
        }
    }

    pub fn audio_stop(&mut self) {
        if let Some((_, players)) = self.audio_out.take() {
            players.iter().for_each(|p| p.stop());
        }
    }
}
