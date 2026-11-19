#!/usr/bin/env python3
"""Whole-stroke erasing, held exposures, undo grouping and playback following.
Run with a 1440x960 Xvfb screen; requires Pillow and xdotool.
"""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import time
from PIL import ImageGrab, ImageChops

root = Path(__file__).resolve().parent.parent
out = root / 'artifacts/ui'
out.mkdir(parents=True, exist_ok=True)
binary = str(root / os.environ.get('MORI_BINARY', 'target/debug/mori'))

def xd(*args):
    subprocess.run(['xdotool', *map(str, args)], check=True)

def key(value):
    xd('key', '--clearmodifiers', value)
    time.sleep(.2)

def click(x, y):
    xd('mousemove', x, y, 'click', 1)
    time.sleep(.2)

def shot(name):
    # Keep the cursor out of the assertions' drawing regions.
    xd('mousemove', 35, 35)
    time.sleep(.1)
    image = ImageGrab.grab(xdisplay=os.environ['DISPLAY']).convert('RGB')
    image.save(out / (name + '.png'))
    return image

def drag(a, b):
    xd('mousemove', *a, 'mousedown', 1)
    time.sleep(.08)
    xd('mousemove', *b)
    time.sleep(.08)
    xd('mouseup', 1)
    time.sleep(.2)

def brush(x, y):
    xd('mousemove', 720, 480, 'mousedown', 3)
    time.sleep(.08)
    xd('mousemove', x, y)
    time.sleep(.08)
    xd('mouseup', 3)
    time.sleep(.15)

def ink(image, bounds):
    return sum(max(p) < 100 for p in image.crop(bounds).getdata())

def launch(env, demo=False):
    app = subprocess.Popen([binary] + (['--demo'] if demo else []), cwd=root, env=env)
    time.sleep(1)
    window = subprocess.check_output(['xdotool', 'search', '--sync', '--pid', str(app.pid)], text=True).splitlines()[0]
    xd('windowfocus', window)
    return app

with tempfile.TemporaryDirectory(prefix='mori-erase-follow-') as temp:
    prefs = Path(temp) / 'preferences.json'
    env = dict(os.environ, MORI_PREFERENCES_PATH=str(prefs), MORI_SYSTEM_LOCALE='en-US')
    app = launch(env)
    try:
        key('o')  # Hide onion ghosts while checking erased ink.
        drag((450, 400), (650, 400))
        drag((800, 520), (1000, 520))
        before = shot('erase-before')
        a, b = (440, 390, 660, 410), (790, 510, 1010, 530)
        assert ink(before, a) > 300 and ink(before, b) > 300
        key('d')  # The new time holds the earlier drawing.
        brush(720, 535)  # Select Eraser on the six-tool wheel.
        drag((540, 350), (540, 450))  # Both endpoints miss; the sweep must hit.
        erased = shot('erase-held')
        assert ink(erased, a) == 0, 'A whole stroke was not erased by the sweep'
        assert ink(erased, b) == ink(before, b), 'Untouched stroke changed'
        key('ctrl+z')
        undone = shot('erase-undone')
        assert ink(undone, a) == ink(before, a) and ink(undone, b) == ink(before, b)
        key('ctrl+shift+z')
        assert ink(shot('erase-redone'), a) == 0
        key('a')
        assert ink(shot('erase-prior-exposure'), a) == ink(before, a), 'Erasing changed the prior exposure'
        key('d')
        click(400, 600)  # A miss must leave no empty undo entry.
        key('ctrl+z')
        assert ink(shot('erase-miss-undo'), a) == ink(before, a), 'An eraser miss consumed an undo step'
        # One gesture touches both strokes and must undo as a single operation.
        key('a')  # Erase the earliest drawing; no earlier drawing remains to hold.
        drag((540, 400), (900, 520))
        both = shot('erase-two-strokes')
        assert ink(both, a) == 0 and ink(both, b) == 0
        key('ctrl+z')
        undone = shot('erase-two-strokes-undone')
        assert ink(undone, a) == ink(before, a) and ink(undone, b) == ink(before, b)
        assert app.poll() is None
    finally:
        app.terminate()
        app.wait(timeout=10)

    for follow in [True, False]:
        data = json.loads(prefs.read_text())
        data['timeline_follow'] = follow
        prefs.write_text(json.dumps(data))
        app = launch(env, demo=True)
        try:
            key('alt+2')
            xd('mousemove', 800, 453, 'keydown', 'ctrl', 'click', '--repeat', 3, '--delay', 40, 4, 'keyup', 'ctrl')
            time.sleep(.5)
            # Start at zero and allow the follow behavior to settle for one frame.
            before_play = shot('follow-before-play-' + str(follow))
            key('space')
            time.sleep(.1)
            first = shot('follow-start-' + str(follow))
            time.sleep(.7)
            second = shot('follow-playing-' + str(follow))
            # Compare the ruler ticks, excluding the moving, bright playhead.
            ticks = []
            for frame in [first, second]:
                image = frame.crop((225, 465, 1400, 473))
                image.putdata([(17, 18, 20) if max(p) > 60 else p for p in image.getdata()])
                ticks.append(image)
            moved = ImageChops.difference(*ticks).getbbox()
            assert bool(moved) == follow, 'The timeline follow preference did not control scrolling'
            if follow:
                # At maximum zoom the picture may hold one exposure across many
                # screen widths; the continuous audio playhead must stay visible.
                xd('mousemove', 800, 453, 'keydown', 'ctrl', 'click', '--repeat', 10, '--delay', 20, 4, 'keyup', 'ctrl')
                time.sleep(.1)
                zoomed = shot('follow-maximum-zoom')
                assert max(zoomed.getpixel((x, 920))[2] for x in range(225, 1400)) > 190, 'Playhead escaped the view at maximum zoom'
            key('space')
            returned = shot('follow-paused-view-' + str(follow))
            before_ticks = before_play.crop((225, 465, 1400, 473))
            after_ticks = returned.crop((225, 465, 1400, 473))
            for image in [before_ticks, after_ticks]:
                image.putdata([(17, 18, 20) if max(p) > 60 else p for p in image.getdata()])
            assert ImageChops.difference(before_ticks, after_ticks).getbbox() is None, 'Pause did not restore the timeline view'
            key('ctrl+comma')
            click(90, 268)
            shot('follow-preference-' + str(follow))
            click(998, 336)
            assert json.loads(prefs.read_text())['timeline_follow'] == (not follow), 'Follow toggle did not persist'
        finally:
            app.terminate()
            app.wait(timeout=10)
print('Whole-stroke erasing, held exposures, undo grouping, misses and configurable playback following: passed')
