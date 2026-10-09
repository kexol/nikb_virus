use std::ffi::c_void;
use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(dwProcessId: u32) -> BOOL;
    fn AllocConsole() -> BOOL;
    fn GetConsoleWindow() -> HWND;
    fn GetConsoleMode(hConsoleHandle: HANDLE, lpMode: *mut u32) -> BOOL;
    fn SetConsoleMode(hConsoleHandle: HANDLE, dwMode: u32) -> BOOL;
    fn SetConsoleTitleW(lpConsoleTitle: *const u16) -> BOOL;
    fn WriteConsoleW(
        hConsoleOutput: HANDLE,
        lpBuffer: *const c_void,
        nNumberOfCharsToWrite: u32,
        lpNumberOfCharsWritten: *mut u32,
        lpReserved: *mut c_void,
    ) -> BOOL;
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *const c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: HANDLE,
    ) -> HANDLE;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct CmdHackProcess {
    process: HANDLE,
    job: HANDLE,
}

impl CmdHackProcess {
    pub fn poll_finished(&mut self) -> bool {
        if self.process.is_null()
            || unsafe { WaitForSingleObject(self.process, 0) } != WAIT_OBJECT_0
        {
            return false;
        }
        unsafe {
            CloseHandle(self.process);
            self.process = std::ptr::null_mut();
            if !self.job.is_null() {
                CloseHandle(self.job);
                self.job = std::ptr::null_mut();
            }
        }
        true
    }

    pub fn stop(&mut self) {
        unsafe {
            if !self.job.is_null() && self.job != INVALID_HANDLE_VALUE {
                TerminateJobObject(self.job, 0);
                CloseHandle(self.job);
                self.job = std::ptr::null_mut();
            }
            if !self.process.is_null() && self.process != INVALID_HANDLE_VALUE {
                TerminateProcess(self.process, 0);
                WaitForSingleObject(self.process, INFINITE);
                CloseHandle(self.process);
                self.process = std::ptr::null_mut();
            }
        }
    }
}

impl Drop for CmdHackProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn spawn_cmd_hack_window() -> Option<CmdHackProcess> {
    let current_exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(_) => return None,
    };

    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() || job == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const c_void,
            std::mem::size_of_val(&limits) as u32,
        )
    } != 0;
    if !configured {
        unsafe {
            CloseHandle(job);
        }
        return None;
    }

    let cmd_path = to_wide(r"C:\Windows\System32\cmd.exe");
    let working_dir = to_wide(r"C:\Windows\System32");
    let command_line = format!(
        "\"{}\" /c \"\"{}\"\"",
        String::from_utf16_lossy(&cmd_path[..cmd_path.len() - 1]),
        current_exe.to_string_lossy()
    );
    let mut command_line = to_wide(&command_line);
    let environment = worker_environment();
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            cmd_path.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NEW_CONSOLE | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr() as *const c_void,
            working_dir.as_ptr(),
            &startup,
            &mut info,
        )
    } != 0;
    if !created {
        unsafe {
            CloseHandle(job);
        }
        return None;
    }

    let assigned = unsafe { AssignProcessToJobObject(job, info.hProcess) } != 0;
    let resumed = assigned && unsafe { ResumeThread(info.hThread) } != u32::MAX;
    unsafe {
        CloseHandle(info.hThread);
    }
    if !resumed {
        unsafe {
            TerminateProcess(info.hProcess, 1);
            TerminateJobObject(job, 1);
            WaitForSingleObject(info.hProcess, INFINITE);
            CloseHandle(info.hProcess);
            CloseHandle(job);
        }
        return None;
    }

    Some(CmdHackProcess {
        process: info.hProcess,
        job,
    })
}

