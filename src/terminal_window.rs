use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::Instant;

use rand::Rng;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::assets::{DecodedImage, FACE_BYTES};
use crate::pattern::MovementPattern;
use crate::state::WindowState;
use crate::window::{get_monitors, live_window_dec, live_window_inc};

const CLIENT_W: i32 = 470;
const CLIENT_H: i32 = 244;
const ROWS: usize = 10;
const TEXT_LEFT: i32 = 14;
const TEXT_TOP: i32 = 10;
const LINE_H: i32 = 22;
const PERCENT_ROW: usize = ROWS - 2;
const BANNER_ROW: usize = ROWS - 1;
const FACE_RECT: (f32, f32, f32, f32) = (250.0, 20.0, 204.0, 204.0);

const CHAR_MS: f64 = 38.0;
const PROGRESS_MS: f64 = 7500.0;
const HOLD_AFTER_BANNER_MS: f64 = 3000.0;
const FRAME_MS: u32 = 16;

static FACE: OnceLock<Option<DecodedImage>> = OnceLock::new();

fn face_image() -> Option<&'static DecodedImage> {
    FACE.get_or_init(|| DecodedImage::load_png(FACE_BYTES).ok())
        .as_ref()
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

type HGLRC = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct Pfd {
    n_size: u16,
    n_version: u16,
    dw_flags: u32,
    i_pixel_type: u8,
    c_color_bits: u8,
    c_red_bits: u8,
    c_red_shift: u8,
    c_green_bits: u8,
    c_green_shift: u8,
    c_blue_bits: u8,
    c_blue_shift: u8,
    c_alpha_bits: u8,
    c_alpha_shift: u8,
    c_accum_bits: u8,
    c_accum_red_bits: u8,
    c_accum_green_bits: u8,
    c_accum_blue_bits: u8,
    c_accum_alpha_bits: u8,
    c_depth_bits: u8,
    c_stencil_bits: u8,
    c_aux_buffers: u8,
    i_layer_type: u8,
    b_reserved: u8,
    dw_layer_mask: u32,
    dw_visible_mask: u32,
    dw_damage_mask: u32,
}

const PFD_DOUBLEBUFFER: u32 = 0x1;
const PFD_DRAW_TO_WINDOW: u32 = 0x4;
const PFD_SUPPORT_OPENGL: u32 = 0x20;

const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_RGBA: i32 = 0x1908;
const GL_BGRA: u32 = 0x80E1;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_NEAREST: i32 = 0x2600;
const GL_LINEAR: i32 = 0x2601;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;
const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_COMPILE_STATUS: u32 = 0x8B81;
const GL_LINK_STATUS: u32 = 0x8B82;
const GL_TEXTURE0: u32 = 0x84C0;

#[link(name = "opengl32")]
extern "system" {
    fn wglCreateContext(hdc: HDC) -> HGLRC;
    fn wglMakeCurrent(hdc: HDC, ctx: HGLRC) -> i32;
    fn wglDeleteContext(ctx: HGLRC) -> i32;
    fn wglGetProcAddress(name: *const u8) -> *const c_void;

    fn glGenTextures(n: i32, textures: *mut u32);
    fn glDeleteTextures(n: i32, textures: *const u32);
    fn glBindTexture(target: u32, texture: u32);
    fn glTexParameteri(target: u32, pname: u32, param: i32);
    fn glPixelStorei(pname: u32, param: i32);
    fn glTexImage2D(
        target: u32,
        level: i32,
        internal: i32,
        w: i32,
        h: i32,
        border: i32,
        format: u32,
        ty: u32,
        data: *const c_void,
    );
    fn glTexSubImage2D(
        target: u32,
        level: i32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        format: u32,
        ty: u32,
        data: *const c_void,
    );
    fn glViewport(x: i32, y: i32, w: i32, h: i32);
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(mask: u32);
    fn glRectf(x1: f32, y1: f32, x2: f32, y2: f32);
}

