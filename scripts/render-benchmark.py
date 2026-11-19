#!/usr/bin/env python3
"""Measure the real renderer on a headless Linux EGL device, without UI/vsync.

Uses the repository's Rust sources and driver GPU timer queries. Query readback
is restricted to this offline measurement; interactive rendering never waits on it.
Requires libEGL, a working OpenGL driver and Cargo. No Python packages required.
"""
import argparse
import ctypes as c
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--size', default='2880x1206')
parser.add_argument('--output', type=Path, default=root/'artifacts/renderer-benchmark.json')
parser.add_argument('--verify', action='store_true', help='Also verify pixel equivalence against four-channel masks')
parser.add_argument('--vendor', default='NVIDIA', help='EGL vendor name substring')
args = parser.parse_args()
width, height = map(int, args.size.lower().split('x'))
if not (width > 0 and height > 0):
    parser.error('size must be positive')

egl = c.CDLL('libEGL.so.1')
def function(name, result, *parameters):
    f = getattr(egl, name)
    f.restype, f.argtypes = result, list(parameters)
    return f
get_proc = function('eglGetProcAddress', c.c_void_p, c.c_char_p)
query_devices = c.CFUNCTYPE(c.c_uint,c.c_int,c.POINTER(c.c_void_p),c.POINTER(c.c_int))(get_proc(b'eglQueryDevicesEXT'))
get_display = c.CFUNCTYPE(c.c_void_p,c.c_uint,c.c_void_p,c.POINTER(c.c_int))(get_proc(b'eglGetPlatformDisplayEXT'))
initialize = function('eglInitialize', c.c_uint, c.c_void_p, c.POINTER(c.c_int), c.POINTER(c.c_int))
query_string = function('eglQueryString', c.c_char_p,c.c_void_p,c.c_int)
terminate = function('eglTerminate',c.c_uint,c.c_void_p)
devices=(c.c_void_p*32)(); count=c.c_int()
if not query_devices(32,devices,c.byref(count)):
    raise RuntimeError('EGL device enumeration failed')
display = None
for device in devices[:count.value]:
    candidate = get_display(0x313f,device,None)  # EGL_PLATFORM_DEVICE_EXT
    major, minor = c.c_int(), c.c_int()
    if initialize(candidate,c.byref(major),c.byref(minor)):
        if args.vendor.lower() in query_string(candidate,0x3053).decode().lower():
            display = candidate
            break
        terminate(candidate)
if display is None:
    raise RuntimeError('No working EGL device for vendor '+args.vendor)
choose = function('eglChooseConfig',c.c_uint,c.c_void_p,c.POINTER(c.c_int),c.POINTER(c.c_void_p),c.c_int,c.POINTER(c.c_int))
attributes=(c.c_int*13)(0x3033,1,0x3040,8,0x3024,8,0x3023,8,0x3022,8,0x3021,8,0x3038)
config=c.c_void_p(); found=c.c_int()
if not choose(display,attributes,c.byref(config),1,c.byref(found)) or not found.value:
    raise RuntimeError('No OpenGL pbuffer config')
if not function('eglBindAPI',c.c_uint,c.c_uint)(0x30a2):
    raise RuntimeError('Cannot select OpenGL')
context_attributes=(c.c_int*7)(0x3098,4,0x30fb,1,0x30fd,1,0x3038)
context=function('eglCreateContext',c.c_void_p,c.c_void_p,c.c_void_p,c.c_void_p,c.POINTER(c.c_int))(display,config,None,context_attributes)
surface_attributes=(c.c_int*5)(0x3057,1,0x3056,1,0x3038)
surface=function('eglCreatePbufferSurface',c.c_void_p,c.c_void_p,c.c_void_p,c.POINTER(c.c_int))(display,config,surface_attributes)
if not context or not surface or not function('eglMakeCurrent',c.c_uint,c.c_void_p,c.c_void_p,c.c_void_p,c.c_void_p)(display,surface,surface,context):
    raise RuntimeError('Cannot make OpenGL 4.1 context current')

with tempfile.TemporaryDirectory(prefix='mori-render-benchmark-') as temporary:
    temp=Path(temporary)
    manifest='''[package]
name="mori-render-benchmark"
version="0.0.0"
edition="2021"
[lib]
crate-type=["cdylib"]
path="src/bench.rs"
[dependencies]
eframe={path=%s,default-features=false,features=["glow","x11"]}
serde={version="1",features=["derive","rc"]}
serde_json="1"
[profile.release]
lto="thin"
codegen-units=1
''' % json.dumps(str(root/'vendor/eframe'))
    (temp/'Cargo.toml').write_text(manifest)
    shutil.copy2(root/'Cargo.lock',temp/'Cargo.lock')
    (temp/'src').mkdir()
    for name in ['model','curve','render','timing']:
        shutil.copy2(root/'src'/(name+'.rs'), temp/'src'/(name+'.rs'))
    shutil.copytree(root/'src/render',temp/'src/render')
    shutil.copytree(root/'shaders',temp/'shaders')
    modules='\n'.join('mod '+name+';' for name in ['model','curve','render','timing'])
    (temp/'src/bench.rs').write_text('''#![allow(dead_code)]
'''+modules+'''
#[link(name="EGL")]
unsafe extern "C" { fn eglGetProcAddress(name: *const std::ffi::c_char) -> *const std::ffi::c_void; }
#[no_mangle]
pub unsafe extern "C" fn measure(width: i32,height:i32,verify: bool) -> *mut std::ffi::c_char {
    let gl=eframe::glow::Context::from_loader_function(|name| eglGetProcAddress(std::ffi::CString::new(name).unwrap().as_ptr()));
    let mut report=render::benchmark::run(&gl,[width,height]);
    if verify { report["verification"]=serde_json::json!(render::Renderer::verify_mask_precision(&gl)); }
    std::ffi::CString::new(report.to_string()).unwrap().into_raw()
}
''')
    env=dict(os.environ,CARGO_TARGET_DIR=str(root/'target'))
    rustup_cargo=Path.home()/'.cargo/bin/cargo'
    cargo=os.environ.get('CARGO') or (str(rustup_cargo) if rustup_cargo.exists() else shutil.which('cargo'))
    subprocess.run([cargo,'build','--release','--quiet','--manifest-path',str(temp/'Cargo.toml')],env=env,check=True)
    library=c.CDLL(str(root/'target/release/libmori_render_benchmark.so'))
    library.measure.argtypes=[c.c_int,c.c_int,c.c_bool];library.measure.restype=c.c_char_p
    results=json.loads(library.measure(width,height,args.verify))
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(results,indent=2)+'\n')
    print(json.dumps(results,indent=2))
function('eglMakeCurrent',c.c_uint,c.c_void_p,c.c_void_p,c.c_void_p,c.c_void_p)(display,None,None,None)
function('eglDestroySurface',c.c_uint,c.c_void_p,c.c_void_p)(display,surface)
function('eglDestroyContext',c.c_uint,c.c_void_p,c.c_void_p)(display,context)
terminate(display)
