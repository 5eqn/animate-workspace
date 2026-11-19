#version 330 core
layout(location=0) in vec2 position;
layout(location=1) in vec2 direction;
uniform vec2 canvas;
uniform vec2 translation;
out vec2 flow;
void main() {
    flow=direction;
    gl_Position=vec4((position.x+translation.x)/canvas.x*2.0-1.0,1.0-(position.y+translation.y)/canvas.y*2.0,0.0,1.0);
}