#[link(name = "gdi32")]
extern "system" {
    fn ChoosePixelFormat(hdc: HDC, pfd: *const Pfd) -> i32;
    fn SetPixelFormat(hdc: HDC, format: i32, pfd: *const Pfd) -> i32;
    fn SwapBuffers(hdc: HDC) -> i32;
}

struct GlFns {
    create_shader: unsafe extern "system" fn(u32) -> u32,
    shader_source: unsafe extern "system" fn(u32, i32, *const *const u8, *const i32),
    compile_shader: unsafe extern "system" fn(u32),
    get_shaderiv: unsafe extern "system" fn(u32, u32, *mut i32),
    delete_shader: unsafe extern "system" fn(u32),
    create_program: unsafe extern "system" fn() -> u32,
    attach_shader: unsafe extern "system" fn(u32, u32),
    link_program: unsafe extern "system" fn(u32),
    get_programiv: unsafe extern "system" fn(u32, u32, *mut i32),
    use_program: unsafe extern "system" fn(u32),
    delete_program: unsafe extern "system" fn(u32),
    get_uniform_location: unsafe extern "system" fn(u32, *const u8) -> i32,
    uniform1f: unsafe extern "system" fn(i32, f32),
    uniform1i: unsafe extern "system" fn(i32, i32),
    uniform2f: unsafe extern "system" fn(i32, f32, f32),
    uniform4f: unsafe extern "system" fn(i32, f32, f32, f32, f32),
    active_texture: unsafe extern "system" fn(u32),
}

unsafe fn proc_addr(name: &str) -> Option<*const c_void> {
    let cname = format!("{name}\0");
    let p = wglGetProcAddress(cname.as_ptr());
    match p as isize {
        -1 | 0 | 1 | 2 | 3 => None,
        _ => Some(p),
    }
}

macro_rules! gl_fn {
    ($name:literal) => {
        std::mem::transmute::<*const c_void, _>(proc_addr($name)?)
    };
}

unsafe fn load_gl() -> Option<GlFns> {
    Some(GlFns {
        create_shader: gl_fn!("glCreateShader"),
        shader_source: gl_fn!("glShaderSource"),
        compile_shader: gl_fn!("glCompileShader"),
        get_shaderiv: gl_fn!("glGetShaderiv"),
        delete_shader: gl_fn!("glDeleteShader"),
        create_program: gl_fn!("glCreateProgram"),
        attach_shader: gl_fn!("glAttachShader"),
        link_program: gl_fn!("glLinkProgram"),
        get_programiv: gl_fn!("glGetProgramiv"),
        use_program: gl_fn!("glUseProgram"),
        delete_program: gl_fn!("glDeleteProgram"),
        get_uniform_location: gl_fn!("glGetUniformLocation"),
        uniform1f: gl_fn!("glUniform1f"),
        uniform1i: gl_fn!("glUniform1i"),
        uniform2f: gl_fn!("glUniform2f"),
        uniform4f: gl_fn!("glUniform4f"),
        active_texture: gl_fn!("glActiveTexture"),
    })
}

const VERT_SRC: &str = r#"#version 120
void main() { gl_Position = gl_Vertex; }
"#;

const FRAG_SRC: &str = r#"#version 120
uniform sampler2D u_text;
uniform sampler2D u_face;
uniform vec2  u_res;
uniform float u_time;
uniform float u_prog;       
uniform vec4  u_face_rect;  

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i),                  hash(i + vec2(1.0, 0.0)), f.x),
               mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * noise(p); p *= 2.03; a *= 0.5; }
    return v;
}

float textMask(vec2 px) {
    vec3 c = texture2D(u_text, px / u_res).rgb;
    return max(c.r, max(c.g, c.b));
}

float inRect(vec2 uv) {
    return step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
}


vec2 faceUV(vec2 px) {
    vec2 uv = (px - u_face_rect.xy) / u_face_rect.zw;
    float amt   = 1.0 - u_prog * 0.8;
    float slice = floor(uv.y * 28.0);
    float tq    = floor(u_time * 14.0);
    float g = step(0.9, hash(vec2(slice, tq))) * (hash(vec2(slice + 3.1, tq)) - 0.5) * 0.18;
    uv.x += g * amt;
    uv.x += sin(uv.y * 40.0 + u_time * 9.0) * 0.004 * amt;
    return uv;
}


