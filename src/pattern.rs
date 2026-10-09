use crate::state::WindowState;
use crate::window::{get_monitors, MonitorRect};
use rand::Rng;
use std::f64::consts::PI;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementPattern {
    Shake,
    Wander,
    Bounce,
    Orbit,
    Spiral,
    Zigzag,
    Dash,
    Jumpcut,
    FleeCursor,
    StalkCursor,
}

fn get_cursor_pos() -> (f64, f64) {
    unsafe {
        let mut pt: POINT = std::mem::zeroed();
        GetCursorPos(&mut pt);
        (pt.x as f64, pt.y as f64)
    }
}

fn in_any_monitor(px: f64, py: f64, monitors: &[MonitorRect]) -> bool {
    for m in monitors {
        if px >= m.x as f64
            && px < (m.x + m.w) as f64
            && py >= m.y as f64
            && py < (m.y + m.h) as f64
        {
            return true;
        }
    }
    false
}

fn desktop_bounds(monitors: &[MonitorRect]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for m in monitors {
        if (m.x as f64) < min_x {
            min_x = m.x as f64;
        }
        if ((m.x + m.w) as f64) > max_x {
            max_x = (m.x + m.w) as f64;
        }
        if (m.y as f64) < min_y {
            min_y = m.y as f64;
        }
        if ((m.y + m.h) as f64) > max_y {
            max_y = (m.y + m.h) as f64;
        }
    }
    (min_x, max_x, min_y, max_y)
}

fn clamp_to_monitors(s: &mut WindowState, monitors: &[MonitorRect]) {
    if monitors.is_empty() {
        return;
    }
    let cx = s.x + s.w as f64 * 0.5;
    let cy = s.y + s.h as f64 * 0.5;
    if in_any_monitor(cx, cy, monitors) {
        return;
    }

    let mut best_m = &monitors[0];
    let mut min_dist_sq = f64::MAX;
    for m in monitors {
        let mcx = m.x as f64 + m.w as f64 * 0.5;
        let mcy = m.y as f64 + m.h as f64 * 0.5;
        let d = (cx - mcx).powi(2) + (cy - mcy).powi(2);
        if d < min_dist_sq {
            min_dist_sq = d;
            best_m = m;
        }
    }

    let margin = 20.0;
    let clamped_cx = cx.clamp(
        best_m.x as f64 + margin,
        (best_m.x + best_m.w) as f64 - margin,
    );
    let clamped_cy = cy.clamp(
        best_m.y as f64 + margin,
        (best_m.y + best_m.h) as f64 - margin,
    );
    s.x = clamped_cx - s.w as f64 * 0.5;
    s.y = clamped_cy - s.h as f64 * 0.5;
}

fn move_and_bounce(s: &mut WindowState, monitors: &[MonitorRect]) {
    let (min_x, max_x, _, _) = desktop_bounds(monitors);

    let next_x = s.x + s.vx;
    let next_cx = next_x + s.w as f64 * 0.5;
    let cy = s.y + s.h as f64 * 0.5;
    if next_x < min_x || next_x + s.w as f64 > max_x || !in_any_monitor(next_cx, cy, monitors) {
        s.vx = -s.vx;
    } else {
        s.x = next_x;
    }

    let next_y = s.y + s.vy;
    let cx = s.x + s.w as f64 * 0.5;
    let next_cy = next_y + s.h as f64 * 0.5;
    if !in_any_monitor(cx, next_cy, monitors) {
        s.vy = -s.vy;
    } else {
        s.y = next_y;
    }

    clamp_to_monitors(s, monitors);
}

impl MovementPattern {
    pub fn random<R: Rng + ?Sized>(rng: &mut R) -> Self {
        match rng.gen_range(0..10) {
            0 => MovementPattern::Shake,
            1 => MovementPattern::Wander,
            2 => MovementPattern::Bounce,
            3 => MovementPattern::Orbit,
            4 => MovementPattern::Spiral,
            5 => MovementPattern::Zigzag,
            6 => MovementPattern::Dash,
            7 => MovementPattern::Jumpcut,
            8 => MovementPattern::FleeCursor,
            _ => MovementPattern::StalkCursor,
        }
    }

