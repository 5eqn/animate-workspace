#version 330 core
in vec2 uv;
out vec4 color;
uniform sampler2D image;
uniform bool mirror;
uniform bool tinting;
uniform vec3 tint;
uniform float opacity;
void main() {
    vec3 c=texture(image,vec2(mirror?1.0-uv.x:uv.x,uv.y)).rgb;
    if(tinting) color=vec4(tint,(1.0-dot(c,vec3(0.333333)))*opacity);
    else color=vec4(c,1.0);
}
