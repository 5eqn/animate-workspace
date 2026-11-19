#version 330 core
in vec2 uv;
out vec4 color;
uniform sampler2D below;
uniform sampler2D ink;
uniform int mode;
uniform int style;
uniform bool inverted;
uniform vec2 motion;
uniform vec2 canvas;
uniform float strength;
uniform ivec2 rhythm_grid;
uniform int rhythm_filled;
uniform ivec2 pixel_size;
vec3 paper(vec2 p) {
    if (any(lessThan(p,vec2(0.0))) || any(greaterThan(p,vec2(1.0)))) return vec3(1.0);
    return texture(below,p).rgb;
}
vec3 hsv(vec3 c) {
    vec3 p=abs(fract(c.xxx+vec3(0.0,2.0/3.0,1.0/3.0))*6.0-3.0);
    return c.z*mix(vec3(1.0),clamp(p-1.0,0.0,1.0),c.y);
}
void main() {
    if(mode==6){
        vec3 base=paper(uv);
        int cell=max(2,int(floor(float(pixel_size.x)*5.0/1920.0)));
        int gap=max(1,int(floor(float(pixel_size.x)*2.0/1920.0)));
        int margin=max(8,int(floor(float(pixel_size.x)*24.0/1920.0)));
        ivec2 extent=rhythm_grid*(cell+gap)-gap;
        // All edges are integer framebuffer coordinates; no filtering/AA on the blocks.
        ivec2 origin=ivec2(pixel_size.x-margin-extent.x,margin);
        ivec2 p=ivec2(floor(gl_FragCoord.xy))-origin;
        if(all(greaterThanEqual(p,ivec2(0)))&&all(lessThan(p,extent))){
            ivec2 index=p/(cell+gap);ivec2 local=p%(cell+gap);
            if(all(lessThan(local,ivec2(cell)))){
                int beat=(rhythm_grid.y-1-index.y)*rhythm_grid.x+index.x;
                base=vec3(beat<rhythm_filled?0.0:0.80);
            }
        }
        color=vec4(base,1.0);return;
    }

    vec3 a=paper(uv);
    vec3 b=vec3(texture(ink,uv).r);
    if (inverted) b=1.0-b;
    vec2 d=motion*strength;
    if (mode==0) color=vec4(b,1.0);
    if (mode==1) color=vec4(1.0-abs(a-b),1.0);
    if (mode==2) color=vec4(min(a,b),1.0);
    if (mode==3) color=vec4(max(a,b),1.0);
    if (mode==4) {
        vec3 moved=paper(uv-d);
        if(style==1) { for(int i=0;i<7;i++) moved=min(moved,mix(vec3(1.0),paper(uv-d*float(i)/6.0),0.25+0.75*float(i)/6.0)); }
        if(style==2) moved=vec3(paper(uv-d*0.94).r,paper(uv-d).g,paper(uv-d*1.06).b);
        if(style==3) {
            moved=vec3(1.0);
            for(int i=0;i<12;i++) { float t=float(i)/11.0; float mask=1.0-dot(paper(uv-d*t),vec3(0.333333));
                vec3 tint=hsv(vec3(t*0.7+0.5,0.85,0.95)); moved=min(moved,mix(vec3(1.0),tint,mask*(0.25+0.75*t))); }
        }
        color=vec4(inverted ? 1.0-moved : moved,1.0);
    }
    if (mode==5) {
        vec4 field=texture(ink,uv);
        float coverage=field.a;
        if(coverage<=0.0) { color=vec4(a,1.0); return; }
        d=vec2(field.x/canvas.x,-field.y/canvas.y)/coverage*strength;
        // Continuous directional shutter integration. No Gaussian kernel.
        vec3 blurred=vec3(0.0); float weight=0.0;
        for(int i=0;i<32;i++) {
            float t=float(i)/31.0-0.5; float w=1.0;
            if(style==1) w=pow(0.5+0.5*cos(t*6.2831853*6.0),8.0);
            vec3 sampleColor=paper(uv-d*t);
            if(style==2) sampleColor=vec3(paper(uv-d*(t-0.12)).r,sampleColor.g,paper(uv-d*(t+0.12)).b);
            if(style==3) sampleColor=mix(vec3(1.0),hsv(vec3(t+0.5,0.9,0.95)),1.0-dot(sampleColor,vec3(0.333333)));
            blurred+=sampleColor*w; weight+=w;
        }
        vec3 result=inverted ? 1.0-blurred/weight : blurred/weight;
        color=vec4(mix(a,result,coverage),1.0);
    }
}
