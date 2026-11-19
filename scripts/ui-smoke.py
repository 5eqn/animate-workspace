#!/usr/bin/env python3
"""Run under Xvfb; exercise the real desktop and keep preferences isolated."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import time
from PIL import ImageGrab, ImageChops

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

def brush(name):
    directions={'round':(720,425),'marker':(763,446),'fill':(774,492),'eraser':(744,530),'liquify':(696,530),'move':(666,492),'scale':(677,446)}
    subprocess.run(['xdotool','mousemove','720','480','mousedown','3'],check=True)
    time.sleep(.1)
    x,y=directions[name]
    subprocess.run(['xdotool','mousemove',str(x),str(y)],check=True)
    time.sleep(.12)
    if name=='marker':shot('brush-wheel')
    subprocess.run(['xdotool','mouseup','3'],check=True)
    time.sleep(.2)

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

with tempfile.TemporaryDirectory(prefix='mori-ui-') as temporary:
    prefs=Path(temporary)/'preferences.json'
    env=dict(os.environ,MORI_SYSTEM_LOCALE='en-US',MORI_PREFERENCES_PATH=str(prefs))
    process=start(env)
    try:
        subprocess.run(['xdotool','mousemove','30','30'],check=True)
        shot('canvas')
        key('alt+2');shot('rhythm')
        # Hover both a track and its precise frame handle.
        clear=shot('track-before-hover')
        subprocess.run(['xdotool','mousemove','600','733'],check=True);time.sleep(.2)
        hovered=shot('track-hover')
        assert ImageChops.difference(clear.crop((450,710,850,755)),hovered.crop((450,710,850,755))).getbbox(),'Track has no hover feedback'
        subprocess.run(['xdotool','mousemove','327','733'],check=True);time.sleep(.2)
        head=shot('frame-head-hover')
        assert sum(min(p[:3])>180 for p in head.crop((322,728,332,739)).getdata())>sum(min(p[:3])>180 for p in clear.crop((322,728,332,739)).getdata())+4,'Frame head did not highlight'
        # Every lane scrubs during the gesture, before release.
        for y in [453,510,560,615,675,733,791,880]:
            subprocess.run(['xdotool','mousemove','600',str(y),'mousedown','1'],check=True);time.sleep(.1)
            subprocess.run(['xdotool','mousemove','950',str(y)],check=True);time.sleep(.2)
            during=shot('scrub-'+str(y))
            assert abs(playhead(during)-950)<16,'Timeline lane did not scrub while dragging: '+str(y)
            subprocess.run(['xdotool','mouseup','1'],check=True);time.sleep(.1)
        # Only the handle starts frame movement; body dragging above only scrubs.
        before=shot('before-frame-drag')
        subprocess.run(['xdotool','mousemove','327','733','mousedown','1'],check=True);time.sleep(.1)
        subprocess.run(['xdotool','mousemove','500','733'],check=True);time.sleep(.2)
        shot('frame-drag-preview')
        subprocess.run(['xdotool','mouseup','1'],check=True);time.sleep(.2)
        moved=shot('frame-moved')
        assert ImageChops.difference(before.crop((300,717,520,750)),moved.crop((300,717,520,750))).getbbox(),'Frame handle did not move the frame'
        click(90,615,3);menu=shot('layer-menu');dark_surface(menu,(95,570,320,780));key('Escape')
        click(260,559,3);shot('tempo-menu');edit(478,622,4);shot('tempo-division-edited');key('Escape')
        key('ctrl+comma')
        for i,name in enumerate(['project','brushes','onion','playback','export','shortcuts','general']):
            click(90,124+i*48);shot('settings-'+name)
        click(90,124)
        click(998,516);assert not prefs.exists() or json.loads(prefs.read_text())['default_watermark'] is True
        click(998,566);assert json.loads(prefs.read_text())['default_watermark'] is False
        click(998,516);click(998,566)
        edit(978,409,16)
        assert json.loads(prefs.read_text())['default_step']==0.0625
        click(90,268);edit(978,152,-125)
        assert json.loads(prefs.read_text())['video_delay_ms']==-125
        click(72,51);assert min(shot('back-to-timeline').getpixel((300,50))[:3])>240
        initial=shot('timeline-before-zoom').crop((225,442,1400,474))
        subprocess.run(['xdotool','mousemove','900','453','keydown','ctrl','click','--repeat','3','--delay','60','4','keyup','ctrl'],check=True);time.sleep(.4)
        zoomed=shot('timeline-zoomed').crop((225,442,1400,474))
        assert ImageChops.difference(initial,zoomed).getbbox()
        subprocess.run(['xdotool','mousemove','1050','453','mousedown','2'],check=True);time.sleep(.1)
        subprocess.run(['xdotool','mousemove','1140','453'],check=True);time.sleep(.2)
        subprocess.run(['xdotool','mouseup','2'],check=True)
        assert ImageChops.difference(zoomed,shot('timeline-panned').crop((225,442,1400,474))).getbbox()
        assert process.poll() is None
    finally:
        process.terminate();process.wait(timeout=10)
    process=start(env,False)
    try:
        assert json.loads(prefs.read_text())['video_delay_ms']==-125
        assert json.loads(prefs.read_text())['default_step']==0.0625
        def selected_brush():
            subprocess.run(['xdotool','mousemove','720','480','mousedown','3'],check=True);time.sleep(.2)
            picture=shot('brush-memory')
            names=['round','marker','fill','eraser','liquify','move','scale']
            import math
            colors=[picture.getpixel((round(720+math.cos(-math.pi/2+i*math.tau/7)*70),round(480+math.sin(-math.pi/2+i*math.tau/7)*70))) for i in range(7)]
            subprocess.run(['xdotool','mouseup','3'],check=True);time.sleep(.1)
            return names[max(range(7),key=lambda i:colors[i][2])]
        assert selected_brush()=='round'
        key('w');assert selected_brush()=='marker','Second layer is not Marker'
        brush('liquify');key('s');assert selected_brush()=='round','Brush leaked to the previous layer'
        key('w');assert selected_brush()=='liquify','Layer did not remember its brush'
        key('s');key('o');brush('fill');click(500,400)
        black=shot('fill-black');assert max(black.getpixel((1000,600))[:3])<5
        key('d');brush('round');click(500,400)
        assert min(shot('new-drawing').getpixel((1000,600))[:3])>240
        brush('eraser');click(500,400)
        assert min(shot('empty-frame-retained').getpixel((1000,600))[:3])>240,'Erasing removed the empty frame'
        key('ctrl+z');assert min(shot('empty-frame-undo').getpixel((1000,600))[:3])>240
        key('w');key('w');assert selected_brush()=='marker','Motion blur layer is not Marker'
        brush('move');assert selected_brush()=='move'
        assert process.poll() is None
    finally:
        process.terminate();process.wait(timeout=10)
print('Track and frame-head feedback, all-lane scrubbing, frame handles, local brushes, Fill and retained empty frames: passed')
