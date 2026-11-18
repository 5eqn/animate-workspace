#!/usr/bin/env python3
"""Verify system Chinese, English override, restore defaults, and save doc screenshots."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import time
from PIL import ImageGrab, ImageChops

root=Path(__file__).resolve().parent.parent
out=root/'artifacts/ui'
out.mkdir(parents=True,exist_ok=True)
binary=str(root/os.environ.get('MORI_BINARY','target/debug/mori'))
def key(value):
    subprocess.run(['xdotool','key','--clearmodifiers',value],check=True);time.sleep(.3)
def click(x,y):
    subprocess.run(['xdotool','mousemove',str(x),str(y),'click','1'],check=True);time.sleep(.3)
def shot(name):
    image=ImageGrab.grab(xdisplay=os.environ['DISPLAY']);image.save(out/(name+'.png'));return image

def launch(env):
    app=subprocess.Popen([binary,'--demo'],cwd=root,env=env)
    time.sleep(1)
    window=subprocess.check_output(['xdotool','search','--sync','--pid',str(app.pid)],text=True).splitlines()[0]
    subprocess.run(['xdotool','windowfocus',window],check=True)
    return app

with tempfile.TemporaryDirectory(prefix='mori-language-') as temp:
    path=Path(temp)/'preferences.json'
    env=dict(os.environ,MORI_SYSTEM_LOCALE='zh-CN',MORI_PREFERENCES_PATH=str(path))
    app=launch(env)
    try:
        key('alt+2');shot('rhythm-zh')
        key('ctrl+comma');shot('settings-zh')
        click(90,172);shot('brushes-zh')
        click(90,412);zh=shot('general-zh')
        click(950,152);shot('language-options-zh')
        click(910,232)
        prefs=json.loads(path.read_text());assert prefs['language']=='English','English language override did not persist'
        en=shot('general-en')
        assert ImageChops.difference(zh.crop((270,40,680,180)),en.crop((270,40,680,180))).getbbox(),'Language switch did not redraw the interface'
    finally:
        app.terminate();app.wait(timeout=10)
    app=launch(env)
    try:
        key('ctrl+comma');click(90,412);restored=shot('language-restored')
        assert ImageChops.difference(en.crop((270,40,680,180)),restored.crop((270,40,680,180))).getbbox() is None,'English override was lost on restart'
        # Give the project a local change, then reset only global preferences.
        click(90,124)
        subprocess.run(['xdotool','mousemove','975','203','click','--repeat','2','--delay','80','1'],check=True)
        key('ctrl+a');subprocess.run(['xdotool','type','--','1.85'],check=True);key('Return')
        click(90,124)
        project_before=shot('project-before-preference-reset')
        click(90,412);click(380,326);shot('restore-defaults-confirmation')
        # The confirmation is centered; activate Restore defaults.
        click(687,539)
        if json.loads(path.read_text())['language']!='System':
            shot('restore-defaults-click-missed')
            raise AssertionError('Restore defaults did not reset the global language preference')
        shot('defaults-restored-zh')
        click(90,124);project_after=shot('project-after-preference-reset')
        assert ImageChops.difference(project_before.crop((940,185,1015,220)),project_after.crop((940,185,1015,220))).getbbox() is None,'Reset changed a project-local aspect ratio'
        assert json.loads(path.read_text())['minimum_pressure']==0.0
    finally:
        app.terminate();app.wait(timeout=10)
print('System Chinese, English override, persistence and global preference reset: passed')
