#version 330 core
in vec2 flow;
uniform bool flow_mode;
out vec4 color;
void main() { color=flow_mode ? vec4(flow,0.0,1.0) : vec4(0.0,0.0,0.0,1.0); }
