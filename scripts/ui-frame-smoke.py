#!/usr/bin/env python3
"""Verify blank/copy shortcuts and frame-head copy/delete in the real UI."""
from pathlib import Path
import os
import subprocess
import tempfile
import time
from PIL import ImageGrab

root=Path(__file__).resolve().parent.parent
out=root/'artifacts/ui'
out.mkdir(parents=True,exist_ok=True)
binary=str(root/os.environ.get('MORI_BINARY','target/debug/mori'))
def xd(*args):subprocess.run(['xdotool',*map(str,args)],check=True)
def key(value):xd('key','--clearmodifiers',value);time.sleep(.2)
def click(x,y,button=1):xd('mousemove',x,y,'click',button);time.sleep(.2)
def shot(name):
    xd('mousemove',30,30);time.sleep(.1)
    im=ImageGrab.grab(xdisplay=os.environ['DISPLAY']).convert('RGB');im.save(out/(name+'.png'));return im

def launch(env,demo=False):
    app=subprocess.Popen([binary]+(['--demo']if demo else []),cwd=root,env=env)
    time.sleep(1)
    window=subprocess.check_output(['xdotool','search','--sync','--pid',str(app.pid)],text=True).splitlines()[0]
    xd('windowfocus',window);return app

def head_present(im):
    return sum(min(im.getpixel((x,y)))>150 for x in range(250,256) for y in range(730,736))>3

def ink(im):return sum(max(p)<100 for p in im.crop((475,380,525,420)).getdata())
with tempfile.TemporaryDirectory(prefix='mori-frame-') as temp:
    env=dict(os.environ,MORI_SYSTEM_LOCALE='en-US',MORI_PREFERENCES_PATH=str(Path(temp)/'prefs.json'))
    app=launch(env)
    try:
        key('o');click(500,400);original=shot('frame-original');assert ink(original)>0
        key('d');key('Delete');blank=shot('frame-blank-del');assert ink(blank)==0
        key('ctrl+z');assert ink(shot('frame-blank-undo'))==ink(original)
        key('ctrl+shift+z');assert ink(shot('frame-blank-redo'))==0
        key('shift+d');assert ink(shot('frame-copy-previous'))==ink(original)
        xd('mousemove',720,480,'mousedown',3);time.sleep(.1);xd('mousemove',720,535);time.sleep(.1);xd('mouseup',3)
        click(500,400);assert ink(shot('frame-erased-empty'))==0
        key('a');assert ink(shot('frame-source-unchanged'))==ink(original)
        key('d');assert ink(shot('frame-empty-kept'))==0
        key('d');key('shift+d');assert ink(shot('frame-copy-empty'))==0
        key('a');key('a');assert ink(shot('frame-original-still-kept'))==ink(original)
    finally:app.terminate();app.wait(timeout=10)
    app=launch(env,demo=True)
    try:
        key('alt+2')
        before=shot('frame-menu-before')
        # First frame occupies time zero, followed by frames at .25, .5, ...
        click(228,733,3);menu=shot('frame-head-menu')
        click(276,750)  # Copy, the first menu row.
        copied=shot('frame-head-copied')
        assert head_present(copied),'Copy did not insert a frame at the next grid point'
        key('ctrl+z');undone=shot('frame-head-copy-undone')
        assert not head_present(undone),'Copy did not undo as one step'
        key('ctrl+shift+z')
        # With a three-second demo, one 1/8-beat step is 24.7 screen pixels.
        click(253,733,3);shot('frame-head-copy-menu');click(280,789)
        deleted=shot('frame-head-deleted')
        assert not head_present(deleted),'Deleting the copied frame did not remove its head'
        key('ctrl+z')
        assert head_present(shot('frame-head-delete-undone')),'Delete could not be undone'
        assert app.poll() is None
    finally:app.terminate();app.wait(timeout=10)
print('Del blank frames, Shift+D independent copies, retained erased frames and frame-head copy/delete: passed')
