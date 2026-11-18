#!/usr/bin/env python3
"""Desktop smoke test. Run with: xvfb-run -a -s '-screen 0 1440x960x24' python3 scripts/ui-smoke.py"""
from pathlib import Path
import os
import subprocess
import time
from PIL import ImageGrab, Image, ImageChops

root = Path(__file__).resolve().parent.parent
out = root / 'artifacts' / 'ui'
out.mkdir(parents=True, exist_ok=True)
process = subprocess.Popen([str(root / os.environ.get('MORI_BINARY', 'target/debug/mori')), '--demo'], cwd=root)

def key(value):
    subprocess.run(['xdotool', 'key', '--clearmodifiers', value], check=True)
    time.sleep(0.35)

def shot(name):
    ImageGrab.grab(xdisplay=os.environ['DISPLAY']).save(out / (name + '.png'))

try:
    time.sleep(1.2)
    window = subprocess.check_output(['xdotool', 'search', '--sync', '--pid', str(process.pid)], text=True).splitlines()[0]
    subprocess.run(['xdotool', 'windowfocus', window], check=True)
    shot('canvas')
    key('alt+2')
    shot('rhythm')
    key('ctrl+comma')
    shot('settings')
    key('alt+1')
    key('d')
    subprocess.run(['xdotool', 'mousemove', '500', '400', 'click', '1'], check=True)
    time.sleep(0.35)
    shot('tap')
    key('z')
    shot('undo')
    key('y')
    shot('redo')
    tap = Image.open(out / 'tap.png').convert('RGB').crop((60,215,1380,755))
    undo = Image.open(out / 'undo.png').convert('RGB').crop((60,215,1380,755))
    redo = Image.open(out / 'redo.png').convert('RGB').crop((60,215,1380,755))
    assert ImageChops.difference(tap, undo).getbbox(), 'Undo did not change the drawing'
    assert ImageChops.difference(tap, redo).getbbox() is None, 'Redo did not restore the same drawing'
    key('l')
    shot('liquify')
    assert process.poll() is None, 'App exited during desktop input'
    print('Desktop pages, tap, undo, redo, liquify: captured in artifacts/ui')
finally:
    process.terminate()
    process.wait(timeout=10)
