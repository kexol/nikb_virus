#[derive(Clone, Debug)]
pub struct WindowState {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub home_x: f64,
    pub home_y: f64,
    pub target_x: f64,
    pub target_y: f64,
    pub phase: f64,
    pub speed: f64,
    pub dir_x: f64,
    pub w: i32,
    pub h: i32,
    pub tick: u64,
}

impl WindowState {
    pub fn new(x: f64, y: f64, w: i32, h: i32, speed: f64) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            home_x: x,
            home_y: y,
            target_x: 0.0,
            target_y: 0.0,
            phase: 0.0,
            speed,
            dir_x: 1.0,
            w,
            h,
            tick: 0,
        }
    }
}
