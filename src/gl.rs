//! Shaders, materials, sprite textures and the post-processing chain.
//!
//! The look is a storybook one. Surfaces get toon lighting: a soft-edged
//! light side, lavender shadows and a thin rim of light. Characters and props
//! get an ink outline drawn as an inverted hull, a constant number of pixels
//! wide. The finished frame gets gentle bloom, a paper texture and darker
//! edges, like a watercolour page.
//!
//! Shaders are written once in GLSL ES 1.00 style and given a prefix: on
//! Gizlet OS raylib runs OpenGL ES 2, on the desktop OpenGL 3.3 core.

use crate::math::{Rng, V3};
use raylib_sys::{self as ffi, Color, Material, Matrix, Mesh, RenderTexture2D, Shader, Texture2D};
use std::cell::{Cell, RefCell};
use std::ffi::CString;

const GLES: bool = cfg!(feature = "gizlet-os");

pub const W: i32 = 640;
pub const H: i32 = 480;

pub const fn rgb(hex: u32) -> Color {
    Color { r: (hex >> 16) as u8, g: (hex >> 8) as u8, b: hex as u8, a: 255 }
}

pub fn rgba(c: Color, a: f32) -> Color {
    Color { a: (a.clamp(0.0, 1.0) * c.a as f32) as u8, ..c }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) }
}

pub fn lighten(c: Color, t: f32) -> Color {
    mix(c, Color { a: c.a, ..rgb(0xFFFFFF) }, t)
}

pub fn scale(c: Color, k: f32) -> Color {
    let s = |x: u8| (x as f32 * k).clamp(0.0, 255.0) as u8;
    Color { r: s(c.r), g: s(c.g), b: s(c.b), a: c.a }
}

/// Componentwise product, as light falling on a surface.
pub fn modulate(c: Color, light: Color) -> Color {
    let m = |a: u8, b: u8| ((a as u32 * b as u32) / 255) as u8;
    Color { r: m(c.r, light.r), g: m(c.g, light.g), b: m(c.b, light.b), a: c.a }
}

pub fn vec3(c: Color) -> [f32; 3] {
    [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0]
}

fn source(vertex: bool, body: &str) -> CString {
    // The shaders are written in GLSL ES 1.00 style. They are compiled as the
    // language of the context raylib made, because a program cannot mix
    // versions, and raylib's own default shaders (paired with ours in the
    // full-screen passes) follow its context: ES 2, ES 3 or desktop GL 3.3.
    let es3 = unsafe { ffi::rlGetVersion() } == ffi::rlGlVersion::RL_OPENGL_ES_30 as i32;
    let head = match (GLES, es3, vertex) {
        (true, false, true) => "#version 100\n",
        (true, false, false) => concat!(
            "#version 100\n#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n",
            "#define FRAG gl_FragColor\n"
        ),
        (true, true, true) => "#version 300 es\n#define attribute in\n#define varying out\n#define texture2D texture\n",
        (true, true, false) => concat!(
            "#version 300 es\nprecision highp float;\n#define varying in\n#define texture2D texture\n",
            "out vec4 fragOut;\n#define FRAG fragOut\n"
        ),
        (false, _, true) => "#version 330\n#define attribute in\n#define varying out\n#define texture2D texture\n",
        (false, _, false) => "#version 330\n#define varying in\n#define texture2D texture\nout vec4 fragOut;\n#define FRAG fragOut\n",
    };
    CString::new(format!("{head}{body}")).expect("shader source")
}

/// Loads a shader; `vs` None keeps raylib's default vertex shader (for
/// full-screen passes drawn as textures).
fn load(vs: Option<&str>, fs: &str) -> Shader {
    let vs = vs.map(|s| source(true, s));
    let fs = source(false, fs);
    let sh = unsafe { ffi::LoadShaderFromMemory(vs.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()), fs.as_ptr()) };
    if sh.id == unsafe { ffi::rlGetShaderIdDefault() } {
        eprintln!("littleprince: a shader failed to compile; using raylib's default");
    }
    sh
}

