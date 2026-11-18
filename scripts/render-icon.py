#!/usr/bin/env python3
"""Rasterize MORI's simple geometric icon (canonical vector: assets/icon.svg)."""
from pathlib import Path
from math import pi, cos, sin
from PIL import Image, ImageDraw
size = 2048
scale = size / 1024
image = Image.new('RGBA', (size, size))
draw = ImageDraw.Draw(image)
draw.rounded_rectangle((48*scale,48*scale,976*scale,976*scale), radius=220*scale, fill='#191b1d')
for degrees in [-40,15,70]:
    angle = degrees*pi/180
    points = []
    for index in range(721):
        t = index/720*2*pi
        x,y = 310*cos(t),130*sin(t)
        points.append(((512+x*cos(angle)-y*sin(angle))*scale,(512+x*sin(angle)+y*cos(angle))*scale))
    draw.line(points,fill='#e8e7e1',width=round(17*scale),joint='curve')
draw.ellipse((475*scale,475*scale,549*scale,549*scale),fill='#d6b184')
image.resize((1024,1024),Image.Resampling.LANCZOS).save(Path(__file__).resolve().parent.parent/'assets/icon.png')
