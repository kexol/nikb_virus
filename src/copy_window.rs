use rand::Rng;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::pattern::MovementPattern;
use crate::state::WindowState;
use crate::window::{get_monitors, live_window_dec, live_window_inc};

const WM_USER: u32 = 0x0400;
const PBM_SETRANGE32: u32 = WM_USER + 6;
const PBM_SETPOS: u32 = WM_USER + 2;
const PBM_SETSTATE: u32 = WM_USER + 16;
const PBST_NORMAL: usize = 1;
const PBS_SMOOTH: u32 = 0x0001;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

pub struct CopyWindowData {
    pub progress: f64,
    pub speed: f64,
    pub pause_ticks: i32,
    pub completed: bool,
    pub countdown_ms: i32,
    pub hwnd_pb: HWND,
    pub hwnd_lbl_percent: HWND,
    pub hwnd_lbl_msg: HWND,
    pub hfont_title: HFONT,
    pub hfont_body: HFONT,
    pub hbr_bg: HBRUSH,

    pub moving: bool,
    pub state: Option<WindowState>,
    pub pattern: Option<MovementPattern>,
}

pub unsafe fn register_copy_window_class(hinstance: HMODULE) -> bool {
    let icce = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_PROGRESS_CLASS,
    };
    InitCommonControlsEx(&icce);

    let class_name = to_wide("NikbvirusCopyingWindowClass");
    let bg_brush = CreateSolidBrush(rgb(225, 234, 241));
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(copy_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: LoadIconW(std::ptr::null_mut(), IDI_WARNING),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: bg_brush,
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: LoadIconW(std::ptr::null_mut(), IDI_WARNING),
    };
    RegisterClassExW(&wc) != 0
}

