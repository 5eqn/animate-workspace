#!/usr/bin/env python3
"""Real desktop test: display-rate cap, input preservation and idle sleep.

Run under Xvfb at 1440 x 960. Uses the release app and isolated preferences.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from PIL import ImageGrab

root=Path(__file__).resolve().parent.parent
binary=root/os.environ.get('MORI_BINARY','target/release/mori')
out=root/'artifacts/ui';out.mkdir(parents=True,exist_ok=True)
for maximum in [0,30]:
    with tempfile.TemporaryDirectory(prefix='mori-pacing-') as temporary:
        temp=Path(temporary)
        prefs=temp/'preferences.json';prefs.write_text(json.dumps({'frame_rate':maximum,'language':'English'}))
        with (temp/'trace.log').open('w+') as log:
            process=subprocess.Popen([str(binary),'--debug-fps','--trace-frames'],env=dict(os.environ,MORI_PREFERENCES_PATH=str(prefs)),stderr=log)
            try:
                time.sleep(1)
                window=subprocess.check_output(['xdotool','search','--sync','--pid',str(process.pid)],text=True).splitlines()[0]
                subprocess.run(['xdotool','windowfocus',window],check=True)
                start=time.monotonic();i=0
                while time.monotonic()-start<6:
                    subprocess.run(['xdotool','mousemove',str(350+i%600),str(400+i%120)],check=True)
                    i+=1
                    time.sleep(.002)
                ImageGrab.grab(xdisplay=os.environ['DISPLAY']).save(out/f'pacing-{maximum or 60}.png')
                log.flush();log.seek(0);trace=log.read()
                reports=re.findall(r'MORI frames=(\d+) moves=(\d+)',trace)
                assert len(reports)>=2,trace
                frames,moves=map(int,reports[-1]);target=maximum or 60
                assert target*1.65<=frames<=target*2.1,(maximum,frames,trace)
                assert moves>frames*1.3,('Input appears limited by paint rate',frames,moves)
                # Once the pointer settles, the application must stop painting.
                time.sleep(.8)
                def ticks():
                    return sum(map(int,Path(f'/proc/{process.pid}/stat').read_text().split()[13:15]))
                before=ticks();time.sleep(1.5);idle_ticks=ticks()-before
                assert idle_ticks<=max(4,os.sysconf('SC_CLK_TCK')*.08),('Application stayed busy while idle',idle_ticks)
                print(f'target={target} Hz: {frames} updates / {moves} pointer events in ~2 s; idle CPU ticks={idle_ticks}',flush=True)
            finally:
                process.terminate();process.wait(timeout=5)
print('Display-rate cap, input preservation and idle sleep passed')
