#!/usr/bin/env python3
"""Exercise actual Xdnd file delivery, waveform import and hover feedback under Xvfb.
Requires python3-xlib, Pillow and xdotool. No native file-dialog automation.
"""
from pathlib import Path
import math
import os
import select
import struct
import subprocess
import tempfile
import time
import wave
from PIL import ImageGrab, ImageChops
from Xlib import X, display, protocol

root=Path(__file__).resolve().parent.parent
out=root/'artifacts/ui'
out.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='mori-drop-') as temp:
    temp=Path(temp)
    audio=temp/'rhythm.wav'
    with wave.open(str(audio),'wb') as file:
        file.setnchannels(1);file.setsampwidth(2);file.setframerate(48000)
        file.writeframes(b''.join(struct.pack('<h',int(math.sin(i*440*2*math.pi/48000)*12000)) for i in range(48000)))
    env=dict(os.environ,MORI_PREFERENCES_PATH=str(temp/'preferences.json'))
    app=subprocess.Popen([str(root/os.environ.get('MORI_BINARY','target/debug/mori')),'--demo'],cwd=root,env=env)
    try:
        time.sleep(1)
        window_id=int(subprocess.check_output(['xdotool','search','--sync','--pid',str(app.pid)],text=True).splitlines()[0])
        subprocess.run(['xdotool','windowfocus',str(window_id),'key','alt+2'],check=True)
        time.sleep(.4)
        before=ImageGrab.grab(xdisplay=os.environ['DISPLAY'])
        subprocess.run(['xdotool','mousemove','850','510'],check=True)
        time.sleep(.25)
        hover=ImageGrab.grab(xdisplay=os.environ['DISPLAY']);hover.save(out/'audio-hover.png')
        assert ImageChops.difference(before.crop((240,485,1390,538)),hover.crop((240,485,1390,538))).getbbox(),'Audio import area has no hover feedback'
        connection=display.Display()
        screen=connection.screen()
        source=screen.root.create_window(0,0,1,1,0,screen.root_depth,X.InputOutput,X.CopyFromParent)
        target=connection.create_resource_object('window',window_id)
        atom=lambda name:connection.intern_atom(name)
        selection=atom('XdndSelection');uri=atom('text/uri-list');copy=atom('XdndActionCopy')
        source.set_selection_owner(selection,X.CurrentTime)
        def send(name,data):
            target.send_event(protocol.event.ClientMessage(window=target,client_type=atom(name),data=(32,data)),event_mask=0)
            connection.flush()
        send('XdndEnter',[source.id,5<<24,uri,0,0])
        send('XdndPosition',[source.id,0,(850<<16)|510,X.CurrentTime,copy])
        received=False
        deadline=time.monotonic()+3
        while time.monotonic()<deadline:
            if not connection.pending_events():
                select.select([connection.fileno()],[],[],.05)
                continue
            event=connection.next_event()
            if event.type==X.SelectionRequest:
                requestor=connection.create_resource_object('window',event.requestor.id)
                prop=event.property or event.target
                requestor.change_property(prop,uri,8,(audio.as_uri()+'\r\n').encode())
                requestor.send_event(protocol.event.SelectionNotify(time=event.time,requestor=requestor,selection=event.selection,target=event.target,property=prop),event_mask=0)
                connection.flush();received=True;break
        assert received,'Application did not request the drag file payload'
        time.sleep(.25)
        ImageGrab.grab(xdisplay=os.environ['DISPLAY']).save(out/'audio-drop-hover.png')
        send('XdndDrop',[source.id,0,X.CurrentTime,0,0])
        time.sleep(.8)
        after=ImageGrab.grab(xdisplay=os.environ['DISPLAY']);after.save(out/'audio-imported.png')
        waveform=after.convert('RGB').crop((240,485,1390,538))
        count=sum(85<r<145 and 90<g<155 and 115<b<180 for r,g,b in waveform.getdata())
        assert count>5000,'File drop did not decode and display the waveform'
        assert app.poll() is None
        connection.close()
        print('Native file drop, audio decoding, waveform and import hover feedback: passed')
    finally:
        app.terminate();app.wait(timeout=10)