float dissolveField(vec2 uv) { return fbm(uv * 5.0 + vec2(0.0, u_time * 0.35)); }
float threshold() { return mix(-0.12, 1.12, u_prog); }

float faceMask(vec2 px) {
    vec2 uv = faceUV(px);
    float n = dissolveField(uv);
    return texture2D(u_face, uv).a * inRect(uv) * smoothstep(n - 0.035, n + 0.035, threshold());
}

void main() {
    vec2 px = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y);

    
    vec2 fc = u_face_rect.xy + u_face_rect.zw * 0.5;
    float d = length((px - fc) / (u_face_rect.zw * 0.8));
    float streak = pow(noise(vec2(px.x * 0.015 - u_time * 0.6, px.y * 0.8)), 6.0);
    vec3 col = vec3(0.012, 0.0, 0.0)
             + vec3(0.6, 0.02, 0.02) * streak * smoothstep(1.4, 0.2, d) * u_prog;

    
    vec2 uv = faceUV(px);
    float inside = inRect(uv);
    float n = dissolveField(uv);
    float t = threshold();
    float m = smoothstep(n - 0.035, n + 0.035, t);

    float ca = 0.012 * (1.0 - u_prog * 0.7);           
    vec4  f  = texture2D(u_face, uv);
    float fr = texture2D(u_face, uv + vec2(ca, 0.0)).r;
    float fb = texture2D(u_face, uv - vec2(ca, 0.0)).b;
    vec3 fcol = vec3(fr, f.g, fb);
    float lum = dot(fcol, vec3(0.299, 0.587, 0.114));
    vec3 graded = mix(fcol, vec3(lum) * vec3(1.35, 0.35, 0.3), 0.35);
    float holo = 0.8 + 0.2 * sin(px.y * 1.6 - u_time * 18.0); 
    graded *= mix(holo, 1.0, u_prog);

    col = mix(col, graded, f.a * inside * m);

    
    float edge = (1.0 - smoothstep(0.0, 0.06, abs(t - n))) * (1.0 - step(0.999, u_prog));
    col += vec3(1.0, 0.18, 0.05) * edge * f.a * inside * 1.8;

    
    float tm = textMask(px);
    col = mix(col, vec3(1.0, 0.12, 0.1), tm);
    col += vec3(1.0, 0.6, 0.5) * pow(tm, 3.0) * 0.25;

    
    float glow = 0.0;
    for (int i = 0; i < 8; i++) {
        float ang = float(i) * 0.785398;
        vec2 dir = vec2(cos(ang), sin(ang));
        glow += textMask(px + dir * 2.5) * 0.9 + faceMask(px + dir * 4.0)  * 0.35;
        glow += textMask(px + dir * 6.0) * 0.5 + faceMask(px + dir * 10.0) * 0.25;
    }
    glow /= 16.0;
    col += vec3(1.0, 0.05, 0.03) * glow * 1.7 * (0.85 + 0.15 * sin(u_time * 3.0));

    
    col *= 0.86 + 0.14 * sin(px.y * 3.14159);
    col += (hash(px + fract(u_time)) - 0.5) * 0.025;
    vec2 q = px / u_res - 0.5;
    col *= 1.0 - dot(q, q) * 0.9;
    col *= 0.97 + 0.03 * sin(u_time * 60.0);

    gl_FragColor = vec4(max(col, 0.0), 1.0);
}
"#;

unsafe fn compile(gl: &GlFns, kind: u32, src: &str) -> Option<u32> {
    let sh = (gl.create_shader)(kind);
    let ptr = src.as_ptr();
    let len = src.len() as i32;
    (gl.shader_source)(sh, 1, &ptr, &len);
    (gl.compile_shader)(sh);
    let mut ok = 0;
    (gl.get_shaderiv)(sh, GL_COMPILE_STATUS, &mut ok);
    if ok == 0 {
        (gl.delete_shader)(sh);
        return None;
    }
    Some(sh)
}