    pub fn init<R: Rng + ?Sized>(&self, s: &mut WindowState, rng: &mut R) {
        match self {
            MovementPattern::Bounce => {
                let a = rng.gen::<f64>() * PI * 2.0;
                s.vx = a.cos() * s.speed;
                s.vy = a.sin() * s.speed;
            }
            MovementPattern::Orbit | MovementPattern::Spiral => {
                s.phase = rng.gen::<f64>() * PI * 2.0;
            }
            MovementPattern::Zigzag => {
                s.dir_x = if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
            }
            MovementPattern::Dash => {
                s.target_x = -999999.0;
            }
            MovementPattern::Jumpcut => {
                s.phase = 20.0 + rng.gen_range(0..40) as f64;
            }
            MovementPattern::FleeCursor => {
                s.vx = 0.0;
                s.vy = 0.0;
            }
            MovementPattern::StalkCursor => {
                s.dir_x = if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
                s.phase = rng.gen::<f64>() * PI * 2.0;
            }
            _ => {}
        }
    }

    pub fn step<R: Rng + ?Sized>(&self, s: &mut WindowState, rng: &mut R) {
        let monitors = get_monitors();

        match self {
            MovementPattern::Shake => {
                s.x = s.home_x + (rng.gen::<f64>() * 2.0 - 1.0) * s.speed * 2.5;
                s.y = s.home_y + (rng.gen::<f64>() * 2.0 - 1.0) * s.speed * 2.5;
                clamp_to_monitors(s, monitors);
            }
            MovementPattern::Wander => {
                s.vx += (rng.gen::<f64>() * 2.0 - 1.0) * s.speed * 0.35;
                s.vy += (rng.gen::<f64>() * 2.0 - 1.0) * s.speed * 0.35;
                let v = s.vx.hypot(s.vy);
                if v > s.speed {
                    s.vx = s.vx / v * s.speed;
                    s.vy = s.vy / v * s.speed;
                }
                move_and_bounce(s, monitors);
            }
            MovementPattern::Bounce => {
                move_and_bounce(s, monitors);
            }
            MovementPattern::Orbit => {
                s.phase += 0.04 + s.speed * 0.01;
                let r = s.speed * 8.0 + 12.0;
                s.x = s.home_x + s.phase.cos() * r;
                s.y = s.home_y + s.phase.sin() * r;
                clamp_to_monitors(s, monitors);
            }
            MovementPattern::Spiral => {
                s.phase += 0.05 + s.speed * 0.012;
                let r =
                    (0.25 + 0.75 * (0.5 + 0.5 * (s.phase * 0.35).sin())) * (s.speed * 14.0 + 20.0);
                s.x = s.home_x + s.phase.cos() * r;
                s.y = s.home_y + s.phase.sin() * r;
                clamp_to_monitors(s, monitors);
            }
            MovementPattern::Zigzag => {
                s.vx = s.speed * s.dir_x;
                let (min_x, max_x, _, _) = desktop_bounds(monitors);
                let next_x = s.x + s.vx;
                let cx = next_x + s.w as f64 * 0.5;
                let cy = s.y + s.h as f64 * 0.5;
                if next_x < min_x
                    || next_x + s.w as f64 > max_x
                    || !in_any_monitor(cx, cy, monitors)
                {
                    s.dir_x = -s.dir_x;
                } else {
                    s.x = next_x;
                }
                s.y = s.home_y + (s.x * 0.045).sin() * s.speed * 6.0;
                clamp_to_monitors(s, monitors);
            }
            MovementPattern::Dash => {
                let dx = s.target_x - s.x;
                let dy = s.target_y - s.y;
                let d = dx.hypot(dy);
                if s.target_x < -99999.0 || d < s.speed * 2.0 + 4.0 {
                    Self::pick_target(s, rng, monitors);
                } else {
                    s.vx = s.vx * 0.85 + dx / d * s.speed * 1.6 * 0.15;
                    s.vy = s.vy * 0.85 + dy / d * s.speed * 1.6 * 0.15;
                    move_and_bounce(s, monitors);
                }
            }
            MovementPattern::Jumpcut => {
                let interval = (s.phase as u64).max(1);
                if s.tick % interval == 0 {
                    let m = &monitors[rng.gen_range(0..monitors.len())];
                    let max_x = (m.w - s.w).max(0) as f64;
                    let max_y = (m.h - s.h).max(0) as f64;
                    s.x = m.x as f64 + rng.gen::<f64>() * max_x;
                    s.y = m.y as f64 + rng.gen::<f64>() * max_y;
                    s.home_x = s.x;
                    s.home_y = s.y;
                }
            }
            MovementPattern::FleeCursor => {
                let (cur_x, cur_y) = get_cursor_pos();

                let left = s.x;
                let top = s.y;
                let right = s.x + s.w as f64;
                let bottom = s.y + s.h as f64;

                let cx = s.x + s.w as f64 * 0.5;
                let cy = s.y + s.h as f64 * 0.5;

                let closest_x = cur_x.clamp(left, right);
                let closest_y = cur_y.clamp(top, bottom);
                let edge_dist = (closest_x - cur_x).hypot(closest_y - cur_y);

                let mut dir_x = cx - cur_x;
                let mut dir_y = cy - cur_y;
                let center_dist = dir_x.hypot(dir_y);

                if center_dist < 0.001 {
                    dir_x = rng.gen::<f64>() * 2.0 - 1.0;
                    dir_y = rng.gen::<f64>() * 2.0 - 1.0;
                }
                let len = dir_x.hypot(dir_y).max(0.001);
                dir_x /= len;
                dir_y /= len;

                let panic_radius = 240.0;
                if edge_dist < panic_radius {
                    let urgency = (1.0 - edge_dist / panic_radius).powi(2);
                    let flee_speed = s.speed * (2.4 + urgency * 8.5);
                    let target_vx = dir_x * flee_speed;
                    let target_vy = dir_y * flee_speed;

                    s.vx = s.vx * 0.74 + target_vx * 0.26;
                    s.vy = s.vy * 0.74 + target_vy * 0.26;
                } else {
                    s.vx *= 0.92;
                    s.vy *= 0.92;
                    s.vx += (rng.gen::<f64>() * 2.0 - 1.0) * 0.25;
                    s.vy += (rng.gen::<f64>() * 2.0 - 1.0) * 0.25;
                }

                if edge_dist < panic_radius {
                    let next_cx = cx + s.vx;
                    let next_cy = cy + s.vy;
                    let can_move_x = in_any_monitor(next_cx, cy, monitors);
                    let can_move_y = in_any_monitor(cx, next_cy, monitors);

                    if !can_move_x && can_move_y {
                        let slide_dir = if cy >= cur_y { 1.0 } else { -1.0 };
                        s.vy += slide_dir * s.speed * 2.5;
                    } else if !can_move_y && can_move_x {
                        let slide_dir = if cx >= cur_x { 1.0 } else { -1.0 };
                        s.vx += slide_dir * s.speed * 2.5;
                    }
                }

                move_and_bounce(s, monitors);
            }
            MovementPattern::StalkCursor => {
                let (cur_x, cur_y) = get_cursor_pos();

                let left = s.x;
                let top = s.y;
                let right = s.x + s.w as f64;
                let bottom = s.y + s.h as f64;

                let cx = s.x + s.w as f64 * 0.5;
                let cy = s.y + s.h as f64 * 0.5;

                let closest_x = cur_x.clamp(left, right);
                let closest_y = cur_y.clamp(top, bottom);
                let edge_dist = (closest_x - cur_x).hypot(closest_y - cur_y);

                let to_cur_x = cur_x - cx;
                let to_cur_y = cur_y - cy;
                let center_dist = to_cur_x.hypot(to_cur_y).max(0.1);
                let dir_x = to_cur_x / center_dist;
                let dir_y = to_cur_y / center_dist;

                let ideal_edge_dist = 110.0;
                let is_inside = cur_x >= left && cur_x <= right && cur_y >= top && cur_y <= bottom;
                let dist_diff = if is_inside {
                    -ideal_edge_dist - 40.0
                } else {
                    edge_dist - ideal_edge_dist
                };

                let radial_speed = (dist_diff * 0.06).clamp(-s.speed * 2.0, s.speed * 2.5);

                let tangent_x = -dir_y;
                let tangent_y = dir_x;
                let tangent_speed = s.speed * 1.2 * s.dir_x;

                s.phase += 0.04;
                let breathing = (s.phase * 2.5).sin() * 0.7;

                let desired_vx =
                    dir_x * radial_speed + tangent_x * tangent_speed + dir_y * breathing;
                let desired_vy =
                    dir_y * radial_speed + tangent_y * tangent_speed - dir_x * breathing;

                s.vx = s.vx * 0.85 + desired_vx * 0.15;
                s.vy = s.vy * 0.85 + desired_vy * 0.15;

                move_and_bounce(s, monitors);
            }
        }
    }

    fn pick_target<R: Rng + ?Sized>(s: &mut WindowState, rng: &mut R, monitors: &[MonitorRect]) {
        if monitors.is_empty() {
            return;
        }
        let m = &monitors[rng.gen_range(0..monitors.len())];
        let max_x = (m.w - s.w).max(0) as f64;
        let max_y = (m.h - s.h).max(0) as f64;
        s.target_x = m.x as f64 + rng.gen::<f64>() * max_x;
        s.target_y = m.y as f64 + rng.gen::<f64>() * max_y;
    }
}
