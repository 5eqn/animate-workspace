#!/usr/bin/env python3
"""Run under Xvfb; exercise the real desktop and keep preferences isolated."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import time
from PIL import ImageGrab, Image, ImageChops

root = Path(__file__).resolve().parent.parent
out = root / 'artifacts' / 'ui'
out.mkdir(parents=True, exist_ok=True)
binary = str(root / os.environ.get('MORI_BINARY', 'target/debug/mori'))

def key(value):
    subprocess.run(['xdotool', 'key', '--clearmodifiers', value], check=True)
    time.sleep(.25)

def click(x,y,button=1):
    subprocess.run(['xdotool','mousemove',str(x),str(y),'click',str(button)],check=True)
    time.sleep(.25)

def edit(x,y,value):
    subprocess.run(['xdotool','mousemove',str(x),str(y),'click','--repeat','2','--delay','80','1'],check=True)
    key('ctrl+a')
    subprocess.run(['xdotool','type','--clearmodifiers','--',str(value)],check=True)
    key('Return')

def shot(name):
    image = ImageGrab.grab(xdisplay=os.environ['DISPLAY'])
    image.save(out / (name + '.png'))
    return image

def dark_surface(image, bounds):
    pixels=list(image.convert('RGB').crop(bounds).getdata())
    bright=sum(min(p)>210 for p in pixels)
    assert bright/len(pixels)<.12,'A menu or input reverted to a bright background'

def playhead(image):
    line=image.convert('RGB')
    return max(range(223,1400),key=lambda x:line.getpixel((x,920))[2])

def start(env,demo=True):
    process = subprocess.Popen([binary]+(['--demo'] if demo else []),cwd=root,env=env)
    time.sleep(1)
    window = subprocess.check_output(['xdotool','search','--sync','--pid',str(process.pid)],text=True).splitlines()[0]
    subprocess.run(['xdotool','windowfocus',window],check=True)
    return process

def cursor_size():
    subprocess.run(['xdotool','mousemove','720','480'],check=True)
    time.sleep(.15)
    image=ImageGrab.grab(xdisplay=os.environ['DISPLAY']).convert('RGB').crop((420,180,1020,780))
    # Only the brush cursor can mark an otherwise empty, white P1 canvas.
    mask=image.convert('L').point(lambda v:255 if v<230 else 0)
    bounds=mask.getbbox()
    return 0 if bounds is None else bounds[2]-bounds[0]

with tempfile.TemporaryDirectory(prefix='mori-ui-') as temporary:
    prefs=Path(temporary)/'preferences.json'
    env=dict(os.environ,MORI_PREFERENCES_PATH=str(prefs))
    process=start(env)
    try:
        shot('canvas')
        key('alt+2');shot('rhythm')
        key('ctrl+comma');shot('settings')
        for i,name in enumerate(['project','brushes','onion','playback','export','shortcuts']):
            click(90,124+i*48);shot('settings-'+name)
        click(72,51)
        returned=shot('back-to-timeline')
        assert min(returned.getpixel((300,50))[:3])>240,'Settings Back did not return to P2'
        key('ctrl+comma');click(90,124)
        click(975,303);popup=shot('settings-grid-popup');dark_surface(popup,(930,345,1010,500));key('Escape')
        click(90,172)
        edit(850,675,84)
        edit(967,675,360)
        assert json.loads(prefs.read_text())['brush_defaults'][1]=={'ink':84.0,'liquify':360.0}
        click(90,268)
        # Edit a global preference through the actual UI, without saving a project.
        subprocess.run(['xdotool','mousemove','978','152','click','--repeat','2','--delay','80','1'],check=True)
        key('ctrl+a')
        subprocess.run(['xdotool','type','--clearmodifiers','--','-125'],check=True)
        key('Return')
        assert json.loads(prefs.read_text())['video_delay_ms']==-125
        key('alt+1');key('d');click(500,400);shot('tap')
        key('z');shot('undo');key('y');shot('redo')
        crop=(60,215,1380,755)
        tap=Image.open(out/'tap.png').convert('RGB').crop(crop)
        undo=Image.open(out/'undo.png').convert('RGB').crop(crop)
        redo=Image.open(out/'redo.png').convert('RGB').crop(crop)
        assert ImageChops.difference(tap,undo).getbbox(),'Undo did not change drawing'
        assert ImageChops.difference(tap,redo).getbbox() is None,'Redo did not restore drawing'
        key('l');shot('liquify')
        key('alt+2');click(90,615,3)
        menu=shot('layer-menu');dark_surface(menu,(96,565,326,950))
        click(277,630);popup=shot('layer-blend-popup');dark_surface(popup,(220,680,310,800))
        key('Escape');click(1250,50);click(90,615,3)
        # The context menu is the only layer-creation control.
        click(140,824)
        created=shot('layer-created')
        assert created.getpixel((35,610))[:3]==(25,27,32), 'New layer was not selected'
        key('z');key('y')
        subprocess.run(['xdotool','mousemove','100','616','mousedown','1'],check=True)
        time.sleep(.15)
        subprocess.run(['xdotool','mousemove','100','790'],check=True)
        time.sleep(.2)
        subprocess.run(['xdotool','mouseup','1'],check=True)
        time.sleep(.3)
        reordered=shot('layers-reordered')
        assert reordered.getpixel((35,790))[:3]==(25,27,32),'Dragging the layer did not reorder it'
        before_tempo=shot('tempo-before').crop((700,546,1000,577))
        subprocess.run(['xdotool','mousemove','815','559','click','--repeat','2','--delay','100','1'],check=True)
        time.sleep(.3)
        after_tempo=shot('tempo-starts').crop((700,546,1000,577))
        assert ImageChops.difference(before_tempo,after_tempo).getbbox(),'Double click did not create a tempo start'
        click(830,559,3);shot('tempo-menu')
        edit(850,575,90);key('Escape');shot('tempo-edited')
        # Zoom and pan operate on the ruler, with no separate slider.
        initial=shot('timeline-before-zoom').crop((225,442,1400,474))
        subprocess.run(['xdotool','mousemove','900','453','keydown','ctrl','click','--repeat','4','--delay','60','4','keyup','ctrl'],check=True)
        time.sleep(.5)
        zoomed=shot('timeline-zoomed').crop((225,442,1400,474))
        assert ImageChops.difference(initial,zoomed).getbbox(),'Timeline gesture did not zoom'
        before_pan=shot('timeline-before-pan').crop((225,442,1400,474))
        subprocess.run(['xdotool','mousemove','1050','453','mousedown','2'],check=True)
        time.sleep(.1)
        subprocess.run(['xdotool','mousemove','1140','453'],check=True)
        time.sleep(.2)
        subprocess.run(['xdotool','mouseup','2'],check=True)
        after_pan=shot('timeline-panned').crop((225,442,1400,474))
        assert ImageChops.difference(before_pan,after_pan).getbbox(),'Timeline gesture did not pan'
        assert process.poll() is None
    finally:
        process.terminate();process.wait(timeout=10)
    # New process: preferences survive restart and pen sizes remain tool/layer specific.
    process=start(env,False)
    try:
        ink0=cursor_size();key('l');liquid0=cursor_size()
        assert liquid0>ink0*10,'Liquify default is not substantially larger'
        key('bracketright');liquid0_changed=cursor_size();assert liquid0_changed>liquid0
        key('w');liquid1=cursor_size();assert liquid1>liquid0*1.4,'Global liquify default was not used by a new layer'
        key('l');ink1=cursor_size();assert ink1>50,'Global ink default was not used by a new project'
        key('s');assert abs(cursor_size()-ink0)<=2,'Ink size leaked across layers'
        key('l');assert abs(cursor_size()-liquid0_changed)<=2,'Liquify size did not survive tool/layer switches'
        key('ctrl+comma');click(90,268);shot('preferences-restored')
        assert json.loads(prefs.read_text())['video_delay_ms']==-125
        click(72,51)
        returned=shot('back-to-canvas')
        assert max(returned.getpixel((300,50))[:3])<80 and min(returned.getpixel((600,400))[:3])>240,'Settings Back did not return to P1'
        key('d');key('d');key('d');key('alt+2')
        origin=playhead(shot('pause-origin'))
        key('space');time.sleep(.35);key('space')
        assert abs(playhead(shot('pause-returned'))-origin)<=1,'Default pause did not return to playback origin'
        key('ctrl+comma');click(90,268)
        panel=shot('pause-settings')
        # Find the filled pause dropdown below the offset explanations.
        runs=[];run=None
        for y in range(235,400):
            r,g,b=panel.convert('RGB').getpixel((1007,y))
            active=20<r<75 and 20<g<80 and 20<b<100
            if active and run is None:run=y
            if not active and run is not None:
                if y-run>10:runs.append((run,y))
                run=None
        assert runs,'Pause dropdown is missing'
        y0,y1=runs[-1];cy=(y0+y1)//2
        click(940,cy);popup=shot('pause-options');dark_surface(popup,(850,cy+25,1000,cy+75))
        click(920,cy+74)
        assert json.loads(prefs.read_text())['pause_behavior']=='KeepPlayhead','Pause option was not persisted'
        click(72,51);key('space');time.sleep(.35);key('space')
        assert playhead(shot('pause-kept'))>origin+20,'Keep-playhead pause still returned to origin'
        assert process.poll() is None
    finally:
        process.terminate();process.wait(timeout=10)
print('Pages, dark popups, Settings Back, pause modes, preferences, brush memory, undo/redo and timeline gestures: passed')