unsafe fn build_program(gl: &GlFns) -> Option<u32> {
    let vs = compile(gl, GL_VERTEX_SHADER, VERT_SRC)?;
    let fs = compile(gl, GL_FRAGMENT_SHADER, FRAG_SRC)?;
    let prog = (gl.create_program)();
    (gl.attach_shader)(prog, vs);
    (gl.attach_shader)(prog, fs);
    (gl.link_program)(prog);
    (gl.delete_shader)(vs);
    (gl.delete_shader)(fs);
    let mut ok = 0;
    (gl.get_programiv)(prog, GL_LINK_STATUS, &mut ok);
    if ok == 0 {
        (gl.delete_program)(prog);
        return None;
    }
    Some(prog)
}

#[derive(Clone, Copy)]
enum Step {
    Wait(f64),
    Type(usize, &'static str),
    ShowPercent,
    StartProgress,
}

fn script() -> Vec<Step> {
    use Step::*;
    vec![
        Wait(200.0),
        Type(0, "> system32 /nickb.exe"),
        Wait(300.0),
        ShowPercent,
        Wait(250.0),
        Type(1, "> infecting..."),
        StartProgress,
        Wait(300.0),
        Type(2, "> Никб вирус"),
        Wait(550.0),
        Type(3, "> Никб вирус"),
        Wait(550.0),
        Type(4, "> Никб вирус"),
        Wait(550.0),
        Type(5, "> Никб вирус"),
        Wait(550.0),
        Type(6, "> Никб вирус"),
    ]
}

const BANNER: &str = "НИКБ ВИРУС - ЗАХВАТИЛ КОМПЬЮТЕР";

struct Gpu {
    hdc: HDC,
    ctx: HGLRC,
    gl: GlFns,
    program: u32,
    tex_text: u32,
    tex_face: u32,
    u_text: i32,
    u_face: i32,
    u_res: i32,
    u_time: i32,
    u_prog: i32,
    u_face_rect: i32,
}

struct TextLayer {
    dc: HDC,
    bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    bits: *mut c_void,
    font: HFONT,
    old_font: HGDIOBJ,
}

struct TermData {
    gpu: Option<Gpu>,
    layer: Option<TextLayer>,

    steps: Vec<Step>,
    step_idx: usize,
    step_ms: f64,

    lines: [String; ROWS],
    typing_row: Option<usize>,
    percent_visible: bool,
    progress_on: bool,
    progress: f64,

    banner_ms: f64,
    hold_ms: f64,

    start: Instant,
    last: Instant,

    state: WindowState,
    pattern: MovementPattern,
}

fn typed_prefix(text: &str, ms: f64) -> (String, bool) {
    let total = text.chars().count();
    let n = ((ms / CHAR_MS).floor() as usize).min(total);
    (text.chars().take(n).collect(), n == total)
}

impl TermData {
    fn update<R: Rng>(&mut self, dt: f64, rng: &mut R) -> bool {
        if self.progress_on && self.progress < 100.0 {
            let jitter = rng.gen_range(0.3..1.7);
            self.progress = (self.progress + dt * (100.0 / PROGRESS_MS) * jitter).min(100.0);
        }
        if self.percent_visible {
            self.lines[PERCENT_ROW] = format!("> {}%", self.progress.floor() as i32);
        }

        let mut budget = dt;
        while budget > 0.0 && self.step_idx < self.steps.len() {
            match self.steps[self.step_idx] {
                Step::Wait(ms) => {
                    let need = ms - self.step_ms;
                    if budget >= need {
                        budget -= need;
                        self.next_step();
                    } else {
                        self.step_ms += budget;
                        budget = 0.0;
                    }
                }
                Step::Type(row, text) => {
                    self.typing_row = Some(row);
                    self.step_ms += budget;
                    let (s, done) = typed_prefix(text, self.step_ms);
                    self.lines[row] = s;
                    if done {
                        let used = text.chars().count() as f64 * CHAR_MS;
                        budget = (self.step_ms - used).max(0.0);
                        self.typing_row = None;
                        self.next_step();
                    } else {
                        budget = 0.0;
                    }
                }
                Step::ShowPercent => {
                    self.percent_visible = true;
                    self.lines[PERCENT_ROW] = "> 0%".to_string();
                    self.next_step();
                }
                Step::StartProgress => {
                    self.progress_on = true;
                    self.next_step();
                }
            }
        }

        if self.step_idx >= self.steps.len() && self.progress >= 100.0 {
            self.banner_ms += dt;
            let (s, done) = typed_prefix(BANNER, self.banner_ms);
            self.lines[BANNER_ROW] = s;
            if done {
                self.typing_row = None;
                self.hold_ms += dt;
                if self.hold_ms >= HOLD_AFTER_BANNER_MS {
                    return false;
                }
            } else {
                self.typing_row = Some(BANNER_ROW);
            }
        }
        true
    }

