#![windows_subsystem = "windows"]

mod assets;
mod audio;
mod cmd_hack;
mod copy_window;
mod infected_window;
mod pattern;
mod state;
mod terminal_window;
mod window;

use rand::Rng;
use std::cell::RefCell;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::audio::AudioManager;
use crate::window::live_window_count;

struct ControllerState {
    hinstance: HMODULE,
    initial_delay: i32,
    spawn_in: i32,
    sound_in: i32,
    active: bool,
    cmd_hack: Option<crate::cmd_hack::CmdHackProcess>,
}

thread_local! {
    static CONTROLLER: RefCell<Option<ControllerState>> = RefCell::new(None);
    static AUDIO: RefCell<Option<AudioManager>> = RefCell::new(None);
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const MAX_OPEN_WINDOWS: usize = 6;

fn parse_display_number() -> Result<Option<usize>, String> {
    let mut args = std::env::args().skip(1);
    let mut display_number = None;
    while let Some(arg) = args.next() {
        if arg != "--display" {
            return Err(format!(
                "Неизвестный аргумент: {arg}\nИспользование: nikb_virus.exe [--display N]"
            ));
        }
        if display_number.is_some() {
            return Err("Параметр --display можно указать только один раз.".to_string());
        }
        let value = args
            .next()
            .ok_or_else(|| "После --display нужно указать номер дисплея.".to_string())?;
        let number = value
            .parse::<usize>()
            .ok()
            .filter(|number| *number > 0)
            .ok_or_else(|| {
                "Значение --display должно быть номером дисплея, начиная с 1.".to_string()
            })?;
        display_number = Some(number);
    }
    Ok(display_number)
}

fn show_startup_error(message: &str) {
    let title = to_wide("Nikbvirus: ошибка запуска");
    let message = to_wide(message);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

unsafe fn register_emergency_hotkey(hwnd: HWND) {
    RegisterHotKey(hwnd, 1, MOD_CONTROL | MOD_SHIFT, 'Q' as u32);
}

unsafe fn unregister_emergency_hotkey(hwnd: HWND) {
    UnregisterHotKey(hwnd, 1);
}

unsafe extern "system" fn controller_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            register_emergency_hotkey(hwnd);
            if SetTimer(hwnd, 1, 50, None) == 0 {
                unregister_emergency_hotkey(hwnd);
                return -1;
            }
            0
        }
        WM_HOTKEY => {
            PostQuitMessage(0);
            0
        }
        WM_TIMER => {
            CONTROLLER.with(|cell| {
                let mut guard = cell.borrow_mut();
                if let Some(ctrl) = guard.as_mut() {
                    if ctrl
                        .cmd_hack
                        .as_mut()
                        .is_some_and(|child| child.poll_finished())
                    {
                        ctrl.cmd_hack.take();
                    }
                    let mut rng = rand::thread_rng();

                    if ctrl.initial_delay > 0 {
                        ctrl.initial_delay -= 1;
                        if ctrl.initial_delay == 0 {
                            ctrl.active = true;
                            ctrl.spawn_in = rng.gen_range(60..120);
                            ctrl.sound_in = rng.gen_range(200..350);
                            if (live_window_count().max(0) as usize) < MAX_OPEN_WINDOWS {
                                crate::copy_window::spawn_copy_window(ctrl.hinstance, true);
                            }
                            AUDIO.with(|a_cell| {
                                if let Some(audio) = a_cell.borrow().as_ref() {
                                    audio.play_random(&mut rng);
                                }
                            });
                        }
                    }

                    if ctrl.active {
                        ctrl.spawn_in -= 1;
                        if ctrl.spawn_in <= 0 {
                            ctrl.spawn_in = rng.gen_range(80..180);
                            if (live_window_count().max(0) as usize) < MAX_OPEN_WINDOWS {
                                let roll = rng.gen_range(0..3);
                                if roll == 0 {
                                    crate::copy_window::spawn_copy_window(ctrl.hinstance, true);
                                } else if roll == 1 {
                                    crate::terminal_window::spawn_terminal_window(ctrl.hinstance);
                                } else {
                                    crate::infected_window::spawn_infected_window(ctrl.hinstance);
                                }
                                AUDIO.with(|a_cell| {
                                    if let Some(audio) = a_cell.borrow().as_ref() {
                                        audio.play_random(&mut rng);
                                    }
                                });
                            }
                        }

                        ctrl.sound_in -= 1;
                        if ctrl.sound_in <= 0 {
                            ctrl.sound_in = rng.gen_range(200..450);
                            AUDIO.with(|a_cell| {
                                if let Some(audio) = a_cell.borrow().as_ref() {
                                    audio.play_random(&mut rng);
                                }
                            });
                        }
                    }
                }
            });
            0
        }
        WM_DESTROY => {
            unregister_emergency_hotkey(hwnd);
            KillTimer(hwnd, 1);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(pvreserved: *const std::ffi::c_void, dwcoinit: u32) -> i32;
}

fn main() {
    if std::env::var_os("NIKB_VIRUS_INTERNAL_WORKER").as_deref() == Some(std::ffi::OsStr::new("1"))
    {
        crate::cmd_hack::run_cmd_hack_worker();
        return;
    }
    let display_number = match parse_display_number() {
        Ok(number) => number,
        Err(message) => {
            show_startup_error(&message);
            return;
        }
    };
    if let Err(message) = crate::window::configure_monitors(display_number) {
        show_startup_error(&message);
        return;
    }

    unsafe {
        CoInitializeEx(std::ptr::null(), 0x2);
        let hinstance = GetModuleHandleW(std::ptr::null());

        AUDIO.with(|cell| {
            *cell.borrow_mut() = Some(AudioManager::new());
        });

        if hinstance.is_null()
            || !crate::copy_window::register_copy_window_class(hinstance)
            || !crate::terminal_window::register_terminal_window_class(hinstance)
            || !crate::infected_window::register_infected_window_class(hinstance)
        {
            show_startup_error(
                "Не удалось зарегистрировать классы окон. Приложение будет закрыто.",
            );
            return;
        }

        let ctrl_class_name = to_wide("NikbvirusControllerClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(controller_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: ctrl_class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            return;
        }

        CONTROLLER.with(|cell| {
            *cell.borrow_mut() = Some(ControllerState {
                hinstance,
                initial_delay: 600,
                spawn_in: 20,
                sound_in: 40,
                active: false,
                cmd_hack: None,
            });
        });

        let ctrl_title = to_wide("NikbvirusController");
        let controller_hwnd = CreateWindowExW(
            0,
            ctrl_class_name.as_ptr(),
            ctrl_title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        if controller_hwnd.is_null() {
            CONTROLLER.with(|cell| *cell.borrow_mut() = None);
            return;
        }

        let cmd_hack = crate::cmd_hack::spawn_cmd_hack_window();
        if cmd_hack.is_none() {
            show_startup_error(
                "Не удалось запустить окно CMD. Основная часть приложения продолжит работу.",
            );
        }
        CONTROLLER.with(|cell| {
            if let Some(ctrl) = cell.borrow_mut().as_mut() {
                ctrl.cmd_hack = cmd_hack;
            }
        });

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        DestroyWindow(controller_hwnd);
        CONTROLLER.with(|cell| {
            if let Some(ctrl) = cell.borrow_mut().as_mut() {
                if let Some(mut child) = ctrl.cmd_hack.take() {
                    child.stop();
                }
            }
            *cell.borrow_mut() = None;
        });
        AUDIO.with(|cell| {
            cell.borrow_mut().take();
        });
    }
}
