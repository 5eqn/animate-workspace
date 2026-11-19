#!/usr/bin/env python3
"""Two-step scale/rotate gestures edit vectors, preserve the pivot and reset state.
Run with a 1440 x 960 Xvfb display, Pillow and xdotool.
"""
from pathlib import Path
import json,os,subprocess,tempfile,time
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
 choices={'round':(720,425),'marker':(763,446),'eraser':(744,530),'liquify':(696,530),'move':(666,492),'scale':(677,446)}
 xd('mousemove',720,480,'mousedown',3);time.sleep(.08)
 xd('mousemove',*choices[name]);time.sleep(.08);xd('mouseup',3);time.sleep(.15)
def shot(name):
 xd('mousemove',30,30);time.sleep(.1)
 im=ImageGrab.grab(xdisplay=os.environ['DISPLAY']).convert('RGB');im.save(out/(name+'.png'));return im
def ink(im,x,y,r=12):return sum(max(p)<100 for p in im.crop((x-r,y-r,x+r+1,y+r+1)).getdata())
with tempfile.TemporaryDirectory(prefix='mori-scale-') as temp:
 prefs=Path(temp)/'prefs.json';prefs.write_text(json.dumps({'language':'English','onion':{'enabled':False,'previous':3,'next':3,'before':[200,100,100],'after':[100,140,200],'opacity':0.15,'falloff':0.6},'brush_defaults':{'ink':20}}))
 app=subprocess.Popen([str(binary)],env=dict(os.environ,MORI_PREFERENCES_PATH=str(prefs),MORI_SYSTEM_LOCALE='en-US'))
 try:
  time.sleep(1)
  window=subprocess.check_output(['xdotool','search','--sync','--pid',str(app.pid)],text=True).splitlines()[0];xd('windowfocus',window)
  click(720,480);click(760,520);before=shot('scale-before');n=ink(before,720,480);assert n>80
  key('d');brush('scale')
  # Center is chosen on release; the first gesture changes no vectors.
  drag((560,450),(600,480));centered=shot('scale-center');assert ink(centered,720,480)==n
  # A center click has no radius and must neither alter nor corrupt the drawing.
  click(600,480);assert ink(shot('scale-center-noop'),720,480)==n
  drag((720,480),(840,480));large=shot('scale-enlarged')
  assert ink(large,840,480)>n*2 and ink(large,720,480)==0,'Scaling did not follow the grabbed point or scale the nib'
  assert ink(large,920,560)>n*2,'Scaling did not include every stroke'
  # The same center remains, allowing shrink and rotation in one gesture.
  drag((840,480),(600,600));rotated=shot('scale-rotated-smaller')
  assert abs(ink(rotated,600,600)-n)<=4,'Rotated grab did not follow the pointer'
  assert abs(ink(rotated,560,640)-n)<=4,'Rotation did not preserve the other stroke'
  key('a');assert ink(shot('scale-previous-frame'),720,480)==n,'Scaling changed the held source'
  key('d');key('ctrl+z');assert ink(shot('scale-undo'),840,480)>n*2
  key('ctrl+shift+z');assert abs(ink(shot('scale-redo'),600,600)-n)<=4
  # Switching away and back to the tool must require a new center.
  brush('round');brush('scale');drag((900,480),(930,480));reset=shot('scale-tool-reset');assert abs(ink(reset,600,600)-n)<=4
  drag((600,600),(270,720));again=shot('scale-after-reset');assert ink(again,270,720)>n*2,'The new center was not used'
  # Switching layers and back must also reset the center.
  key('w');key('s');drag((1000,400),(1040,400));assert ink(shot('scale-layer-reset'),270,720)>n*2
  # Later ordinary brushes must act in the same canvas coordinates.
  brush('round');click(850,400);assert ink(shot('scale-new-ink'),850,400)>80
  brush('eraser');drag((270,690),(270,750));assert ink(shot('scale-erase'),270,720)==0
  key('ctrl+z');assert ink(shot('scale-erase-undo'),270,720)>n*2
  brush('liquify');drag((270,720),(320,720));assert ink(shot('scale-liquify'),310,720)>10
  assert app.poll() is None
 finally:app.terminate();app.wait(timeout=10)
print('Scale/rotate: release-set pivot, precise grab tracking, repeated shrink/rotate, held frames, undo, reset on brush/layer switches and normal editing passed')