    fn next_step(&mut self) {
        self.step_idx += 1;
        self.step_ms = 0.0;
    }
}

unsafe fn create_text_layer() -> Option<TextLayer> {
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    if dc.is_null() {
        return None;
    }
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = CLIENT_W;
    bmi.bmiHeader.biHeight = -CLIENT_H;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if bmp.is_null() || bits.is_null() {
        DeleteDC(dc);
        return None;
    }
    let old_bmp = SelectObject(dc, bmp);

    let face_name = to_wide("Consolas");
    let font = CreateFontW(
        -17,
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
        ANTIALIASED_QUALITY as u32,
        FIXED_PITCH as u32,
        face_name.as_ptr(),
    );
    let old_font = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, 0x00FF_FFFF);

    Some(TextLayer {
        dc,
        bmp,
        old_bmp,
        bits,
        font,
        old_font,
    })
}

unsafe fn draw_text_layer(layer: &TextLayer, data: &TermData, elapsed_ms: f64) {
    let rc = RECT {
        left: 0,
        top: 0,
        right: CLIENT_W,
        bottom: CLIENT_H,
    };
    FillRect(layer.dc, &rc, GetStockObject(BLACK_BRUSH as i32));

    let cursor_on = ((elapsed_ms / 400.0) as i64) % 2 == 0;
    for (row, line) in data.lines.iter().enumerate() {
        let mut s = line.clone();
        if data.typing_row == Some(row) && cursor_on {
            s.push('_');
        }
        if s.is_empty() {
            continue;
        }
        let w: Vec<u16> = s.encode_utf16().collect();
        TextOutW(
            layer.dc,
            TEXT_LEFT,
            TEXT_TOP + row as i32 * LINE_H,
            w.as_ptr(),
            w.len() as i32,
        );
    }
    GdiFlush();
}

unsafe fn destroy_text_layer(layer: TextLayer) {
    SelectObject(layer.dc, layer.old_font);
    SelectObject(layer.dc, layer.old_bmp);
    DeleteObject(layer.font);
    DeleteObject(layer.bmp);
    DeleteDC(layer.dc);
}

