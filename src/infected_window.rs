use rand::Rng;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::assets::{DecodedImage, RED_WARNING_BYTES};
use crate::pattern::MovementPattern;
use crate::state::WindowState;
use crate::window::{get_monitors, live_window_dec, live_window_inc};

const CLIENT_W: i32 = 440;
const CLIENT_H: i32 = 204;
const ID_BTN_OK: usize = 1;

static WARNING_ICON: OnceLock<Option<DecodedImage>> = OnceLock::new();

fn warning_icon() -> Option<&'static DecodedImage> {
    WARNING_ICON
        .get_or_init(|| {
            DecodedImage::load_png_scaled_blended(RED_WARNING_BYTES, 86, 86, (10, 13, 16)).ok()
        })
        .as_ref()
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

struct InfectedData {
    hwnd_btn: HWND,
    hfont_header: HFONT,
    hfont_body: HFONT,
    hfont_btn: HFONT,
    hbr_bg: HBRUSH,
    hbr_btn: HBRUSH,
    hbr_btn_pressed: HBRUSH,
    hpen_btn_border: HPEN,

    state: WindowState,
    pattern: MovementPattern,
    life_ms: i32,
}

const CLASS_NAME: &str = "NikbvirusInfectedWindowClass";

pub unsafe fn register_infected_window_class(hinstance: HMODULE) -> bool {
    let class_name = to_wide(CLASS_NAME);
    let bg_brush = CreateSolidBrush(rgb(10, 13, 16));
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(infected_window_proc),
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

pub unsafe fn spawn_infected_window(hinstance: HMODULE) {
    spawn_infected_window_with_pattern(hinstance, None);
}

pub unsafe fn spawn_infected_window_with_pattern(
    hinstance: HMODULE,
    pattern_override: Option<MovementPattern>,
) {
    let mut rng = rand::thread_rng();
    let monitors = get_monitors();
    let monitor = &monitors[rng.gen_range(0..monitors.len())];

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let ex_style = WS_EX_TOPMOST | WS_EX_NOACTIVATE;

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: CLIENT_W,
        bottom: CLIENT_H,
    };
    AdjustWindowRectEx(&mut rect, style, 0, ex_style);
    let win_w = rect.right - rect.left;
    let win_h = rect.bottom - rect.top;

    let max_x = (monitor.w - win_w).max(1);
    let max_y = (monitor.h - win_h).max(1);
    let init_x = monitor.x + rng.gen_range(0..max_x);
    let init_y = monitor.y + rng.gen_range(0..max_y);

    let speed = 1.5 + rng.gen::<f64>() * 3.0;
    let mut state = WindowState::new(init_x as f64, init_y as f64, win_w, win_h, speed);
    let pattern = pattern_override.unwrap_or_else(|| MovementPattern::random(&mut rng));
    pattern.init(&mut state, &mut rng);

    let font_name = to_wide("Segoe UI");
    let hfont_header = CreateFontW(
        -28,
        0,
        0,
        0,
        FW_BOLD as i32,
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
        -16,
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

    let hfont_btn = CreateFontW(
        -16,
        0,
        0,
        0,
        FW_BOLD as i32,
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

    let hbr_bg = CreateSolidBrush(rgb(10, 13, 16));
    let hbr_btn = CreateSolidBrush(rgb(228, 0, 0));
    let hbr_btn_pressed = CreateSolidBrush(rgb(180, 0, 0));
    let hpen_btn_border = CreatePen(PS_SOLID as i32, 2, rgb(230, 230, 230));

    let life_ms = -10_000 - rng.gen_range(0..14_000);

    let data = Box::new(InfectedData {
        hwnd_btn: std::ptr::null_mut(),
        hfont_header,
        hfont_body,
        hfont_btn,
        hbr_bg,
        hbr_btn,
        hbr_btn_pressed,
        hpen_btn_border,
        state,
        pattern,
        life_ms,
    });

    let class_name = to_wide(CLASS_NAME);
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
            HWND_TOPMOST,
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

const ODS_SELECTED: u32 = 0x0001;

#[repr(C)]
struct DRAWITEMSTRUCT {
    pub ctl_type: u32,
    pub ctl_id: u32,
    pub item_id: u32,
    pub item_action: u32,
    pub item_state: u32,
    pub hwnd_item: HWND,
    pub hdc: HDC,
    pub rc_item: RECT,
    pub item_data: usize,
}

pub unsafe extern "system" fn infected_window_proc(
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
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InfectedData;
            if !ptr.is_null() {
                let data = &mut *ptr;
                let cs = lparam as *const CREATESTRUCTW;
                let hinstance = (*cs).hInstance;

                let btn_class = to_wide("BUTTON");
                let btn_title = to_wide("OK");
                let hwnd_btn = CreateWindowExW(
                    0,
                    btn_class.as_ptr(),
                    btn_title.as_ptr(),
                    WS_CHILD | WS_VISIBLE | BS_OWNERDRAW as u32,
                    265,
                    154,
                    148,
                    34,
                    hwnd,
                    ID_BTN_OK as _,
                    hinstance,
                    std::ptr::null(),
                );
                data.hwnd_btn = hwnd_btn;

                SetTimer(hwnd, 1, 33, None);
            }
            0
        }
        WM_DRAWITEM => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InfectedData;
            let dis = lparam as *const DRAWITEMSTRUCT;
            if !ptr.is_null() && !dis.is_null() {
                let data = &*ptr;
                let hdc = (*dis).hdc;
                let rc = (*dis).rc_item;

                let is_pressed = ((*dis).item_state & ODS_SELECTED) != 0;
                let brush = if is_pressed {
                    data.hbr_btn_pressed
                } else {
                    data.hbr_btn
                };

                let old_pen = SelectObject(hdc, data.hpen_btn_border as _);
                let old_brush = SelectObject(hdc, brush as _);

                RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, 6, 6);

                SelectObject(hdc, old_brush);
                SelectObject(hdc, old_pen);

                let old_font = SelectObject(hdc, data.hfont_btn as _);
                SetBkMode(hdc, TRANSPARENT as i32);
                SetTextColor(hdc, rgb(255, 255, 255));

                let text = to_wide("OK");
                let mut text_rc = rc;
                DrawTextW(
                    hdc,
                    text.as_ptr(),
                    text.len() as i32 - 1,
                    &mut text_rc,
                    (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
                );

                SelectObject(hdc, old_font);
                return 1;
            }
            0
        }
        WM_SETCURSOR => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InfectedData;
            if !ptr.is_null() {
                let data = &*ptr;
                if lparam as HWND == data.hwnd_btn {
                    SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_HAND));
                    return 1;
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as usize;
            if id == ID_BTN_OK {
                DestroyWindow(hwnd);
                return 0;
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InfectedData;
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !hdc.is_null() && !ptr.is_null() {
                let data = &*ptr;

                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, data.hbr_bg);

                if let Some(img) = warning_icon() {
                    SetStretchBltMode(hdc, HALFTONE as i32);
                    SetBrushOrgEx(hdc, 0, 0, std::ptr::null_mut());

                    let mut bmi: BITMAPINFO = std::mem::zeroed();
                    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                    bmi.bmiHeader.biWidth = img.width;
                    bmi.bmiHeader.biHeight = -img.height;
                    bmi.bmiHeader.biPlanes = 1;
                    bmi.bmiHeader.biBitCount = 32;
                    bmi.bmiHeader.biCompression = BI_RGB;

                    StretchDIBits(
                        hdc,
                        14,
                        10,
                        img.width,
                        img.height,
                        0,
                        0,
                        img.width,
                        img.height,
                        img.bgra.as_ptr() as *const _,
                        &bmi,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                }

                SetBkMode(hdc, TRANSPARENT as i32);
                let old_font = SelectObject(hdc, data.hfont_header as _);
                SetTextColor(hdc, rgb(255, 255, 255));
                let header = to_wide("Никб вирус");
                TextOutW(hdc, 114, 16, header.as_ptr(), header.len() as i32 - 1);

                SelectObject(hdc, data.hfont_body as _);
                SetTextColor(hdc, rgb(238, 238, 238));

                let lines = [
                    (to_wide("Ваш компьютер заражен!"), 60),
                    (to_wide("Файл: никб.exe"), 84),
                    (to_wide("Вся система теперь под контролем"), 108),
                    (to_wide("Никб вируса!"), 132),
                ];

                for (line_w, y) in &lines {
                    TextOutW(hdc, 114, *y, line_w.as_ptr(), line_w.len() as i32 - 1);
                }

                SelectObject(hdc, old_font);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InfectedData;
            if !ptr.is_null() {
                let data = &mut *ptr;
                data.state.tick += 1;
                let mut rng = rand::thread_rng();
                data.pattern.step(&mut data.state, &mut rng);

                let x = data.state.x.round() as i32;
                let y = data.state.y.round() as i32;

                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );

                data.life_ms += 33;
                if data.life_ms >= 0 {
                    DestroyWindow(hwnd);
                    return 0;
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
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InfectedData;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let data = Box::from_raw(ptr);
                DeleteObject(data.hfont_header as _);
                DeleteObject(data.hfont_body as _);
                DeleteObject(data.hfont_btn as _);
                DeleteObject(data.hbr_bg as _);
                DeleteObject(data.hbr_btn as _);
                DeleteObject(data.hbr_btn_pressed as _);
                DeleteObject(data.hpen_btn_border as _);
                live_window_dec();
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