/// A shader and the uniform locations asked for so far.
pub struct Uniforms {
    pub shader: Shader,
    locs: RefCell<Vec<(&'static str, i32)>>,
}

impl Uniforms {
    fn new(shader: Shader) -> Uniforms {
        Uniforms { shader, locs: RefCell::new(Vec::new()) }
    }
    pub fn loc(&self, name: &'static str) -> i32 {
        if let Some(&(_, l)) = self.locs.borrow().iter().find(|(n, _)| *n == name) {
            return l;
        }
        let c = CString::new(name).unwrap();
        let l = unsafe { ffi::GetShaderLocation(self.shader, c.as_ptr()) };
        self.locs.borrow_mut().push((name, l));
        l
    }
    fn set(&self, name: &'static str, v: *const f32, kind: ffi::ShaderUniformDataType) {
        let l = self.loc(name);
        if l >= 0 {
            unsafe { ffi::SetShaderValue(self.shader, l, v as _, kind as i32) };
        }
    }
    pub fn f(&self, name: &'static str, v: f32) {
        self.set(name, &v, ffi::ShaderUniformDataType::SHADER_UNIFORM_FLOAT);
    }
    pub fn v2(&self, name: &'static str, v: [f32; 2]) {
        self.set(name, v.as_ptr(), ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC2);
    }
    pub fn v3(&self, name: &'static str, v: [f32; 3]) {
        self.set(name, v.as_ptr(), ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC3);
    }
    pub fn tex(&self, name: &'static str, t: Texture2D) {
        let l = self.loc(name);
        if l >= 0 {
            unsafe { ffi::SetShaderValueTexture(self.shader, l, t) };
        }
    }
}

const LIGHTING_GLSL: &str = r#"
vec3 lamp_light(vec3 base, vec3 n, vec3 l, vec3 color) {
    float ld = length(l);
    float lamp = max(0.0, 1.0 - ld / 2.3);
    return base * color * lamp * lamp * (0.35 + 0.65 * max(dot(n, l / max(ld, 0.001)), 0.0));
}

float fog_factor(vec3 delta, vec2 range) {
    return clamp((length(delta) - range.x) / (range.y - range.x), 0.0, 1.0);
}
"#;

const LIT_VS: &str = r#"
attribute vec3 vertexPosition;
attribute vec3 vertexNormal;
attribute vec4 vertexColor;
uniform mat4 mvp;
uniform mat4 matModel;
varying vec3 vN;
varying vec3 vW;
varying vec4 vC;
void main() {
    vW = (matModel * vec4(vertexPosition, 1.0)).xyz;
    vN = (matModel * vec4(vertexNormal, 0.0)).xyz;
    vC = vertexColor;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
"#;

const LIT_FS: &str = r#"
varying vec3 vN;
varying vec3 vW;
varying vec4 vC;
uniform vec4 colDiffuse;
uniform vec3 sunDir;
uniform vec3 sunCol;
uniform vec3 shadeCol;
uniform vec3 skyCol;
uniform vec3 camPos;
uniform vec3 rimCol;
uniform vec3 fogCol;
uniform vec3 lampPos;
uniform vec3 lampCol;
uniform vec2 fogRange;
uniform float emissive;
uniform float fresnel;
void main() {
    vec3 n = normalize(vN);
    vec3 up = normalize(vW);
    vec3 base = vC.rgb * colDiffuse.rgb;
    float ndl = dot(n, sunDir);
    float lit = smoothstep(-0.02, 0.07, ndl);
    float day = smoothstep(-0.28, 0.18, dot(up, sunDir));
    vec3 light = mix(shadeCol, sunCol * 0.9, lit * day) + skyCol * (0.06 + 0.1 * (dot(n, up) * 0.5 + 0.5));
    vec3 col = base * light;
    col += lamp_light(base, n, lampPos - vW, lampCol);
    vec3 v = normalize(camPos - vW);
    float rim = 1.0 - max(dot(n, v), 0.0);
    col += rimCol * smoothstep(0.62, 0.74, rim) * lit * day * 0.22 * vC.a;
    col += base * emissive;
    float a = colDiffuse.a * vC.a;
    if (fresnel > 0.0) {
        float fr = rim * rim;
        col += vec3(1.0) * fresnel * fr * rim;
        a = clamp(a + fresnel * fr, 0.0, 1.0);
    }
    float f = fog_factor(camPos - vW, fogRange);
    col = mix(col, fogCol, f * 0.4);
    FRAG = vec4(col, a);
}
"#;

/// The same lighting, worked out per vertex: for the large surfaces (ground,
/// grass, props), where per-pixel lighting would cost most of the frame on a
/// Mali-G31 and the dense meshes keep the shading smooth anyway.
const LITV_VS: &str = r#"
attribute vec3 vertexPosition;
attribute vec3 vertexNormal;
attribute vec2 vertexTexCoord;
attribute vec4 vertexColor;
uniform mat4 mvp;
uniform mat4 matModel;
uniform float time;
uniform vec3 pusher;
uniform vec3 wind;
uniform vec3 sunDir;
uniform vec3 sunCol;
uniform vec3 shadeCol;
uniform vec3 skyCol;
uniform vec3 camPos;
uniform vec3 fogCol;
uniform vec3 lampPos;
uniform vec3 lampCol;
uniform vec2 fogRange;
varying vec4 vC;
void main() {
    vec3 w = (matModel * vec4(vertexPosition, 1.0)).xyz;
    vec3 off = vec3(0.0);
    float k = vertexTexCoord.x;
    if (k > 0.0) {
        float s = sin(time * 2.3 + vertexTexCoord.y * 6.2831 + dot(w, vec3(0.9, 0.4, 0.7)));
        float g = sin(time * 0.7 + dot(w, vec3(0.2, 0.5, 0.3))) * 0.5 + 0.5;
        off = wind * (0.03 + 0.06 * s * g) * k;
        vec3 d = w - pusher;
        float dl = length(d);
        off += d / max(dl, 0.001) * k * 0.2 * max(0.0, 1.0 - dl / 0.5);
        vec3 upw = normalize(w);
        off -= upw * dot(off, upw) * 0.6;
    }
    w += off;
    vec3 n = normalize((matModel * vec4(vertexNormal, 0.0)).xyz);
    vec3 up = normalize(w);
    vec3 base = vertexColor.rgb;
    float lit = smoothstep(-0.02, 0.07, dot(n, sunDir));
    float day = smoothstep(-0.28, 0.18, dot(up, sunDir));
    vec3 col = base * (mix(shadeCol, sunCol * 0.9, lit * day) + skyCol * (0.06 + 0.1 * (dot(n, up) * 0.5 + 0.5)));
    // Scenery has no rim light, and lamp light only while the lamp burns.
    if (lampCol.r > 0.0) {
        col += lamp_light(base, n, lampPos - w, lampCol);
    }
    float f = fog_factor(camPos - w, fogRange);
    vC = vec4(mix(col, fogCol, f * f * 0.85), vertexColor.a);
    gl_Position = mvp * vec4(vertexPosition + off, 1.0);
}
"#;

const LITV_FS: &str = r#"
varying vec4 vC;
void main() {
    FRAG = vC;
}
"#;

/// Batched parts: a mesh holding RIG_N copies of a unit shape, each copy
/// tagged with its index in `u`. Every copy reads its placement (three rows of
/// an affine matrix), colour and glow from `bones`, so up to RIG_N parts of
/// one shape draw in a single call. On the handheld's GPU driver each draw
/// call costs about 0.15 ms, so this turns some 200 draws into about 12.
pub const RIG_N: usize = 64;

const RIG_HEAD: &str = r#"
attribute vec3 vertexPosition;
attribute vec3 vertexNormal;
attribute vec2 vertexTexCoord;
uniform mat4 mvp;
uniform vec4 bones[256];
"#;

const RIG_VS: &str = r#"
varying vec3 vN;
varying vec3 vW;
varying vec4 vC;
void main() {
    int i = int(vertexTexCoord.x + 0.5) * 4;
    vec4 r0 = bones[i];
    vec4 r1 = bones[i + 1];
    vec4 r2 = bones[i + 2];
    vec3 p = vertexPosition;
    vec3 w = vec3(dot(r0.xyz, p) + r0.w, dot(r1.xyz, p) + r1.w, dot(r2.xyz, p) + r2.w);
    vec3 a = vec3(r0.x, r1.x, r2.x);
    vec3 b = vec3(r0.y, r1.y, r2.y);
    vec3 c = vec3(r0.z, r1.z, r2.z);
    vec3 n = vertexNormal;
    vN = (cross(b, c) * n.x + cross(c, a) * n.y + cross(a, b) * n.z) * sign(dot(a, cross(b, c)));
    vW = w;
    vC = bones[i + 3];
    gl_Position = mvp * vec4(w, 1.0);
}
"#;

const RIG_FS: &str = r#"
varying vec3 vN;
varying vec3 vW;
varying vec4 vC;
uniform vec3 sunDir;
uniform vec3 sunCol;
uniform vec3 shadeCol;
uniform vec3 skyCol;
uniform vec3 camPos;
uniform vec3 rimCol;
uniform vec3 fogCol;
uniform vec3 lampPos;
uniform vec3 lampCol;
uniform vec2 fogRange;
void main() {
    vec3 n = normalize(vN);
    vec3 up = normalize(vW);
    vec3 base = vC.rgb;
    float lit = smoothstep(-0.02, 0.07, dot(n, sunDir));
    float day = smoothstep(-0.28, 0.18, dot(up, sunDir));
    vec3 col = base * (mix(shadeCol, sunCol * 0.9, lit * day) + skyCol * (0.06 + 0.1 * (dot(n, up) * 0.5 + 0.5)));
    col += lamp_light(base, n, lampPos - vW, lampCol);
    float rim = 1.0 - max(dot(n, normalize(camPos - vW)), 0.0);
    col += rimCol * smoothstep(0.62, 0.74, rim) * lit * day * 0.22;
    col += base * vC.a;
    float f = fog_factor(camPos - vW, fogRange);
    FRAG = vec4(mix(col, fogCol, f * 0.4), 1.0);
}
"#;

const OUTLINE_GLSL: &str = r#"
vec4 expand_outline(vec4 c, vec4 c2, float width) {
    vec2 s = c2.xy / c2.w - c.xy / c.w;
    float l = length(s);
    s = l > 0.000001 ? s / l : vec2(0.0);
    c.xy += s * width * vec2(2.0 / 640.0, 2.0 / 480.0) * c.w;
    return c;
}
"#;

const RIG_OUTLINE_VS: &str = r#"
uniform float width;
void main() {
    int i = int(vertexTexCoord.x + 0.5) * 4;
    vec4 r0 = bones[i];
    vec4 r1 = bones[i + 1];
    vec4 r2 = bones[i + 2];
    vec3 p = vertexPosition;
    vec3 d = vec3(dot(r0.xyz, p), dot(r1.xyz, p), dot(r2.xyz, p));
    vec3 w = d + vec3(r0.w, r1.w, r2.w);
    vec4 c = mvp * vec4(w, 1.0);
    vec4 c2 = mvp * vec4(w + normalize(d) * 0.05, 1.0);
    gl_Position = expand_outline(c, c2, width);
}
"#;

/// One batched shape: the id of its single mesh's vertex buffer, the
/// batch mesh and the vertices per copy.
#[derive(Clone, Copy)]
pub struct Rig {
    pub single: u32,
    pub mesh: Mesh,
    pub per: i32,
    pub per_tris: i32,
    pub coarse: Option<(usize, f32)>,
}

/// The hero, skinned on the CPU (see `hero.rs`): textured, lit per pixel.
const SKIN_VS: &str = r#"
attribute vec3 vertexPosition;
attribute vec3 vertexNormal;
attribute vec2 vertexTexCoord;
uniform mat4 mvp;
uniform mat4 matModel;
varying vec3 vN;
varying vec3 vW;
varying vec2 vUV;
void main() {
    vW = (matModel * vec4(vertexPosition, 1.0)).xyz;
    vN = (matModel * vec4(vertexNormal, 0.0)).xyz;
    vUV = vertexTexCoord;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
"#;

const SKIN_FS: &str = r#"
varying vec3 vN;
varying vec3 vW;
varying vec2 vUV;
uniform sampler2D texture0;
uniform vec4 colDiffuse;
uniform vec3 sunDir;
uniform vec3 sunCol;
uniform vec3 shadeCol;
uniform vec3 skyCol;
uniform vec3 camPos;
uniform vec3 rimCol;
uniform vec3 fogCol;
uniform vec3 lampPos;
uniform vec3 lampCol;
uniform vec2 fogRange;
void main() {
    vec3 n = normalize(vN);
    vec3 up = normalize(vW);
    vec3 base = texture2D(texture0, vUV).rgb * colDiffuse.rgb;
    float lit = smoothstep(-0.02, 0.07, dot(n, sunDir));
    float day = smoothstep(-0.28, 0.18, dot(up, sunDir));
    vec3 col = base * (mix(shadeCol, sunCol * 0.95, lit * day) + skyCol * (0.08 + 0.12 * (dot(n, up) * 0.5 + 0.5)));
    col += lamp_light(base, n, lampPos - vW, lampCol);
    float rim = 1.0 - max(dot(n, normalize(camPos - vW)), 0.0);
    col += rimCol * smoothstep(0.6, 0.72, rim) * (0.25 + 0.75 * lit * day) * 0.3;
    float f = fog_factor(camPos - vW, fogRange);
    FRAG = vec4(mix(col, fogCol, f * 0.4), 1.0);
}
"#;

const OUTLINE_VS: &str = r#"
attribute vec3 vertexPosition;
attribute vec3 vertexNormal;
uniform mat4 mvp;
uniform float width;
uniform float radial;
void main() {
    vec3 n = radial > 0.5 ? vertexPosition : vertexNormal;
    vec4 c = mvp * vec4(vertexPosition, 1.0);
    vec4 c2 = mvp * vec4(vertexPosition + normalize(n) * 0.05, 1.0);
    gl_Position = expand_outline(c, c2, width);
}
"#;

const OUTLINE_FS: &str = r#"
uniform vec4 colDiffuse;
void main() {
    FRAG = vec4(colDiffuse.rgb, 1.0);
}
"#;

const BRIGHT_FS: &str = r#"
varying vec2 fragTexCoord;
uniform sampler2D texture0;
uniform float threshold;
void main() {
    vec3 c = texture2D(texture0, fragTexCoord).rgb;
    float l = max(c.r, max(c.g, c.b));
    FRAG = vec4(c * smoothstep(threshold, threshold + 0.25, l), 1.0);
}
"#;

const BLUR_FS: &str = r#"
varying vec2 fragTexCoord;
uniform sampler2D texture0;
uniform vec2 dir;
void main() {
    vec3 c = texture2D(texture0, fragTexCoord).rgb * 0.227;
    c += texture2D(texture0, fragTexCoord + dir * 1.385).rgb * 0.316;
    c += texture2D(texture0, fragTexCoord - dir * 1.385).rgb * 0.316;
    c += texture2D(texture0, fragTexCoord + dir * 3.231).rgb * 0.070;
    c += texture2D(texture0, fragTexCoord - dir * 3.231).rgb * 0.070;
    FRAG = vec4(c, 1.0);
}
"#;

const COMPOSITE_FS: &str = r#"
varying vec2 fragTexCoord;
uniform sampler2D texture0;
uniform sampler2D bloom;
uniform sampler2D paper;
uniform float bloomAmt;
uniform float aberr;
uniform float warp;
uniform float flash;
uniform float vignette;
uniform float paperAmt;
void main() {
    vec2 uv = fragTexCoord;
    vec2 d = uv - vec2(0.5);
    float r2 = dot(d, d);
    vec3 c;
    if (warp > 0.001) {
        c = vec3(0.0);
        for (int i = 0; i < 6; i++) {
            float k = 1.0 - warp * 0.04 * float(i);
            c += texture2D(texture0, vec2(0.5) + d * k).rgb;
        }
        c /= 6.0;
    } else {
        c = texture2D(texture0, uv).rgb;
    }
    if (aberr > 0.001) {
        vec2 o = d * aberr * 0.03;
        c.r = mix(c.r, texture2D(texture0, uv + o).r, 0.85);
        c.b = mix(c.b, texture2D(texture0, uv - o).b, 0.85);
    }
    if (bloomAmt > 0.0) {
        c += texture2D(bloom, uv).rgb * bloomAmt;
    }
    if (paperAmt > 0.0) {
        float p = texture2D(paper, uv * vec2(9.0, 6.75)).r;
        c *= 1.0 + (p - 0.5) * 0.08 * paperAmt;
        c = mix(c, c * vec3(1.03, 1.0, 0.95), paperAmt * 0.6);
    }
    c *= 1.0 - vignette * smoothstep(0.16, 0.6, r2);
    c = mix(c, vec3(1.0), flash);
    FRAG = vec4(c, 1.0);
}
"#;

/// What lights a frame: the sun, the shadow and sky colours, the lamp, and
/// the player's position for grass to part around.
pub struct Lighting {
    pub sun: V3,
    pub sun_col: Color,
    pub shade: Color,
    pub sky: Color,
    pub rim: Color,
    pub fog: Color,
    pub fog_range: (f32, f32),
    pub cam: V3,
    pub pusher: V3,
    pub wind: V3,
    pub lamp: V3,
    pub lamp_col: Color,
}

impl Lighting {
    /// The toon colour of `base` on a surface facing `n` at `up`, for things
    /// drawn through rlgl (the scarf, strings) rather than the lit shader.
    pub fn shade_cpu(&self, base: Color, n: V3, up: V3) -> Color {
        let ndl = n.dot(self.sun).abs();
        let lit = crate::math::smooth((ndl + 0.02) / 0.1);
        let day = crate::math::smooth((up.dot(self.sun) + 0.28) / 0.46);
        let a = modulate(base, mix(self.shade, self.sun_col, lit * day));
        let b = modulate(base, scale(self.sky, 0.16));
        Color { r: a.r.saturating_add(b.r), g: a.g.saturating_add(b.g), b: a.b.saturating_add(b.b), a: 255 }
    }
}

pub struct Post {
    /// 2: bloom, paper and lens effects; 1: paper and lens effects; 0: a plain copy.
    pub level: u8,
    pub bloom: f32,
    pub threshold: f32,
    pub aberr: f32,
    pub warp: f32,
    pub flash: f32,
}

/// Whether and how a part gets its ink outline.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Ink {
    None,
    /// Pushed out along the mesh's smooth normals.
    Smooth,
    /// Pushed out from the shape's origin, for the unit shapes (gap-free on boxes).
    Radial,
}

struct Part {
    surface_lod: Option<&'static str>,
    rig: Option<usize>,
    mesh: Mesh,
    m: Matrix,
    c: Color,
    e: f32,
    ink: Ink,
}

pub struct Gl {
    pub lit: Uniforms,
    skin: Uniforms,
    skin_mat: Material,
    rig_u: Uniforms,
    rig_mat: Material,
    rig_outline: Uniforms,
    rig_outline_mat: Material,
    rigs: RefCell<Vec<Rig>>,
    pub litv: Uniforms,
    litv_mat: Material,
    outline: Uniforms,
    bright: Uniforms,
    blur: Uniforms,
    composite: Uniforms,
    lit_mat: Material,
    outline_mat: Material,
    pub sprite: Texture2D,
    pub ring: Texture2D,
    paper: Texture2D,
    pub scene: RenderTexture2D,
    small: [RenderTexture2D; 2],
    parts: RefCell<Vec<Part>>,
    pub ink: Cell<Color>,
    pub ink_px: Cell<f32>,
    pub outlines: Cell<bool>,
    /// The hero's outline costs several milliseconds on the handheld: high effects only.
    pub hero_ink: Cell<bool>,
}

fn material(shader: Shader) -> Material {
    let mut m = unsafe { ffi::LoadMaterialDefault() };
    m.shader = shader;
    m
}

fn texture(size: i32, px: &mut [u8], repeat: bool) -> Texture2D {
    let img = ffi::Image {
        data: px.as_mut_ptr() as _,
        width: size,
        height: size,
        mipmaps: 1,
        format: ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 as i32,
    };
    let t = unsafe { ffi::LoadTextureFromImage(img) };
    unsafe {
        ffi::SetTextureFilter(t, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
        if repeat {
            ffi::SetTextureWrap(t, ffi::TextureWrap::TEXTURE_WRAP_REPEAT as i32);
        }
    }
    t
}

/// A white texture whose alpha is `f(distance from centre, 0..1)`.
fn sprite_texture(size: i32, f: impl Fn(f32) -> f32) -> Texture2D {
    let mut px = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let a = f((dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            let i = ((y * size + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    texture(size, &mut px, false)
}

/// Tileable grey paper: two octaves of value noise and fine fibres.
fn paper_texture() -> Texture2D {
    const S: usize = 128;
    let mut rng = Rng::new(42);
    let lattice = |cells: usize, rng: &mut Rng| -> Vec<f32> { (0..cells * cells).map(|_| rng.f()).collect() };
    let l1 = lattice(8, &mut rng);
    let l2 = lattice(32, &mut rng);
    let sample = |l: &[f32], cells: usize, x: f32, y: f32| {
        let fx = x * cells as f32;
        let fy = y * cells as f32;
        let (ix, iy) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (crate::math::smooth(fx.fract()), crate::math::smooth(fy.fract()));
        let at = |i: usize, j: usize| l[(j % cells) * cells + (i % cells)];
        let a = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * tx;
        let b = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * tx;
        a + (b - a) * ty
    };
    let mut px = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let (u, v) = (x as f32 / S as f32, y as f32 / S as f32);
            let n = 0.5 * sample(&l1, 8, u, v) + 0.35 * sample(&l2, 32, u, v) + 0.15 * rng.f();
            let g = (n.clamp(0.0, 1.0) * 255.0) as u8;
            let i = (y * S + x) * 4;
            px[i..i + 4].copy_from_slice(&[g, g, g, 255]);
        }
    }
    texture(S as i32, &mut px, true)
}

fn target(w: i32, h: i32) -> RenderTexture2D {
    let t = unsafe { ffi::LoadRenderTexture(w, h) };
    unsafe {
        ffi::SetTextureFilter(t.texture, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
        ffi::SetTextureWrap(t.texture, ffi::TextureWrap::TEXTURE_WRAP_CLAMP as i32);
    }
    t
}

impl Gl {
    /// `scale` sizes the 3D scene target against the 640×480 canvas.
    pub fn new(scale: f32) -> Gl {
        let (sw, sh) = ((W as f32 * scale).round() as i32, (H as f32 * scale).round() as i32);
        let lit = load(Some(LIT_VS), &format!("{LIGHTING_GLSL}{LIT_FS}"));
        let outline = load(Some(&format!("{OUTLINE_GLSL}{OUTLINE_VS}")), OUTLINE_FS);
        let litv = load(Some(&format!("{LIGHTING_GLSL}{LITV_VS}")), LITV_FS);
        let skin = load(Some(SKIN_VS), &format!("{LIGHTING_GLSL}{SKIN_FS}"));
        let rig = load(Some(&format!("{RIG_HEAD}{RIG_VS}")), &format!("{LIGHTING_GLSL}{RIG_FS}"));
        let rig_outline = load(Some(&format!("{OUTLINE_GLSL}{RIG_HEAD}{RIG_OUTLINE_VS}")), OUTLINE_FS);
        Gl {
            lit: Uniforms::new(lit),
            skin: Uniforms::new(skin),
            skin_mat: material(skin),
            rig_u: Uniforms::new(rig),
            rig_mat: material(rig),
            rig_outline: Uniforms::new(rig_outline),
            rig_outline_mat: material(rig_outline),
            rigs: RefCell::new(Vec::new()),
            litv: Uniforms::new(litv),
            litv_mat: material(litv),
            outline: Uniforms::new(outline),
            bright: Uniforms::new(load(None, BRIGHT_FS)),
            blur: Uniforms::new(load(None, BLUR_FS)),
            composite: Uniforms::new(load(None, COMPOSITE_FS)),
            lit_mat: material(lit),
            outline_mat: material(outline),
            sprite: sprite_texture(64, |d| (1.0 - d).max(0.0).powf(2.2)),
            ring: sprite_texture(64, |d| (1.0 - ((d - 0.8) / 0.14).powi(2)).max(0.0)),
            paper: paper_texture(),
            scene: target(sw, sh),
            small: [target(sw / 4, sh / 4), target(sw / 4, sh / 4)],
            parts: RefCell::new(Vec::with_capacity(256)),
            ink: Cell::new(rgb(0x3A2B33)),
            ink_px: Cell::new(1.4),
            outlines: Cell::new(true),
            hero_ink: Cell::new(true),
        }
    }

    /// Sets the lighting every lit draw this frame uses.
    pub fn light(&self, l: &Lighting, time: f32) {
        for u in [&self.lit, &self.litv, &self.rig_u, &self.skin] {
            Self::light_one(u, l);
        }
        for u in [&self.lit, &self.rig_u, &self.skin] {
            u.v3("rimCol", vec3(l.rim));
        }
        self.litv.f("time", time);
        self.litv.v3("pusher", l.pusher.arr());
        self.litv.v3("wind", l.wind.arr());
        self.lit.f("emissive", 0.0);
        self.lit.f("fresnel", 0.0);
    }

    fn light_one(u: &Uniforms, l: &Lighting) {
        u.v3("sunDir", l.sun.arr());
        u.v3("sunCol", vec3(l.sun_col));
        u.v3("shadeCol", vec3(l.shade));
        u.v3("skyCol", vec3(l.sky));
        u.v3("camPos", l.cam.arr());
        u.v3("fogCol", vec3(l.fog));
        u.v2("fogRange", [l.fog_range.0, l.fog_range.1]);
        u.v3("lampPos", l.lamp.arr());
        u.v3("lampCol", vec3(l.lamp_col));
    }

    fn submit(&self, mesh: Mesh, mat: Material, m: Matrix, lod: Option<&'static str>) {
        crate::prof::mesh(mesh.triangleCount as u64, lod);
        unsafe { ffi::DrawMesh(mesh, mat, m) };
    }

    /// Draws a mesh now with the lit shader.
    pub fn draw(&self, mesh: Mesh, m: Matrix, c: Color, emissive: f32) {
        if mesh.vertexCount == 0 {
            return;
        }
        self.lit.f("emissive", emissive);
        unsafe { (*self.lit_mat.maps).color = c };
        self.submit(mesh, self.lit_mat, m, None);
    }

    /// Queues a mesh; `flush` draws every queued fill, then every outline.
    pub fn part(&self, mesh: Mesh, m: Matrix, c: Color, e: f32, ink: Ink) {
        if mesh.vertexCount > 0 {
            self.parts.borrow_mut().push(Part { surface_lod: None, rig: self.rig_of(mesh, &m), mesh, m, c, e, ink });
        }
    }

    /// Queues a world-space surface in draw order, lit per vertex.
    pub fn surface(&self, mesh: Mesh, lod: &'static str) {
        if mesh.vertexCount > 0 {
            self.parts.borrow_mut().push(Part {
                surface_lod: Some(lod),
                rig: None,
                mesh,
                m: crate::math::identity(),
                c: rgb(0xFFFFFF),
                e: 0.0,
                ink: Ink::None,
            });
        }
    }

    pub fn hero_outline_enabled(&self) -> bool {
        self.outlines.get() && self.hero_ink.get() && !crate::prof::skip("heroink")
    }

    /// Draws the CPU-skinned hero mesh and the optional ink outline.
    pub fn draw_skinned(&self, mesh: Mesh, ink: Option<Mesh>, m: Matrix, tex: Texture2D) {
        unsafe {
            (*self.skin_mat.maps).texture = tex;
            (*self.skin_mat.maps).color = rgb(0xFFFFFF);
            self.submit(mesh, self.skin_mat, m, None);
            if let Some(ink) = ink {
                (*self.outline_mat.maps).color = self.ink.get();
                self.outline.f("width", self.ink_px.get());
                self.outline.f("radial", 0.0);
                ffi::rlSetCullFace(ffi::rlCullMode::RL_CULL_FACE_FRONT as i32);
                self.submit(ink, self.outline_mat, m, None);
                ffi::rlSetCullFace(ffi::rlCullMode::RL_CULL_FACE_BACK as i32);
            }
        }
    }

    /// Whether the 3D scene can be drawn straight to a 640×480 screen,
    /// skipping the scene texture and the full-screen copy out of it.
    pub fn direct(&self, level: u8) -> bool {
        level < 2
            && self.scene.texture.width == W
            && unsafe { ffi::GetRenderWidth() == W && ffi::GetRenderHeight() == H }
    }

    /// Registers the batched unit shapes (see `Kit`).
    pub fn set_rig(&self, rigs: Vec<Rig>) {
        *self.rigs.borrow_mut() = rigs;
    }

    fn rig_of(&self, mesh: Mesh, m: &Matrix) -> Option<usize> {
        if mesh.vboId.is_null() {
            return None;
        }
        let id = unsafe { *mesh.vboId };
        let rigs = self.rigs.borrow();
        let index = rigs.iter().position(|r| r.single == id)?;
        if let Some((coarse, cutoff)) = rigs[index].coarse {
            let len = |x: f32, y: f32, z: f32| (x * x + y * y + z * z).sqrt();
            let size = len(m.m0, m.m1, m.m2).max(len(m.m4, m.m5, m.m6)).max(len(m.m8, m.m9, m.m10));
            if size < cutoff {
                return Some(coarse);
            }
        }
        Some(index)
    }

    /// Draws parts of one rig in batches of up to RIG_N.
    fn draw_rig<'a>(&self, r: &Rig, mut parts: impl Iterator<Item = &'a Part>, u: &Uniforms, mat: Material) {
        let mut bones = [[[0.0; 4]; 4]; RIG_N];
        loop {
            let mut count = 0;
            for (rows, p) in bones.iter_mut().zip(parts.by_ref()) {
                let m = p.m;
                let c = vec3(p.c);
                *rows = [
                    [m.m0, m.m4, m.m8, m.m12],
                    [m.m1, m.m5, m.m9, m.m13],
                    [m.m2, m.m6, m.m10, m.m14],
                    [c[0], c[1], c[2], p.e],
                ];
                count += 1;
            }
            if count == 0 {
                break;
            }
            let mut mesh = r.mesh;
            mesh.vertexCount = r.per * count;
            mesh.triangleCount = r.per_tris * count;
            unsafe {
                ffi::SetShaderValueV(
                    u.shader,
                    u.loc("bones"),
                    bones.as_ptr() as _,
                    ffi::ShaderUniformDataType::SHADER_UNIFORM_VEC4 as i32,
                    count * 4,
                );
            }
            self.submit(mesh, mat, crate::math::identity(), None);
        }
    }

    pub fn flush(&self) {
        let mut parts = self.parts.borrow_mut();
        let rigs = self.rigs.borrow();
        for p in parts.iter().rev().filter(|p| p.rig.is_none() && p.surface_lod.is_none()) {
            self.draw(p.mesh, p.m, p.c, p.e);
        }
        for p in parts.iter().filter(|p| p.surface_lod.is_some()) {
            self.submit(p.mesh, self.litv_mat, p.m, p.surface_lod);
        }
        self.lit.f("emissive", 0.0);
        for (k, r) in rigs.iter().enumerate() {
            self.draw_rig(r, parts.iter().filter(|p| p.rig == Some(k)), &self.rig_u, self.rig_mat);
        }
        if self.outlines.get() {
            unsafe {
                (*self.outline_mat.maps).color = self.ink.get();
                (*self.rig_outline_mat.maps).color = self.ink.get();
                ffi::rlSetCullFace(ffi::rlCullMode::RL_CULL_FACE_FRONT as i32);
            }
            self.outline.f("width", self.ink_px.get());
            self.rig_outline.f("width", self.ink_px.get());
            let mut radial = None;
            for p in parts.iter().filter(|p| p.ink != Ink::None && p.rig.is_none()) {
                let r = p.ink == Ink::Radial;
                if radial != Some(r) {
                    self.outline.f("radial", if r { 1.0 } else { 0.0 });
                    radial = Some(r);
                }
                self.submit(p.mesh, self.outline_mat, p.m, None);
            }
            for (k, r) in rigs.iter().enumerate() {
                self.draw_rig(
                    r,
                    parts.iter().filter(|p| p.rig == Some(k) && p.ink != Ink::None),
                    &self.rig_outline,
                    self.rig_outline_mat,
                );
            }
            unsafe { ffi::rlSetCullFace(ffi::rlCullMode::RL_CULL_FACE_BACK as i32) };
        }
        parts.clear();
    }

    /// A see-through glass shape that brightens towards its edges.
    pub fn draw_glass(&self, mesh: Mesh, m: Matrix, c: Color) {
        self.lit.f("fresnel", 0.9);
        unsafe {
            ffi::BeginBlendMode(ffi::BlendMode::BLEND_ALPHA as i32);
            ffi::rlDisableDepthMask();
        }
        self.draw(mesh, m, c, 0.2);
        unsafe {
            ffi::rlEnableDepthMask();
            ffi::EndBlendMode();
        }
        self.lit.f("fresnel", 0.0);
    }

    /// Draws the 640×480 scene to the screen at `dest` with the effects in `post`.
    pub fn present(&self, post: &Post, dest: ffi::Rectangle) {
        let flip = |t: &RenderTexture2D| ffi::Rectangle {
            x: 0.0,
            y: 0.0,
            width: t.texture.width as f32,
            height: -(t.texture.height as f32),
        };
        let origin = ffi::Vector2 { x: 0.0, y: 0.0 };
        let white = rgb(0xFFFFFF);
        let bloom = post.level >= 2 && !crate::prof::skip("bloom");
        let lens = post.level >= 1;
        unsafe {
            if bloom {
                // Bright parts at a quarter of the size, blurred across then down, twice.
                let sw = self.small[0].texture.width as f32;
                let sh = self.small[0].texture.height as f32;
                let full = ffi::Rectangle { x: 0.0, y: 0.0, width: sw, height: sh };
                self.bright.f("threshold", post.threshold);
                ffi::BeginTextureMode(self.small[0]);
                ffi::BeginShaderMode(self.bright.shader);
                ffi::DrawTexturePro(self.scene.texture, flip(&self.scene), full, origin, 0.0, white);
                ffi::EndShaderMode();
                ffi::EndTextureMode();
                for (i, d) in
                    [[1.0 / sw, 0.0], [0.0, 1.0 / sh], [2.0 / sw, 0.0], [0.0, 2.0 / sh]].into_iter().enumerate()
                {
                    let (src, dst) = (&self.small[i % 2], self.small[(i + 1) % 2]);
                    ffi::BeginTextureMode(dst);
                    ffi::BeginShaderMode(self.blur.shader);
                    self.blur.v2("dir", d);
                    ffi::DrawTexturePro(src.texture, flip(src), full, origin, 0.0, white);
                    ffi::EndShaderMode();
                    ffi::EndTextureMode();
                }
            }
            let c = &self.composite;
            ffi::BeginShaderMode(c.shader);
            c.f("bloomAmt", if bloom { post.bloom } else { 0.0 });
            c.f("aberr", if lens { post.aberr } else { 0.0 });
            c.f("warp", if lens { post.warp } else { 0.0 });
            c.f("flash", post.flash);
            c.f("vignette", if lens { 0.42 } else { 0.0 });
            c.f("paperAmt", if lens { 1.0 } else { 0.0 });
            c.tex("bloom", self.small[0].texture);
            c.tex("paper", self.paper);
            ffi::DrawTexturePro(self.scene.texture, flip(&self.scene), dest, origin, 0.0, white);
            ffi::EndShaderMode();
        }
    }
}

/// Camera-facing and free-oriented textured quads through rlgl's batch:
/// stars, sparks, glows, ripples.
#[derive(Clone, Copy)]
pub struct Sprites {
    pub right: V3,
    pub up: V3,
    pub fwd: V3,
}

impl Sprites {
    pub fn begin(tex: Texture2D) {
        unsafe {
            ffi::rlSetTexture(tex.id);
            ffi::rlBegin(ffi::RL_QUADS as i32);
        }
    }
    pub fn end() {
        unsafe {
            ffi::rlEnd();
            ffi::rlSetTexture(0);
        }
    }
    /// A quad centred on `c` spanning ±`a` and ±`b`.
    pub fn quad(c: V3, a: V3, b: V3, col: Color) {
        unsafe {
            ffi::rlColor4ub(col.r, col.g, col.b, col.a);
            for (p, (u, v)) in
                [(c - a - b, (0.0, 0.0)), (c - a + b, (0.0, 1.0)), (c + a + b, (1.0, 1.0)), (c + a - b, (1.0, 0.0))]
            {
                ffi::rlTexCoord2f(u, v);
                ffi::rlVertex3f(p.x, p.y, p.z);
            }
        }
    }
    pub fn billboard(&self, c: V3, size: f32, col: Color) {
        Sprites::quad(c, self.right * size, self.up * size, col);
    }
    /// A billboard stretched along the on-screen direction of `dir`.
    pub fn streak(&self, c: V3, dir: V3, len: f32, width: f32, col: Color) {
        let d = dir.flatten(self.fwd).norm();
        let side = d.cross(self.fwd).norm();
        Sprites::quad(c, d * len, side * width, col);
    }
}

fn blended(mode: ffi::BlendMode, f: impl FnOnce()) {
    unsafe {
        ffi::BeginBlendMode(mode as i32);
        ffi::rlDisableDepthMask();
        ffi::rlDisableBackfaceCulling();
    }
    f();
    unsafe {
        ffi::rlDrawRenderBatchActive();
        ffi::rlEnableBackfaceCulling();
        ffi::rlEnableDepthMask();
        ffi::EndBlendMode();
    }
}

/// Glowing things that add light: no depth writes, both sides.
pub fn additive(f: impl FnOnce()) {
    blended(ffi::BlendMode::BLEND_ADDITIVE, f);
}

/// Soft see-through things such as dust and shadows.
pub fn alpha(f: impl FnOnce()) {
    blended(ffi::BlendMode::BLEND_ALPHA, f);
}