unsafe fn create_gpu(hwnd: HWND) -> Option<Gpu> {
    let hdc = GetDC(hwnd);
    if hdc.is_null() {
        return None;
    }
    let pfd = Pfd {
        n_size: std::mem::size_of::<Pfd>() as u16,
        n_version: 1,
        dw_flags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
        i_pixel_type: 0,
        c_color_bits: 32,
        c_alpha_bits: 8,
        ..Default::default()
    };
    let fmt = ChoosePixelFormat(hdc, &pfd);
    if fmt == 0 || SetPixelFormat(hdc, fmt, &pfd) == 0 {
        ReleaseDC(hwnd, hdc);
        return None;
    }
    let ctx = wglCreateContext(hdc);
    if ctx.is_null() {
        ReleaseDC(hwnd, hdc);
        return None;
    }
    wglMakeCurrent(hdc, ctx);

    let fail = |hdc: HDC, ctx: HGLRC| {
        wglMakeCurrent(std::ptr::null_mut(), std::ptr::null_mut());
        wglDeleteContext(ctx);
        ReleaseDC(hwnd, hdc);
    };

    let Some(gl) = load_gl() else {
        fail(hdc, ctx);
        return None;
    };
    let Some(program) = build_program(&gl) else {
        fail(hdc, ctx);
        return None;
    };

    glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
    let mut tex = [0u32; 2];
    glGenTextures(2, tex.as_mut_ptr());

    glBindTexture(GL_TEXTURE_2D, tex[0]);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    glTexImage2D(
        GL_TEXTURE_2D,
        0,
        GL_RGBA,
        CLIENT_W,
        CLIENT_H,
        0,
        GL_BGRA,
        GL_UNSIGNED_BYTE,
        std::ptr::null(),
    );

    glBindTexture(GL_TEXTURE_2D, tex[1]);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    if let Some(img) = face_image() {
        glTexImage2D(
            GL_TEXTURE_2D,
            0,
            GL_RGBA,
            img.width,
            img.height,
            0,
            GL_BGRA,
            GL_UNSIGNED_BYTE,
            img.bgra.as_ptr() as *const c_void,
        );
    } else {
        let blank = [0u8; 4];
        glTexImage2D(
            GL_TEXTURE_2D,
            0,
            GL_RGBA,
            1,
            1,
            0,
            GL_BGRA,
            GL_UNSIGNED_BYTE,
            blank.as_ptr() as *const c_void,
        );
    }

    let loc = |name: &str| {
        let c = format!("{name}\0");
        (gl.get_uniform_location)(program, c.as_ptr())
    };
    let u_text = loc("u_text");
    let u_face = loc("u_face");
    let u_res = loc("u_res");
    let u_time = loc("u_time");
    let u_prog = loc("u_prog");
    let u_face_rect = loc("u_face_rect");

    Some(Gpu {
        hdc,
        ctx,
        gl,
        program,
        tex_text: tex[0],
        tex_face: tex[1],
        u_text,
        u_face,
        u_res,
        u_time,
        u_prog,
        u_face_rect,
    })
}

unsafe fn render(gpu: &Gpu, layer: &TextLayer, time_s: f32, prog: f32) {
    wglMakeCurrent(gpu.hdc, gpu.ctx);
    let gl = &gpu.gl;

    (gl.active_texture)(GL_TEXTURE0);
    glBindTexture(GL_TEXTURE_2D, gpu.tex_text);
    glTexSubImage2D(
        GL_TEXTURE_2D,
        0,
        0,
        0,
        CLIENT_W,
        CLIENT_H,
        GL_BGRA,
        GL_UNSIGNED_BYTE,
        layer.bits,
    );
    (gl.active_texture)(GL_TEXTURE0 + 1);
    glBindTexture(GL_TEXTURE_2D, gpu.tex_face);

    glViewport(0, 0, CLIENT_W, CLIENT_H);
    glClearColor(0.0, 0.0, 0.0, 1.0);
    glClear(GL_COLOR_BUFFER_BIT);

    (gl.use_program)(gpu.program);
    (gl.uniform1i)(gpu.u_text, 0);
    (gl.uniform1i)(gpu.u_face, 1);
    (gl.uniform2f)(gpu.u_res, CLIENT_W as f32, CLIENT_H as f32);
    (gl.uniform1f)(gpu.u_time, time_s);
    (gl.uniform1f)(gpu.u_prog, prog);
    let (fx, fy, fw, fh) = FACE_RECT;
    (gl.uniform4f)(gpu.u_face_rect, fx, fy, fw, fh);

    glRectf(-1.0, -1.0, 1.0, 1.0);
    SwapBuffers(gpu.hdc);
}

unsafe fn destroy_gpu(hwnd: HWND, gpu: Gpu) {
    wglMakeCurrent(gpu.hdc, gpu.ctx);
    (gpu.gl.delete_program)(gpu.program);
    let tex = [gpu.tex_text, gpu.tex_face];
    glDeleteTextures(2, tex.as_ptr());
    wglMakeCurrent(std::ptr::null_mut(), std::ptr::null_mut());
    wglDeleteContext(gpu.ctx);
    ReleaseDC(hwnd, gpu.hdc);
}

