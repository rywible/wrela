#include <metal_stdlib>
using namespace metal;
constant int materialKind [[function_constant(0)]];
constant float PI=3.14159265359;
struct Vertex {float4 position;float4 normal;float4 color;};
struct Instance {float4x4 model;float4 tint;};
struct Uniforms {float4x4 viewProjection;float4x4 lightVP;float4x4 inverseVP;float4 cameraTime;float4 sunExposure;float4 options;float4 sky;float4 environment;float4 weatherBounds;float4 skyTimes;float4 rigPosition;float4 rigColor;float4 rigParams;float4 lookSurface;float4 lookLight;float4 lookGrade;};
float3 artSaturation(float3 c,float amount){float y=dot(c,float3(.2126,.7152,.0722));return max(mix(float3(y),c,amount),0.);}
float3 artSky(float3 c,constant Uniforms &u){return artSaturation(c,u.lookLight.z)*u.lookLight.x;}
// ATMOSPHERE
struct Varying {float4 position [[position]];float3 world;float3 local;float3 normal;float3 color;float4 shadow;float kind;float ao;};
float hash(float2 p){return fract(sin(dot(p,float2(127.1,311.7)))*43758.5453);}
float noise(float2 p){float2 i=floor(p),f=fract(p);f=f*f*(3-2*f);return mix(mix(hash(i),hash(i+float2(1,0)),f.x),mix(hash(i+float2(0,1)),hash(i+1),f.x),f.y);}
float filteredNoise(float2 p){float footprint=max(length(dfdx(p)),length(dfdy(p)));return mix(noise(p),.5,smoothstep(.3,1.5,footprint));}
float4 windAt(float2 p,const device float4 *flow) {
    p=(p+160)/10;int2 i=int2(floor(p));float2 f=fract(p);
    uint a=(i.y&31)*32+(i.x&31),b=(i.y&31)*32+((i.x+1)&31),c=((i.y+1)&31)*32+(i.x&31),d=((i.y+1)&31)*32+((i.x+1)&31);
    return mix(mix(flow[a],flow[b],f.x),mix(flow[c],flow[d],f.x),f.y);
}
float3 deform(float3 p,float3 local,float weight,float kind,constant Uniforms &u,const device float4 *flow) {
    if(kind!=2 && kind!=3 && kind!=4 && kind!=8)return p;
    float2 response=windAt(p.xz,flow).zw*u.options.y;
    if(kind==3)p.xz+=response*weight*.65;
    if(kind==2 || kind==4 || kind==8)p.xz+=response*pow(max(local.y,0.)/5.,2.)*.75;
    return p;
}
Varying evaluateVertex(Vertex v,Instance inst,constant Uniforms &u,const device float4 *flow) {
    float3 p=(inst.model*v.position).xyz;
    p=deform(p,v.position.xyz,v.normal.w,inst.tint.w,u,flow);
    float3x3 m=float3x3(inst.model[0].xyz,inst.model[1].xyz,inst.model[2].xyz);
    Varying o;o.position=u.viewProjection*float4(p,1);o.world=p;o.local=v.position.xyz;
    o.normal=normalize(m*v.normal.xyz);o.color=v.color.xyz*inst.tint.xyz;o.shadow=u.lightVP*float4(p,1);o.kind=inst.tint.w;o.ao=v.color.w;return o;
}
vertex Varying gardenVertex(uint id [[vertex_id]],uint instanceID [[instance_id]],const device Vertex *verts [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]]) {return evaluateVertex(verts[id],instances[instanceID],u,flow);}
struct Meshlet {uint4 ranges;float4 sphere;};
using GardenMesh=metal::mesh<Varying,void,64,124,metal::topology::triangle>;
[[mesh]] void gardenMesh(GardenMesh output,uint tid [[thread_index_in_threadgroup]],uint3 group [[threadgroup_position_in_grid]],const device Vertex *verts [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]],const device Meshlet *meshlets [[buffer(4)]],const device uint *indices [[buffer(5)]],const device uchar *triangles [[buffer(6)]]) {
    Meshlet m=meshlets[group.x];Instance inst=instances[group.y];
    float3 center=(inst.model*float4(m.sphere.xyz,1)).xyz;float scale=length(inst.model[0].xyz);
    float radius=m.sphere.w*scale+((inst.tint.w==2 || inst.tint.w==4 || inst.tint.w==8) ? 1.0:0.35);
    float4x4 rows=transpose(u.viewProjection);float4 planes[6]={rows[3]+rows[0],rows[3]-rows[0],rows[3]+rows[1],rows[3]-rows[1],rows[2],rows[3]-rows[2]};
    bool visible=true;for(uint i=0;i<6;i++)visible=visible && dot(planes[i],float4(center,1))>=-radius*length(planes[i].xyz);
    if(!visible){if(tid==0)output.set_primitive_count(0);return;}
    if(tid<m.ranges.z)output.set_vertex(tid,evaluateVertex(verts[indices[m.ranges.x+tid]],inst,u,flow));
    for(uint i=tid;i<m.ranges.w*3;i+=64)output.set_index(i,triangles[m.ranges.y+i]);
    if(tid==0)output.set_primitive_count(m.ranges.w);
}
vertex float4 shadowVertex(uint id [[vertex_id]],uint instanceID [[instance_id]],const device Vertex *verts [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]]) {
    Vertex v=verts[id];Instance inst=instances[instanceID];float3 p=(inst.model*v.position).xyz;
    return u.lightVP*float4(deform(p,v.position.xyz,v.normal.w,inst.tint.w,u,flow),1);
}
float3 display(float3 c){c=max(c,0.);return clamp((c*(2.51*c+.03))/(c*(2.43*c+.59)+.14),0.,1.);}
float3 brdf(float3 n,float3 v,float3 l,float3 base,float rough,float metal) {
    float3 h=normalize(v+l);float NoL=max(dot(n,l),0.),NoV=max(dot(n,v),.001),NoH=max(dot(n,h),0.),VoH=max(dot(v,h),0.);
    float a=rough*rough,a2=a*a,q=NoH*NoH*(a2-1)+1,D=a2/(PI*q*q);
    float vis=.5/max(NoL*sqrt(NoV*NoV*(1-a2)+a2)+NoV*sqrt(NoL*NoL*(1-a2)+a2),.001);
    float3 F0=mix(float3(.04),base,metal),F=F0+(1-F0)*pow(1-VoH,5.);
    return ((1-F)*base*(1-metal)/PI+D*vis*F)*NoL;
}
float3 bumpNormal(float3 p,float3 n,float height,float strength) {
    float3 dx=dfdx(p),dy=dfdy(p),r1=cross(dy,n),r2=cross(n,dx);float det=dot(dx,r1);
    if(abs(det)<1e-12)return n;
    float3 grad=(r1*dfdx(height)+r2*dfdy(height))/det;
    return normalize(n-grad*strength);
}
kernel void sceneLightState(texture2d<float> trans [[texture(0)]],texture2d<float> previousSky [[texture(1)]],texture2d<float> sky [[texture(2)]],texture2d<float> clearIrradiance [[texture(3)]],constant Uniforms &u [[buffer(0)]],device float4 *output [[buffer(1)]]) {
    constexpr sampler s(filter::linear,address::clamp_to_edge);
    float3 sun=u.sunExposure.xyz;
    float cloud=mix(previousSky.sample(s,reprojectSkyUV(previousSky,sun,(u.cameraTime.w-u.skyTimes.x)*u.options.y)).a,sky.sample(s,reprojectSkyUV(sky,sun,(u.cameraTime.w-u.skyTimes.y)*u.options.y)).a,u.environment.z);
    float3 direct=u.lookLight.x*3.5*sampleTrans(trans,float3(0,Rg+.002,0),sun)*cloud;
    if(u.environment.x==4)direct=u.rigColor.rgb*u.rigColor.w;
    float meter=dot(clearIrradiance.sample(s,skyUV(float3(0,1,0))).rgb,float3(.2126,.7152,.0722));
    output[0]=float4(direct,u.environment.x==4 ? 1.:clamp(.055/max(meter,.001),1.,24.));
    // Solar atmospheric attenuation is global, including when the disk is hidden.
    // Integrate once, keeping the display shader's tiny solar branch inexpensive.
    output[1]=float4(transmittance(float3(0,Rg+.002,0),sun,u.sky.z)*(u.lookLight.x*3.5/(PI*.00465*.00465)),0);
}
float4 shadeSurface(Varying in,bool front,constant Uniforms &u,constant float4 &lighting,constant float4 &material,depth2d<float> shadowMap,texture2d<float> sky,texture2d<float> irradiance,texture2d<float> trans,texture2d<float> previousSky,texture2d<float> previousIrradiance) {
    constexpr sampler shadowSampler(coord::normalized,address::clamp_to_edge,filter::linear,compare_func::less_equal);
    constexpr sampler env(filter::linear,address::clamp_to_edge);
    int kind=materialKind<0 ? int(in.kind):materialKind;
    float3 n=normalize(in.normal),color=in.color;float rough=.8,bump=0,strength=.04;
    if((kind==3 || kind==8) && !front)n=-n;
    float broad=filteredNoise(in.world.xz*.18),grain=filteredNoise(in.local.xz*18+in.local.y*3);
    if(kind==1) {
        float pathX=sin(in.world.z*.048)*9+sin(in.world.z*.105)*2;
        float path=1-smoothstep(1.5,2.7,abs(in.world.x-pathX)+(broad-.5)*.65);
        color=mix(float3(.29,.42,.16),float3(.42,.51,.23),broad);
        color=mix(color,float3(.53,.46,.30),path);color=mix(color,float3(.43,.45,.34),smoothstep(.18,.5,1-n.y));
        bump=filteredNoise(in.world.xz*14)*.0015+filteredNoise(in.world.xz*2)*.008;strength=1;rough=.95;
    } else if(kind==4) {
        float angle=atan2(in.local.z,in.local.x),ridge=sin(angle*17+filteredNoise(float2(in.local.y*.6,angle))*5);
        float footprint=max(fwidth(angle)*17,fwidth(in.local.y)*3);
        ridge*=1-smoothstep(.4,2.,footprint);
        bump=ridge*.009+filteredNoise(float2(angle*12,in.local.y*1.5))*.012;strength=1;
        color*=.7+.4*grain+.18*ridge;rough=.9;
    } else if(kind==5) {
        float stone=filteredNoise(in.local.xz*4+in.local.y),fine=filteredNoise(in.local.xy*35);
        float moss=smoothstep(.35,.8,n.y)*smoothstep(.35,.65,filteredNoise(in.world.xz*2));
        color=mix(color*(.72+.4*stone),float3(.28,.36,.13),moss*.65);
        bump=stone*.04+fine*.005;strength=1;rough=mix(.76,.98,moss);
    } else if(kind==6) {
        float angle=atan2(in.local.z,in.local.x),ribs=sin(angle*23+in.local.y*.7);
        ribs*=1-smoothstep(.4,2.,fwidth(angle)*23);
        float speck=filteredNoise(in.local.xz*45+in.local.y*9);
        color*=.78+.2*speck+.10*ribs;
        bump=(ribs*.007+grain*.003)*.01;strength=1;rough=.62+.13*grain;
    } else if(kind==2 || kind==8) {
        float leaves=filteredNoise(in.local.xy*9)+filteredNoise(in.local.xz*9);
        color*=.78+.2*leaves;bump=leaves*.012;strength=1;rough=.82;
    } else if(kind==3) {color*=.85+.15*grain;rough=.85;}
    if(material.z>.5)return float4(color*8*u.sunExposure.w,1);
    if(material.w>.5 && abs(n.y)>.9) {
        float spacing=pow(10.,floor(log10(max(u.rigParams.z,0.001))));
        float2 grid=in.world.xz/spacing,edge=abs(fract(grid-.5)-.5)/max(fwidth(grid),.001);
        float fade=1-smoothstep(.25,.7,max(fwidth(grid.x),fwidth(grid.y)));
        color*=1-.18*fade*(1-smoothstep(0.,1.,min(edge.x,edge.y)));
    }
    if(material.x>=0)rough=material.x;
    float metal=material.y;
    rough=clamp(rough+u.lookSurface.y,.045,1.);
    if(kind!=1)color=mix(in.color,color,u.lookSurface.x);
    color=artSaturation(color,u.lookSurface.z);
    n=bumpNormal(in.world,n,bump,strength*u.lookSurface.x);
    float wet=u.environment.y;rough=mix(rough,max(.22,rough*.5),wet);color*=mix(1.,.78,wet);
    float3 base=pow(max(color,0.),float3(2.2)),v=normalize(u.cameraTime.xyz-in.world),l=u.sunExposure.xyz;
    float3 sunlight=lighting.xyz;
    if(u.rigPosition.w>.5) {
        float3 delta=u.rigPosition.xyz-in.world;float d2=max(dot(delta,delta),pow(u.rigParams.z*u.rigParams.x,2.));
        l=normalize(delta);sunlight/=max(d2,1e-10);
        // Finite source broadens the highlight; penumbra filtering below is an
        // artistic approximation, not an area-light visibility integral.
        rough=sqrt(min(1.,rough*rough+u.rigParams.x*u.rigParams.z/sqrt(d2)*.15));
    }
    float3 sc=in.shadow.xyz/in.shadow.w;float2 suv=sc.xy*float2(.5,-.5)+.5;float visibility=1;
    if(all(suv>0) && all(suv<1) && sc.z>0 && sc.z<1 && max(sunlight.r,max(sunlight.g,sunlight.b))>.00001) {
        visibility=0;
        float3 dx=dfdx(sc),dy=dfdy(sc);float2 a=dx.xy*float2(.5,-.5),b=dy.xy*float2(.5,-.5);
        float determinant=a.x*b.y-a.y*b.x;
        float2 gradient=abs(determinant)>1e-12 ? float2(dx.z*b.y-dy.z*a.y,a.x*dy.z-b.x*dx.z)/determinant:float2(0);
        float bias=.00005+dot(abs(gradient),float2(1./2048.));
        // Pair the six texel weights of the bilinearly filtered 5×5 box.
        // Nine hardware comparisons preserve the original footprint and weights.
        float2 pixel=suv*2048.-.5,cell=floor(pixel),f=fract(pixel);
        float3 wx=float3(2-f.x,2,1+f.x),wy=float3(2-f.y,2,1+f.y);
        float3 ox=float3(-2+1/wx.x,.5,2+f.x/wx.z),oy=float3(-2+1/wy.x,.5,2+f.y/wy.z);
        for(uint y=0;y<3;y++)for(uint x=0;x<3;x++) {
            float2 centerUV=(cell+.5+float2(ox[x],oy[y]))/2048.;
            float2 uv=suv+(centerUV-suv)*(u.rigPosition.w>.5 ? 1+u.rigParams.x*20:1),offset=uv-suv;
            visibility+=shadowMap.sample_compare(shadowSampler,uv,sc.z+dot(gradient,offset)-bias)*(wx[x]*wy[y]/25.);
        }
    }
    float3 ambient=mix(previousIrradiance.sample(env,skyUV(n)).rgb,irradiance.sample(env,skyUV(n)).rgb,u.environment.z);
    ambient=u.environment.x==4 ? float3(u.rigParams.y):artSky(ambient,u);
    ambient*=u.lookLight.y;
    float ao=clamp(in.ao,0.,1.);float3 lit=base*(1-metal)*ambient*ao+brdf(n,v,l,base,rough,metal)*sunlight*visibility;

    if(kind==2 || kind==3 || kind==8)lit+=base*sunlight*max(dot(-n,l),0.)*.15*visibility;
    float3 F0=mix(float3(.04),base,metal),F=F0+(1-F0)*pow(1-max(dot(n,v),0.),5.);
    float3 reflected=normalize(mix(reflect(-v,n),n,rough*rough));
    float3 reflection=u.environment.x==4 ? float3(u.rigParams.y):mix(skyLight(previousSky,reflected),skyLight(sky,reflected),u.environment.z);
    if(u.environment.x!=4)reflection=artSky(reflection,u);
    lit+=reflection*F*(1-rough)*ao;
    if(u.environment.x!=4 && (u.options.z<.5 || material.w>1.5)) {
        float distance=length(in.world-u.cameraTime.xyz);
        float3 extinction=float3(.005802,.013558,.033100)+.004440+(u.environment.x==3 ? .04:0.);
        float3 T=exp(-extinction*distance*.001);
        float3 direction=normalize(in.world-u.cameraTime.xyz);
        float3 air=mix(skyLight(previousSky,direction),skyLight(sky,direction),u.environment.z);
        if(material.w>1.5) {
            // Match the sky's horizon lookup and live-cloud reprojection exactly.
            direction=normalize(float3(direction.x,.003,direction.z));
            float2 a=reprojectSkyUV(previousSky,direction,(u.cameraTime.w-u.skyTimes.x)*u.options.y);
            float2 b=reprojectSkyUV(sky,direction,(u.cameraTime.w-u.skyTimes.y)*u.options.y);
            air=mix(previousSky.sample(env,a).rgb,sky.sample(env,b).rgb,u.environment.z);
        }
        lit=lit*T+artSky(air,u)*(1-T);
    }
    return float4(lit*u.sunExposure.w,1);
}
fragment float4 gardenFragment(Varying in [[stage_in]],bool front [[front_facing]],constant Uniforms &u [[buffer(1)]],constant float4 &lighting [[buffer(2)]],constant float4 &material [[buffer(4)]],depth2d<float> shadowMap [[texture(0)]],texture2d<float> sky [[texture(1)]],texture2d<float> irradiance [[texture(2)]],texture2d<float> trans [[texture(3)]],texture2d<float> previousSky [[texture(4)]],texture2d<float> previousIrradiance [[texture(5)]]) {
    return shadeSurface(in,front,u,lighting,material,shadowMap,sky,irradiance,trans,previousSky,previousIrradiance);
}
struct SkyOut {float4 position [[position]];float2 uv;};
vertex SkyOut skyVertex(uint id [[vertex_id]]){float2 p=float2((id<<1)&2,id&2);return {float4(p*2-1,1,1),p};}
// An exact plane intersection: no mesh extent, tessellation or marching steps.
// Writing depth keeps the regular scene and MSAA edges in front of the floor.
struct GroundOut {float4 color [[color(0)]];float depth [[depth(any)]];};
fragment GroundOut groundFragment(SkyOut screen [[stage_in]],constant Uniforms &u [[buffer(1)]],constant float4 &lighting [[buffer(2)]],constant float4 &material [[buffer(4)]],depth2d<float> shadowMap [[texture(0)]],texture2d<float> sky [[texture(1)]],texture2d<float> irradiance [[texture(2)]],texture2d<float> trans [[texture(3)]],texture2d<float> previousSky [[texture(4)]],texture2d<float> previousIrradiance [[texture(5)]]) {
    float4 far=u.inverseVP*float4(screen.uv*2-1,1,1);float3 ray=normalize(far.xyz);
    if(ray.y>=0 || u.cameraTime.y<=0)discard_fragment();
    float distance=u.cameraTime.y/max(-ray.y,1e-8);
    float3 p=u.cameraTime.xyz+ray*distance;p.y=0;
    Varying surface;surface.position=u.viewProjection*float4(p,1);
    surface.world=p;surface.local=p;surface.normal=float3(0,1,0);
    surface.color=float3(.43);surface.shadow=u.lightVP*float4(p,1);surface.kind=7;surface.ao=1;
    float4 color=shadeSurface(surface,true,u,lighting,material,shadowMap,sky,irradiance,trans,previousSky,previousIrradiance);
    // Far ground remains behind scene geometry but in front of the sky at depth 1.
    return {color,clamp(surface.position.z/surface.position.w,0.,0.99999994)};
}
fragment float4 skyFragment(SkyOut in [[stage_in]],constant Uniforms &u [[buffer(1)]],constant float4 *lighting [[buffer(2)]],texture2d<float> sky [[texture(1)]],texture2d<float> previousSky [[texture(4)]],texture2d<float> clearSky [[texture(6)]]) {
    float4 far=u.inverseVP*float4(in.uv*2-1,1,1);float3 ray=normalize(far.xyz);
    if(u.environment.x==4)return float4(float3(.045,.039,.032),1);
    if(ray.y<0) {
        // The empty soundstage still has an Earth horizon. Repeating the horizon
        // texel down the screen produces a false glowing curtain below the sun.
        float3 ground=float3(.14,.16,.12)*(skyLight(sky,float3(0,1,0))+.02*max(u.sunExposure.y,0.));
        float3 horizon=skyLight(clearSky,normalize(float3(ray.x,.003,ray.z)));
        float mist=exp(ray.y*70.);
        return float4(artSky(mix(ground,horizon,mist),u)*u.sunExposure.w,1);
    }
    // Ground-intersecting atmosphere paths belong to the terrain, not a black sky seam.
    float3 lookup=normalize(float3(ray.x,max(ray.y,.003),ray.z));
    float2 previousUV=reprojectSkyUV(previousSky,lookup,(u.cameraTime.w-u.skyTimes.x)*u.options.y);
    float2 currentUV=reprojectSkyUV(sky,lookup,(u.cameraTime.w-u.skyTimes.y)*u.options.y);
    constexpr sampler smp(filter::linear,s_address::repeat,t_address::clamp_to_edge);
    float3 L=mix(previousSky.sample(smp,previousUV).rgb,sky.sample(smp,currentUV).rgb,u.environment.z);
    float disk=smoothstep(cos(.0048),cos(.0043),dot(ray,u.sunExposure.xyz));
    float cloudT=mix(previousSky.sample(smp,previousUV).a,sky.sample(smp,currentUV).a,u.environment.z);
    // Irradiance / solar solid angle. Keep the disk small but truly emissive;
    // glare is integrated in the HDR post pass instead of painting a larger disk.
    L=artSky(L,u);
    L+=disk*lighting[1].rgb*cloudT;
    return float4(min(L*u.sunExposure.w,60000.),1);
}
kernel void atmosphereIrradiance(texture2d<float> sky [[texture(0)]],texture2d<float,access::write> output [[texture(1)]],constant Uniforms &u [[buffer(0)]],uint2 id [[thread_position_in_grid]]) {
    float3 n=skyDirection((float2(id)+.5)/float2(output.get_width(),output.get_height()));
    float3 tangent=normalize(cross(abs(n.y)<.99?float3(0,1,0):float3(1,0,0),n)),bitangent=cross(n,tangent),L=0;
    float3 ground=skyLight(sky,float3(0,1,0))*float3(.18,.20,.12)+max(u.sunExposure.y,0.)*float3(.04,.045,.025);
    for(uint i=0;i<64;i++) {
        float r=sqrt((i+.5)/64.),phi=i*2.39996323;float3 d=tangent*(r*cos(phi))+bitangent*(r*sin(phi))+n*sqrt(1-r*r);
        L+=d.y>0 ? skyLight(sky,d):ground;
    }
    output.write(float4(L/64.,1),id);
}
kernel void bloomExtract(texture2d<float,access::read> hdr [[texture(0)]],texture2d<float,access::write> bloom [[texture(1)]],uint2 id [[thread_position_in_grid]]) {
    if(id.x>=bloom.get_width() || id.y>=bloom.get_height())return;
    float3 value=0;
    for(uint y=0;y<4;y++)for(uint x=0;x<4;x++) {
        float3 c=hdr.read(id*4+uint2(x,y)).rgb;float brightness=max(c.r,max(c.g,c.b));
        // Bound glare energy for a fixed SDR display without clipping HDR source.
        value+=c/max(1.,brightness/300.)*smoothstep(1.,2.,brightness)/16.;
    }
    bloom.write(float4(value,1),id);
}
kernel void bloomBlur(texture2d<half> source [[texture(0)]],texture2d<half,access::write> target [[texture(1)]],constant int2 &axis [[buffer(0)]],uint2 id [[thread_position_in_grid]]) {
    if(id.x>=target.get_width() || id.y>=target.get_height())return;
    constexpr sampler s(coord::pixel,filter::linear,address::clamp_to_edge);
    const float offsets[6]={1.4584295168,3.4039848067,5.3518057801,7.3029407160,9.2581597095,11.2179287324};
    const half weights[6]={0.2322836775,0.1353307781,0.0511574886,0.0125396480,0.0019914324,0.0002047061};
    float2 uv=float2(id)+.5;
    half3 sum=source.sample(s,uv).rgb*half(0.1329845386);
    for(uint i=0;i<6;i++) {
        float2 d=float2(axis)*offsets[i];
        sum+=(source.sample(s,uv-d).rgb+source.sample(s,uv+d).rgb)*weights[i];
    }
    target.write(half4(sum,1),id);
}
fragment float4 displayComposite(SkyOut in [[stage_in]],texture2d<float> hdr [[texture(0)]],texture2d<float> bloom [[texture(1)]],constant float4 &lighting [[buffer(0)]],constant float4 &look [[buffer(1)]]) {
    constexpr sampler s(filter::linear,address::clamp_to_edge);
    float2 uv=in.position.xy/float2(hdr.get_width(),hdr.get_height());
    float3 radiance=(hdr.sample(s,uv).rgb+bloom.sample(s,uv).rgb*look.w)*lighting.w;
    radiance*=exp(look.z*float3(.8,0,-.8));
    radiance=.18*pow(max(radiance/.18,0.),float3(look.y));
    return float4(artSaturation(display(radiance),look.x),1);
}
