use std::sync::atomic::{AtomicI32, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static LIVE_WINDOWS: AtomicI32 = AtomicI32::new(0);

pub fn live_window_count() -> i32 {
    LIVE_WINDOWS.load(Ordering::SeqCst)
}

pub fn live_window_inc() {
    LIVE_WINDOWS.fetch_add(1, Ordering::SeqCst);
}

pub fn live_window_dec() {
    LIVE_WINDOWS.fetch_sub(1, Ordering::SeqCst);
}

#[derive(Clone, Copy, Debug)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

static MONITORS: std::sync::OnceLock<Vec<MonitorRect>> = std::sync::OnceLock::new();

pub fn get_monitors() -> &'static [MonitorRect] {
    MONITORS.get_or_init(get_all_monitors)
}

pub fn configure_monitors(display_number: Option<usize>) -> Result<usize, String> {
    let mut monitors = get_all_monitors();
    monitors.sort_by_key(|monitor| (monitor.x, monitor.y));
    let monitor_count = monitors.len();

    if let Some(number) = display_number {
        if number == 0 || number > monitor_count {
            return Err(format!(
                "Дисплей {number} не найден. Подключено дисплеев: {monitor_count}. Нумерация идёт слева направо."
            ));
        }
        MONITORS
            .set(vec![monitors[number - 1]])
            .map_err(|_| "Не удалось настроить список дисплеев.".to_string())?;
    } else {
        MONITORS
            .set(monitors)
            .map_err(|_| "Не удалось настроить список дисплеев.".to_string())?;
    }

    Ok(monitor_count)
}

unsafe extern "system" fn monitor_enum_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitors = &mut *(lparam as *mut Vec<MonitorRect>);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(hmon, &mut mi) != 0 {
        monitors.push(MonitorRect {
            x: mi.rcWork.left,
            y: mi.rcWork.top,
            w: (mi.rcWork.right - mi.rcWork.left).max(640),
            h: (mi.rcWork.bottom - mi.rcWork.top).max(480),
        });
    }
    1
}

pub fn get_all_monitors() -> Vec<MonitorRect> {
    let mut monitors = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_enum_proc),
            &mut monitors as *mut _ as isize,
        );
    }
    if monitors.is_empty() {
        let w = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(640);
        let h = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(480);
        monitors.push(MonitorRect { x: 0, y: 0, w, h });
    }
    monitors
}