pub unsafe fn spawn_copy_window(hinstance: HMODULE, moving: bool) {
    let mut rng = rand::thread_rng();
    let monitors = get_monitors();
    let monitor = &monitors[rng.gen_range(0..monitors.len())];

    let client_w = 360;
    let client_h = 140;

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let ex_style = WS_EX_TOPMOST | WS_EX_NOACTIVATE;

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    AdjustWindowRectEx(&mut rect, style, 0, ex_style);
    let win_w = rect.right - rect.left;
    let win_h = rect.bottom - rect.top;

    let (init_x, init_y) = if moving {
        let max_x = (monitor.w - win_w).max(1);
        let max_y = (monitor.h - win_h).max(1);
        (
            monitor.x + rng.gen_range(0..max_x),
            monitor.y + rng.gen_range(0..max_y),
        )
    } else {
        let center_x = monitor.x + (monitor.w - win_w) / 2 + rng.gen_range(-30..=30);
        let center_y = monitor.y + (monitor.h - win_h) / 2 + rng.gen_range(-30..=30);
        (center_x, center_y)
    };

    let bg_brush = CreateSolidBrush(rgb(225, 234, 241));
    let font_name = to_wide("Segoe UI");

    let hfont_title = CreateFontW(
        -15,
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        font_name.as_ptr(),
    );

    let hfont_body = CreateFontW(
        -14,
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        font_name.as_ptr(),
    );

    let (state_opt, pattern_opt) = if moving {
        let speed = 1.5 + rng.gen::<f64>() * 3.0;
        let mut st = WindowState::new(init_x as f64, init_y as f64, win_w, win_h, speed);
        let pat = MovementPattern::random(&mut rng);
        pat.init(&mut st, &mut rng);
        (Some(st), Some(pat))
    } else {
        (None, None)
    };

    let data = Box::new(CopyWindowData {
        progress: 0.0,
        speed: 0.85 + rng.gen::<f64>() * 0.35,
        pause_ticks: 0,
        completed: false,
        countdown_ms: 0,
        hwnd_pb: std::ptr::null_mut(),
        hwnd_lbl_percent: std::ptr::null_mut(),
        hwnd_lbl_msg: std::ptr::null_mut(),
        hfont_title,
        hfont_body,
        hbr_bg: bg_brush,
        moving,
        state: state_opt,
        pattern: pattern_opt,
    });

    let class_name = to_wide("NikbvirusCopyingWindowClass");
    let title = to_wide("Никб вирус");

    let hwnd = CreateWindowExW(
        ex_style,
        class_name.as_ptr(),
        title.as_ptr(),
        style,
        init_x,
        init_y,
        win_w,
        win_h,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        hinstance,
        Box::into_raw(data) as *const _,
    );

    if !hwnd.is_null() {
        live_window_inc();
        SetWindowPos(
            hwnd,
            -1 as isize as HWND,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        UpdateWindow(hwnd);
    }
}

pub unsafe extern "system" fn copy_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = lparam as *const CREATESTRUCTW;
            if !cs.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CREATE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CopyWindowData;
            if !ptr.is_null() {
                let data = &mut *ptr;
                let cs = lparam as *const CREATESTRUCTW;
                let hinstance = (*cs).hInstance;

                let static_class = to_wide("STATIC");

                let lbl_copy_text = to_wide("Копирование... Никб.exe");
                let hwnd_lbl_title = CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    lbl_copy_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | 0,
                    16,
                    14,
                    328,
                    20,
                    hwnd,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                );
                SendMessageW(hwnd_lbl_title, WM_SETFONT, data.hfont_title as usize, 1);

                let pb_class = to_wide("msctls_progress32");
                let hwnd_pb = CreateWindowExW(
                    0,
                    pb_class.as_ptr(),
                    std::ptr::null(),
                    WS_CHILD | WS_VISIBLE | PBS_SMOOTH,
                    16,
                    42,
                    328,
                    22,
                    hwnd,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                );
                SendMessageW(hwnd_pb, PBM_SETRANGE32, 0, 1000);
                SendMessageW(hwnd_pb, PBM_SETPOS, 0, 0);
                SendMessageW(hwnd_pb, PBM_SETSTATE, PBST_NORMAL, 0);
                data.hwnd_pb = hwnd_pb;

                let pct_text = to_wide("0%");
                let hwnd_lbl_percent = CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    pct_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | 0,
                    16,
                    70,
                    328,
                    20,
                    hwnd,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                );
                SendMessageW(hwnd_lbl_percent, WM_SETFONT, data.hfont_body as usize, 1);
                data.hwnd_lbl_percent = hwnd_lbl_percent;

                let empty_text = to_wide("");
                let hwnd_lbl_msg = CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    empty_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE | 0,
                    16,
                    94,
                    328,
                    22,
                    hwnd,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                );
                SendMessageW(hwnd_lbl_msg, WM_SETFONT, data.hfont_body as usize, 1);
                data.hwnd_lbl_msg = hwnd_lbl_msg;

                SetTimer(hwnd, 1, 33, None);
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CopyWindowData;
            if !ptr.is_null() {
                let data = &*ptr;
                let hdc = wparam as HDC;
                SetBkMode(hdc, TRANSPARENT as i32);
                SetTextColor(hdc, rgb(24, 24, 24));
                return data.hbr_bg as LRESULT;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ERASEBKGND => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CopyWindowData;
            if !ptr.is_null() {
                let data = &*ptr;
                let hdc = wparam as HDC;
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, data.hbr_bg);
                return 1;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_TIMER => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CopyWindowData;
            if !ptr.is_null() {
                let data = &mut *ptr;

                if !data.completed {
                    if data.pause_ticks > 0 {
                        data.pause_ticks -= 1;
                    } else {
                        let mut rng = rand::thread_rng();

                        if rng.gen_ratio(1, 35) {
                            data.pause_ticks = rng.gen_range(3..8);
                        } else {
                            let step = rng.gen_range(0.35..1.05) * data.speed;
                            data.progress += step;

                            if data.progress >= 100.0 {
                                data.progress = 100.0;
                                data.completed = true;
                                data.countdown_ms = 3000;

                                SendMessageW(data.hwnd_pb, PBM_SETPOS, 1000, 0);

                                let pct_str = to_wide("100%");
                                SetWindowTextW(data.hwnd_lbl_percent, pct_str.as_ptr());

                                let msg_str = to_wide("Теперь ты тоже Никб...");
                                SetWindowTextW(data.hwnd_lbl_msg, msg_str.as_ptr());
                            } else {
                                let pos = (data.progress * 10.0).round() as usize;
                                SendMessageW(data.hwnd_pb, PBM_SETPOS, pos, 0);

                                let pct_text = format!("{}%", data.progress.floor() as i32);
                                let pct_str = to_wide(&pct_text);
                                SetWindowTextW(data.hwnd_lbl_percent, pct_str.as_ptr());
                            }
                        }
                    }
                } else {
                    data.countdown_ms -= 33;
                    if data.countdown_ms <= 0 {
                        DestroyWindow(hwnd);
                        return 0;
                    }
                }

                if data.moving {
                    if let (Some(state), Some(pattern)) =
                        (data.state.as_mut(), data.pattern.as_ref())
                    {
                        let mut rng = rand::thread_rng();
                        state.tick += 1;
                        pattern.step(state, &mut rng);
                        let x = state.x.round() as i32;
                        let y = state.y.round() as i32;
                        SetWindowPos(
                            hwnd,
                            std::ptr::null_mut(),
                            x,
                            y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                        );
                    }
                }
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, 1);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CopyWindowData;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let data = Box::from_raw(ptr);
                DeleteObject(data.hfont_title as _);
                DeleteObject(data.hfont_body as _);
                DeleteObject(data.hbr_bg as _);
                live_window_dec();
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
