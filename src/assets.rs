pub static SOUND_BYTES: [&[u8]; 6] = [
    include_bytes!("../assets/sounds/nikbvirus1.ogg"),
    include_bytes!("../assets/sounds/nikbvirus2.ogg"),
    include_bytes!("../assets/sounds/nikbvirus3.ogg"),
    include_bytes!("../assets/sounds/nikbvirus4.ogg"),
    include_bytes!("../assets/sounds/nikbvirus5.ogg"),
    include_bytes!("../assets/sounds/nikbvirus6.ogg"),
];

pub static FACE_BYTES: &[u8] = include_bytes!("../assets/textures/nikb_head.png");
pub static RED_WARNING_BYTES: &[u8] = include_bytes!("../assets/textures/red_warning.png");

#[derive(Clone)]
pub struct DecodedImage {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}

impl DecodedImage {
    pub fn load_png(bytes: &[u8]) -> Result<Self, image::ImageError> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for pixel in img.pixels() {
            bgra.push(pixel[2]);
            bgra.push(pixel[1]);
            bgra.push(pixel[0]);
            bgra.push(pixel[3]);
        }
        Ok(Self {
            width: w as i32,
            height: h as i32,
            bgra,
        })
    }

    pub fn load_png_scaled_blended(
        bytes: &[u8],
        target_w: u32,
        target_h: u32,
        bg_rgb: (u8, u8, u8),
    ) -> Result<Self, image::ImageError> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let resized = image::imageops::resize(
            &img,
            target_w,
            target_h,
            image::imageops::FilterType::Lanczos3,
        );
        let mut bgra = Vec::with_capacity((target_w * target_h * 4) as usize);
        for pixel in resized.pixels() {
            let a = pixel[3] as f32 / 255.0;
            let inv_a = 1.0 - a;
            let r = (pixel[0] as f32 * a + bg_rgb.0 as f32 * inv_a).round() as u8;
            let g = (pixel[1] as f32 * a + bg_rgb.1 as f32 * inv_a).round() as u8;
            let b = (pixel[2] as f32 * a + bg_rgb.2 as f32 * inv_a).round() as u8;
            bgra.push(b);
            bgra.push(g);
            bgra.push(r);
            bgra.push(255);
        }
        Ok(Self {
            width: target_w as i32,
            height: target_h as i32,
            bgra,
        })
    }
}
