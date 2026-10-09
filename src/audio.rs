use crate::assets::SOUND_BYTES;
use rand::Rng;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::cell::RefCell;
use std::io::Cursor;

pub struct AudioManager {
    current_sink: RefCell<Option<Sink>>,
    handle: Option<OutputStreamHandle>,

    _stream: Option<OutputStream>,
}

impl AudioManager {
    pub fn new() -> Self {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let res = std::panic::catch_unwind(|| OutputStream::try_default());
        std::panic::set_hook(prev_hook);
        match res {
            Ok(Ok((stream, handle))) => Self {
                current_sink: RefCell::new(None),
                handle: Some(handle),
                _stream: Some(stream),
            },
            _ => Self {
                current_sink: RefCell::new(None),
                handle: None,
                _stream: None,
            },
        }
    }

    pub fn is_playing(&self) -> bool {
        if let Some(sink) = self.current_sink.borrow().as_ref() {
            !sink.empty()
        } else {
            false
        }
    }

    pub fn play_random<R: Rng + ?Sized>(&self, rng: &mut R) {
        let Some(handle) = &self.handle else { return };

        if self.is_playing() {
            return;
        }

        let idx = rng.gen_range(0..SOUND_BYTES.len());
        let data = SOUND_BYTES[idx];
        let cursor = Cursor::new(data);
        if let Ok(source) = Decoder::new(cursor) {
            let pitch = 0.9 + rng.gen::<f32>() * 0.25;
            if let Ok(sink) = Sink::try_new(handle) {
                sink.append(source.speed(pitch));
                *self.current_sink.borrow_mut() = Some(sink);
            }
        }
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        if let Some(sink) = self.current_sink.borrow_mut().take() {
            sink.stop();
        }
    }
}
