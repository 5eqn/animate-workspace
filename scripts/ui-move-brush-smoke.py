#!/usr/bin/env python3
"""Move a held drawing, edit translated strokes, undo and keep other frames/layers.
Run under a 1440 x 960 Xvfb display with Pillow and xdotool.
"""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import time
from PIL import ImageGrab
root=Path(__file__).resolve().parent.parent
binary=root/os.environ.get('MORI_BINARY','target/release/mori')
out=root/'artifacts/ui';out.mkdir(parents=True,exist_ok=True)
def xd(*args):subprocess.run(['xdotool',*map(str,args)],check=True)
def key(value):xd('key','--clearmodifiers',value);time.sleep(.2)
def click(x,y):xd('mousemove',x,y,'click',1);time.sleep(.2)
def drag(a,b):
    xd('mousemove',*a,'mousedown',1);time.sleep(.08)
    for i in range(1,13):
        xd('mousemove',round(a[0]+(b[0]-a[0])*i/12),round(a[1]+(b[1]-a[1])*i/12));time.sleep(.012)
    xd('mouseup',1);time.sleep(.2)
def brush(name):
    choices={'round':(720,425),'marker':(768,452),'fill':(768,508),'eraser':(720,535),'liquify':(672,508),'move':(672,452)}
    xd('mousemove',720,480,'mousedown',3);time.sleep(.08)
    xd('mousemove',*choices[name]);time.sleep(.08);xd('mouseup',3);time.sleep(.15)
def shot(name):
    xd('mousemove',30,30);time.sleep(.1)
    im=ImageGrab.grab(xdisplay=os.environ['DISPLAY']).convert('RGB');im.save(out/(name+'.png'));return im
def ink(im,region):return sum(max(p)<100 for p in im.crop(region).getdata())
with tempfile.TemporaryDirectory(prefix='mori-move-') as temp:
    prefs=Path(temp)/'prefs.json';prefs.write_text(json.dumps({'language':'English','onion':{'enabled':False}}))
    app=subprocess.Popen([str(binary)],env=dict(os.environ,MORI_PREFERENCES_PATH=str(prefs),MORI_SYSTEM_LOCALE='en-US'))
    try:
        time.sleep(1)
        window=subprocess.check_output(['xdotool','search','--sync','--pid',str(app.pid)],text=True).splitlines()[0];xd('windowfocus',window)
        # Two strokes on layer 1, and an untouched mark on layer 2.
        drag((380,360),(520,360));drag((430,430),(550,430))
        key('w');click(980,600);key('s')
        before=shot('move-brush-before');old=(360,345,570,445);new=(560,405,770,505);other=(940,570,1020,630)
        assert ink(before,old)>300 and ink(before,other)>10
        key('d');brush('move');drag((600,520),(800,580))
        moved=shot('move-brush-held');assert ink(moved,old)==0,'Move left original strokes behind'
        assert ink(moved,new)>300,'Move did not translate all strokes'
        assert ink(moved,other)==ink(before,other),'Move changed another layer'
        key('a');prior=shot('move-brush-prior');assert ink(prior,old)==ink(before,old),'Move modified the held source'
        key('d');key('ctrl+z');assert ink(shot('move-brush-undo'),old)==ink(before,old)
        key('ctrl+shift+z');assert ink(shot('move-brush-redo'),old)==0
        # New ink at a translated frame uses world pointer coordinates.
        brush('round');click(850,400);assert ink(shot('move-brush-new-ink'),(840,390,860,410))>5
        brush('eraser');drag((650,385),(650,455))
        erased=shot('move-brush-erased');assert ink(erased,(570,410,730,430))==0,'Eraser missed translated stroke'
        assert ink(erased,(620,480,760,500))>100,'Eraser removed an unrelated translated stroke'
        key('ctrl+z');assert ink(shot('move-brush-erase-undo'),(570,410,730,430))>100
        brush('liquify');drag((650,420),(650,455))
        liquified=shot('move-brush-liquified');assert ink(liquified,(625,432,675,465))>5,'Liquify ignored the translated drawing'
        # The infinite Fill brush remains infinite after moving it.
        key('a');key('Delete');brush('fill');click(500,400);brush('move');drag((600,400),(900,550))
        filled=shot('move-brush-fill');assert max(filled.getpixel((500,400)))<5 and max(filled.getpixel((1300,650)))<5
        assert app.poll() is None
    finally:app.terminate();app.wait(timeout=10)
print('Move brush: held-frame isolation, all-stroke translation, other layers, undo, translated ink/eraser/liquify and infinite Fill passed')