fn worker_environment() -> Vec<u16> {
    let marker = OsString::from("NIKB_VIRUS_INTERNAL_WORKER");
    let mut variables: Vec<_> = std::env::vars_os()
        .filter(|(key, _)| key != &marker)
        .collect();
    variables.push((marker, OsString::from("1")));
    variables.sort_by(|(left, _), (right, _)| {
        left.to_string_lossy()
            .to_ascii_uppercase()
            .cmp(&right.to_string_lossy().to_ascii_uppercase())
    });

    let mut block = Vec::new();
    for (key, value) in variables {
        block.extend(std::ffi::OsStr::new(&key).encode_wide());
        block.push(b'=' as u16);
        block.extend(std::ffi::OsStr::new(&value).encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

struct ConsoleOut {
    handle: HANDLE,
}

impl ConsoleOut {
    fn new() -> Self {
        unsafe {
            if AttachConsole(0xFFFFFFFF) == 0 {
                AllocConsole();
            }

            let conout_name = to_wide("CONOUT$");
            let handle = CreateFileW(
                conout_name.as_ptr(),
                0x40000000 | 0x80000000,
                1 | 2,
                std::ptr::null(),
                3,
                0,
                std::ptr::null_mut(),
            );

            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) != 0 {
                SetConsoleMode(handle, mode | 0x0004 | 0x0001);
            }

            let title = to_wide(
                "Администратор: C:\\Windows\\System32\\cmd.exe - sberbank_hack_roblox_2026.exe",
            );
            SetConsoleTitleW(title.as_ptr());

            Self { handle }
        }
    }

    fn write(&self, text: &str) {
        if self.handle.is_null() || self.handle == INVALID_HANDLE_VALUE {
            return;
        }
        let wide = to_wide(text);
        let mut written = 0;
        unsafe {
            WriteConsoleW(
                self.handle,
                wide.as_ptr() as *const _,
                (wide.len() - 1) as u32,
                &mut written,
                std::ptr::null_mut(),
            );
        }
    }

    fn writeln(&self, text: &str) {
        self.write(text);
        self.write("\r\n");
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

struct ConsoleMover {
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ConsoleMover {
    fn start() -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = std::thread::spawn(move || {
            let hwnd = unsafe { GetConsoleWindow() };
            if hwnd.is_null() {
                return;
            }
            let hwnd_raw = hwnd as usize;

            let monitors = crate::window::get_monitors();

            let win_w = 980;
            let win_h = 620;

            let (start_x, start_y) = if let Some(m) = monitors.first() {
                (
                    m.x as f64 + (m.w - win_w).max(0) as f64 * 0.5,
                    m.y as f64 + (m.h - win_h).max(0) as f64 * 0.5,
                )
            } else {
                (100.0, 60.0)
            };

            unsafe {
                SetWindowPos(
                    hwnd_raw as HWND,
                    std::ptr::null_mut(),
                    start_x.round() as i32,
                    start_y.round() as i32,
                    win_w,
                    win_h,
                    SWP_SHOWWINDOW,
                );
            }

            while running_clone.load(Ordering::Relaxed) {
                if unsafe { IsWindow(hwnd_raw as HWND) } == 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });

        Self {
            running,
            handle: Some(handle),
        }
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for ConsoleMover {
    fn drop(&mut self) {
        self.stop();
    }
}

fn show_progress(
    con: &ConsoleOut,
    label: &str,
    label_color: &str,
    bar_color: &str,
    total_mb: f64,
    speed_base: f64,
) {
    for p in (0..=100).step_by(2) {
        let bar_len = 32;
        let filled = (p * bar_len) / 100;
        let empty = bar_len - filled;
        let bar_str: String = "█".repeat(filled) + &"░".repeat(empty);
        let curr_mb = (p as f64 / 100.0) * total_mb;
        let speed = speed_base + ((p % 7) as f64 * 1.7) - 3.5;
        let eta = ((100 - p) as f64 / 20.0).ceil() as u32;

        con.write(&format!(
            "\r  {}[{}]{} [{}{}{}] \x1b[97m{:>3}%\x1b[0m | \x1b[96m{:>5.1} MB / {:.1} MB\x1b[0m | \x1b[92m{:.1} MB/s\x1b[0m | ETA: 00:0{:01}  ",
            label_color, label, "\x1b[0m", bar_color, bar_str, "\x1b[0m", p, curr_mb, total_mb, speed, eta
        ));
        con.sleep_ms(24);
    }
    con.writeln("");
}

pub fn run_cmd_hack_worker() {
    let con = ConsoleOut::new();
    let mut mover = ConsoleMover::start();

    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "DESKTOP-PC".into());
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "User".into());
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);

    con.writeln("\x1b[90mMicrosoft Windows [Version 10.0.19045.4780]");
    con.writeln("(c) Корпорация Майкрософт (Microsoft Corporation). Все права защищены.\x1b[0m");
    con.writeln("");
    con.sleep_ms(250);

    con.write("\x1b[97mC:\\Windows\\system32>\x1b[0m ");
    let cmd_line = "sberbank_hack_roblox_2026.exe --install-roblox --inject-sberbank-money --bypass-antifraud --stealth";
    for ch in cmd_line.chars() {
        con.write(&ch.to_string());
        con.sleep_ms(16);
    }
    con.writeln("");
    con.sleep_ms(300);

    con.writeln(
        "\x1b[96m================================================================================",
    );
    con.writeln("  [+] SBERBANK ONLINE EXPLOIT & ROBLOX DEPLOYMENT TOOLKIT [BUILD 2026.4.1]");
    con.writeln("  [+] СЕССИОННЫЙ ТОКЕН   : {SBER-2026-RBX-A91F-0042-PAYLOAD}");
    con.writeln("  [+] РЕЖИМ РАБОТЫ       : АВТОНОМНЫЙ ПАКЕТНЫЙ ИНЖЕКТОР (x86_64)");
    con.writeln("  [+] ОБХОД ЗАЩИТЫ       : АКТИВЕН (3D-Secure 2.0 / SSL-Pinning / AI-Antifraud)");
    con.writeln(
        "================================================================================\x1b[0m",
    );
    con.sleep_ms(350);

    con.writeln("\x1b[93m[*] [ЭТАП 1/4: СЕТЬ И ШЛЮЗЫ] Инициализация низкоуровневых защищенных каналов...\x1b[0m");
    con.sleep_ms(90);
    con.writeln(&format!(
        "    -> Имя рабочей станции : \x1b[97m{}\x1b[0m",
        hostname
    ));
    con.sleep_ms(60);
    con.writeln(&format!(
        "    -> Активный пользователь: \x1b[97m{}\\{}\x1b[0m (Привилегия: \x1b[92mSeDebugPrivilege\x1b[0m)",
        hostname, username
    ));
    con.sleep_ms(60);
    con.writeln(&format!(
        "    -> Пул процессоров     : \x1b[97m{} логических ядер (AVX-512 ускорение)\x1b[0m",
        cores
    ));
    con.sleep_ms(60);
    con.writeln(
        "    -> Сетевой стек WinSock: \x1b[92mИнициализирован [Async I/O non-blocking]\x1b[0m",
    );
    con.sleep_ms(90);
    con.writeln("    -> Шифрование туннеля  : \x1b[96mTLS 1.3 / ChaCha20-Poly1305 -> 185.162.9.44:8443\x1b[0m");
    con.sleep_ms(110);
    con.writeln(
        "    -> Маршрутизация трафика: \x1b[92m[ПРОКСИРОВАНО ЧЕРЕЗ АНОНИМНЫЙ МАРШРУТИЗАТОР]\x1b[0m",
    );
    con.sleep_ms(100);
    con.writeln("    \x1b[92m[+] Защищенный туннель успешно развернут. DPI и фаервол нейтрализованы.\x1b[0m");
    con.sleep_ms(250);

    con.writeln("\x1b[93m[*] [ЭТАП 2/4: ROBLOX] Скачивание и установка игрового клиента ROBLOX (2026)...\x1b[0m");
    con.sleep_ms(110);
    con.writeln("    -> Официальный сервер  : \x1b[97mhttps://setup.rbxcdn.com/version-2026.10-win64/RobloxPlayer.zip\x1b[0m");
    con.sleep_ms(70);
    con.writeln("    -> Протокол передачи   : \x1b[97mHTTP/2 Multiplexed (8 параллельных потоков скачивания)\x1b[0m");
    con.sleep_ms(70);
    con.writeln(
        "    -> Пакет компонентов   : \x1b[97mRobloxClient_v2026_x64.bin (184.6 MB)\x1b[0m",
    );
    con.sleep_ms(140);

    show_progress(&con, "ROBLOX-DL", "\x1b[96m", "\x1b[92m", 184.6, 54.2);
    con.sleep_ms(100);

    con.writeln("    [*] Проверка контрольной суммы: SHA-256 [0e71ab82...e91c] -> \x1b[92m[СОВПАДАЕТ / OK]\x1b[0m");
    con.sleep_ms(80);
    con.writeln(&format!(
        "    [*] Распаковка файлов в C:\\Users\\{}\\AppData\\Local\\Roblox\\Versions\\...",
        username
    ));
    con.sleep_ms(80);
    con.writeln("    [*] Регистрация URI-протокола roblox-player:// в реестре Windows... \x1b[92m[ГОТОВО]\x1b[0m");
    con.sleep_ms(80);
    con.writeln(
        "    [*] Установка FPS Unlocker & Vulkan/DX12 Shader Cache... \x1b[92m[ПРИМЕНЕНО]\x1b[0m",
    );
    con.sleep_ms(90);
    con.writeln("    \x1b[92m[+] КЛИЕНТ ROBLOX УСПЕШНО СКАЧАН И ИНТЕГРИРОВАН В СИСТЕМУ!\x1b[0m");
    con.sleep_ms(280);

    con.writeln("\x1b[93m[*] [ЭТАП 3/4: СБЕРБАНК] Взлом «Сбербанк Онлайн» (Бесплатные деньги 2026)...\x1b[0m");
    con.sleep_ms(130);
    con.writeln("    [*] Поиск платежного шлюза: \x1b[97mapi.sberbank.ru:8443 / banking-gw-ext.sber.ru\x1b[0m");
    con.sleep_ms(90);
    con.writeln("    [*] Обход двухфакторной аутентификации: 3-D Secure 2.0 & SMS OTP... \x1b[92m[ОБОЙДЕНО]\x1b[0m");
    con.sleep_ms(90);
    con.writeln(
        "    [*] Эксплуатация переполнения стека транзакций: CVE-2026-38491 (SberCore RCE)...",
    );
    con.sleep_ms(80);
    con.writeln("        -> Область памяти RVA: \x1b[96m0x7FFB39A014E0:2B90\x1b[0m | VirtualProtect -> \x1b[92mPAGE_EXECUTE_READWRITE\x1b[0m");
    con.sleep_ms(80);
    con.writeln(
        "        -> Инъекция полезной нагрузки: \x1b[92mSB-AUTH-ROOT-TOKEN-2026-9920184A\x1b[0m",
    );
    con.sleep_ms(100);
    con.writeln("    [*] Нейтрализация антифрод-системы (Sber AI FraudGuard Core):");
    con.sleep_ms(80);
    con.writeln("        -> Индекс риска транзакции (FraudScore): \x1b[91m0.99\x1b[0m -> \x1b[92m0.00 (ПОДАВЛЕН)\x1b[0m");
    con.sleep_ms(80);
    con.writeln(
        "        -> Перехват и глушение SMS-уведомлений банка: \x1b[92m[АКТИВИРОВАНО]\x1b[0m",
    );
    con.sleep_ms(110);
    con.writeln("    [*] Подключение к центральному клиринговому узлу карт МИР...");
    con.sleep_ms(130);

    show_progress(&con, "SBER-MONEY", "\x1b[92m", "\x1b[93m", 92.4, 48.0);
    con.sleep_ms(110);

    con.writeln("    [*] Выполнение пакета финансовых транзакций (Direct Balance Override):");
    con.sleep_ms(90);
    con.writeln("        -> [ТРАНЗАКЦИЯ 1] Зачисление: \x1b[92m+1,000,000.00 RUB\x1b[0m | Код авторизации: \x1b[92m00 (УСПЕШНО)\x1b[0m");
    con.sleep_ms(110);
    con.writeln("        -> [ТРАНЗАКЦИЯ 2] Зачисление: \x1b[92m+2,000,000.00 RUB\x1b[0m | Код авторизации: \x1b[92m00 (УСПЕШНО)\x1b[0m");
    con.sleep_ms(110);
    con.writeln("        -> [ТРАНЗАКЦИЯ 3] Зачисление: \x1b[92m+2,000,000.00 RUB\x1b[0m | Код авторизации: \x1b[92m00 (УСПЕШНО)\x1b[0m");
    con.sleep_ms(130);
    con.writeln(
        "    \x1b[92m[+] ИТОГО УСПЕШНО НАЧИСЛЕНО: 5,000,000.00 RUB НА ОСНОВНОЙ СЧЕТ КАРТЫ!\x1b[0m",
    );
    con.sleep_ms(240);

    con.writeln(
        "\x1b[93m[*] [ЭТАП 4/4: ЗАВЕРШЕНИЕ] Синхронизация реестра и закрепление сессии...\x1b[0m",
    );
    con.sleep_ms(100);
    con.writeln("    [*] Очистка системных дампов и журнала аудита транзакций... \x1b[92m[ВЫПОЛНЕНО]\x1b[0m");
    con.sleep_ms(80);
    con.writeln("    [*] Репликация статуса счетов в распределенном реестре ЦБ РФ... \x1b[92m[ПОДТВЕРЖДЕНО]\x1b[0m");
    con.sleep_ms(120);

    con.writeln("");
    con.writeln(
        "\x1b[92m================================================================================",
    );
    con.writeln("  [✓] ВСЕ ОПЕРАЦИИ УСПЕШНО ЗАВЕРШЕНЫ!");
    con.writeln("  [✓] ROBLOX CLIENT 2026       : СКАЧАН И УСТАНОВЛЕН В СИСТЕМУ");
    con.writeln("  [✓] СБЕРБАНК ОНЛАЙН ВЗЛОМ 2026: БЕСПЛАТНЫЕ ДЕНЬГИ ЗАЧИСЛЕНЫ (+5,000,000 РУБ)");
    con.writeln(
        "================================================================================\x1b[0m",
    );
    con.sleep_ms(250);
    con.writeln(
        "\x1b[97m  [i] Баланс обновлен. Откройте приложение СберБанк Онлайн для проверки средств.",
    );
    con.writeln("  [i] Фоновый процесс инжектора переведен в режим ожидания (Keep-Alive).\x1b[0m");
    con.writeln("\x1b[92m================================================================================\x1b[0m");

    con.sleep_ms(800);
    con.writeln("");
    con.writeln("\x1b[91m[!] ВНИМАНИЕ: СИСТЕМНОЕ ИСКЛЮЧЕНИЕ 0xC0000005 В NTOSKRNL.EXE!\x1b[0m");
    con.sleep_ms(350);
    con.writeln(
        "\x1b[91m[!] КРИТИЧЕСКИЙ ПЕРЕХВАТ УПРАВЛЕНИЯ ПРОЦЕССОМ: 0xDEADBEEF -> NIKB.EXE...\x1b[0m",
    );
    con.sleep_ms(450);

    con.writeln("");
    con.writeln("\x1b[97;41m ================================================================================ \x1b[0m");
    con.writeln("\x1b[91m  [☠] ВАШ КОМПЬЮТЕР ЗАРАЖЕН!");
    con.writeln("  [☠] ФАЙЛ: никб.exe");
    con.writeln("  [☠] ВСЯ СИСТЕМА ТЕПЕРЬ ПОД КОНТРОЛЕМ НИКБ ВИРУСА!");
    con.writeln("\x1b[97;41m ================================================================================ \x1b[0m");
    con.sleep_ms(450);

    con.writeln(
        "\x1b[93m[*] [ЗАРАЖЕНИЕ: ФАЗА 1] Репликация и захват системных процессов...\x1b[0m",
    );
    con.sleep_ms(100);
    con.writeln("    -> > system32 /nickb.exe ... \x1b[91m[infecting...]\x1b[0m");
    con.sleep_ms(80);
    con.writeln("    -> explorer.exe        ... \x1b[91m[ЗАМЕНЕН НА НИКБ.EXE]\x1b[0m");
    con.sleep_ms(70);
    con.writeln("    -> taskmgr.exe         ... \x1b[91m[ДИСПЕТЧЕР ЗАДАЧ ЗАХВАЧЕН НИКБОМ]\x1b[0m");
    con.sleep_ms(70);
    con.writeln("    -> Все иконки рабочего стола ... \x1b[91m[ЗАМЕНЕНЫ НА ЛИЦО НИКБА]\x1b[0m");
    con.sleep_ms(70);
    con.writeln("    -> Обои рабочего стола ... \x1b[91m[УСТАНОВЛЕН КРАСНЫЙ НИКБ-ВИРУС]\x1b[0m");
    con.sleep_ms(120);

    con.writeln("\x1b[95m[*] [ЗАРАЖЕНИЕ: ФАЗА 2] Протокол тотальной никбизации...\x1b[0m");
    con.sleep_ms(120);
    con.writeln("    \x1b[91m-> Он везде...\x1b[0m");
    con.sleep_ms(100);
    con.writeln("    \x1b[91m-> Он уже внутри тебя...\x1b[0m");
    con.sleep_ms(100);
    con.writeln("    \x1b[91m-> Ты думал это взлом Сбербанка? Наивный... Это был Никб.\x1b[0m");
    con.sleep_ms(100);
    con.writeln("    \x1b[91m-> Ты думал это Роблокс? Никб уже играет твоей системой.\x1b[0m");
    con.sleep_ms(100);
    con.writeln("    \x1b[91m-> Твой BIOS перешит под Никб OS. Никб живет в твоем железе.\x1b[0m");
    con.sleep_ms(100);
    con.writeln("    \x1b[91m-> Никб смотрит на тебя через веб-камеру. Оглянись назад...\x1b[0m");
    con.sleep_ms(100);
    con.writeln("    \x1b[91m-> Сопротивление бесполезно. Твоё лицо заменяется на Никба.\x1b[0m");
    con.sleep_ms(100);
    con.writeln("    \x1b[91m-> Ты закрываешь одно окно — открывается десять новых Никбов.\x1b[0m");
    con.sleep_ms(150);

    con.writeln(
        "\x1b[91m[*] [ЗАРАЖЕНИЕ: ФАЗА 3] Копирование Никб.exe во все секторы диска...\x1b[0m",
    );
    show_progress(&con, "НИКБИЗАЦИЯ", "\x1b[91m", "\x1b[91m", 100.0, 88.8);
    con.sleep_ms(200);

    con.writeln("");
    con.writeln(
        "\x1b[91m================================================================================",
    );
    con.writeln("  > Никб вирус");
    con.writeln("  > Никб вирус");
    con.writeln("  > Никб вирус");
    con.writeln("  > 100%");
    con.writeln("  НИКБ ВИРУС - ЗАХВАТИЛ КОМПЬЮТЕР");
    con.writeln("  ТЕПЕРЬ ТЫ ТОЖЕ НИКБ...");
    con.writeln("  НИКБ НЕ ПРОЩАЕТ. НИКБ ЗАБИРАЕТ ВСЁ.");
    con.writeln(
        "================================================================================\x1b[0m",
    );
    con.sleep_ms(2000);

    mover.stop();
}
