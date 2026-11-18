#version 330 core
layout(location=0) in vec2 position;
uniform vec2 canvas;
void main() { gl_Position=vec4(position.x/canvas.x*2.0-1.0,1.0-position.y/canvas.y*2.0,0.0,1.0); }