const CLASS_NAME: &str = "NikbvirusTerminalWindowClass";

pub unsafe fn register_terminal_window_class(hinstance: HMODULE) -> bool {
    let class_name = to_wide(CLASS_NAME);
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_OWNDC | CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(terminal_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: LoadIconW(std::ptr::null_mut(), IDI_WARNING),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: GetStockObject(BLACK_BRUSH as i32),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: LoadIconW(std::ptr::null_mut(), IDI_WARNING),
    };
    RegisterClassExW(&wc) != 0
}

pub unsafe fn spawn_terminal_window(hinstance: HMODULE) {
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

    let init_x = monitor.x + rng.gen_range(0..(monitor.w - win_w).max(1));
    let init_y = monitor.y + rng.gen_range(0..(monitor.h - win_h).max(1));

    let speed = 1.5 + rng.gen::<f64>() * 3.0;
    let mut state = WindowState::new(init_x as f64, init_y as f64, win_w, win_h, speed);
    let pattern = MovementPattern::random(&mut rng);
    pattern.init(&mut state, &mut rng);

    let now = Instant::now();
    let data = Box::new(TermData {
        gpu: None,
        layer: None,
        steps: script(),
        step_idx: 0,
        step_ms: 0.0,
        lines: Default::default(),
        typing_row: None,
        percent_visible: false,
        progress_on: false,
        progress: 0.0,
        banner_ms: 0.0,
        hold_ms: 0.0,
        start: now,
        last: now,
        state,
        pattern,
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
        Box::into_raw(data) as *const c_void,
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

unsafe fn data_of(hwnd: HWND) -> *mut TermData {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TermData
}

unsafe fn frame(hwnd: HWND, data: &mut TermData) {
    if let (Some(gpu), Some(layer)) = (data.gpu.as_ref(), data.layer.as_ref()) {
        let elapsed = data.start.elapsed().as_secs_f64() * 1000.0;
        draw_text_layer(layer, data, elapsed);
        render(
            gpu,
            layer,
            (elapsed / 1000.0) as f32,
            (data.progress / 100.0) as f32,
        );
    }
    let _ = hwnd;
}

pub unsafe extern "system" fn terminal_window_proc(
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
            let ptr = data_of(hwnd);
            if ptr.is_null() {
                return -1;
            }
            let data = &mut *ptr;
            data.layer = create_text_layer();
            data.gpu = create_gpu(hwnd);
            if data.gpu.is_none() || data.layer.is_none() {
                return -1;
            }
            SetTimer(hwnd, 1, FRAME_MS, None);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            BeginPaint(hwnd, &mut ps);
            let ptr = data_of(hwnd);
            if !ptr.is_null() {
                frame(hwnd, &mut *ptr);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            let ptr = data_of(hwnd);
            if ptr.is_null() {
                return 0;
            }
            let data = &mut *ptr;
            let now = Instant::now();
            let dt = (now - data.last).as_secs_f64() * 1000.0;
            data.last = now;

            let mut rng = rand::thread_rng();
            if !data.update(dt.min(100.0), &mut rng) {
                DestroyWindow(hwnd);
                return 0;
            }

            data.state.tick += 1;
            data.pattern.step(&mut data.state, &mut rng);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                data.state.x.round() as i32,
                data.state.y.round() as i32,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            );

            frame(hwnd, data);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_NCDESTROY => {
            KillTimer(hwnd, 1);
            let ptr = data_of(hwnd);
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let mut data = Box::from_raw(ptr);
                let created = data.gpu.is_some() && data.layer.is_some();
                if let Some(gpu) = data.gpu.take() {
                    destroy_gpu(hwnd, gpu);
                }
                if let Some(layer) = data.layer.take() {
                    destroy_text_layer(layer);
                }
                if created {
                    live_window_dec();
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
