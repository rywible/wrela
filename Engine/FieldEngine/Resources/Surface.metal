#include <metal_stdlib>
using namespace metal;
constant int materialKind [[function_constant(0)]];
constant bool hasSurfaceLayers [[function_constant(1)]];
constant bool hasGroomCoverage [[function_constant(2)]];
constant float PI=3.14159265359;
struct Vertex {float4 position;float4 normal;float4 color;float4 groom;};
struct SkinWeight {uint4 joints;float4 weights;};
struct CorrectiveUniform {float4 center;float4 radius;float4 offset;float4 dilation;};
Vertex skinVertex(Vertex v,uint id,const device SkinWeight *weights,constant float4x4 *bones,uint count,constant CorrectiveUniform *correctives,uint correctiveCount) {
    for(uint k=0;k<correctiveCount;++k) {
        CorrectiveUniform c=correctives[k];float3 q=v.position.xyz-c.center.xyz,d=q/c.radius.xyz;
        float a=max(0.,1.-dot(d,d));if(a<=0.)continue;
        float w=a*a*a;float3 g=-6.*a*a*q/(c.radius.xyz*c.radius.xyz),move=c.offset.xyz+q*c.dilation.xyz;
        float3x3 j=float3x3(1.);for(uint i=0;i<3;++i){j[i]+=move*g[i];j[i][i]+=w*c.dilation[i];}
        v.position.xyz+=w*move;
        v.normal.xyz=normalize(cross(j[1],j[2])*v.normal.x+cross(j[2],j[0])*v.normal.y+cross(j[0],j[1])*v.normal.z);
    }
    if(count==0)return v;
    SkinWeight w=weights[id];
    if(count & 0x80000000u) {
        // Normalized dual-quaternion blend. Choose the strongest influence's
        // hemisphere so zero-weight slots cannot determine the rotation branch.
        uint reference=0;for(uint i=1;i<4;++i)if(w.weights[i]>w.weights[reference])reference=i;
        float4 anchor=bones[w.joints[reference]][0],real=0.,dual=0.;
        for(uint i=0;i<4;++i) {
            float4 r=bones[w.joints[i]][0],d=bones[w.joints[i]][1];
            float a=w.weights[i]*(dot(r,anchor)<0. ? -1.:1.);real+=r*a;dual+=d*a;
        }
        float inverseLength=1./max(length(real),1e-8);real*=inverseLength;dual*=inverseLength;
        float3 translation=2.*(real.w*dual.xyz-dual.w*real.xyz+cross(real.xyz,dual.xyz));
        v.position.xyz+=2.*cross(real.xyz,cross(real.xyz,v.position.xyz)+real.w*v.position.xyz)+translation;
        v.normal.xyz+=2.*cross(real.xyz,cross(real.xyz,v.normal.xyz)+real.w*v.normal.xyz);
        v.normal.xyz=normalize(v.normal.xyz);return v;
    }
    float4x4 m=bones[w.joints.x]*w.weights.x+bones[w.joints.y]*w.weights.y+bones[w.joints.z]*w.weights.z+bones[w.joints.w]*w.weights.w;
    v.position=m*v.position;
    float3x3 n=float3x3(m[0].xyz,m[1].xyz,m[2].xyz);
    float3 normal=cross(n[1],n[2])*v.normal.x+cross(n[2],n[0])*v.normal.y+cross(n[0],n[1])*v.normal.z;
    v.normal.xyz=dot(normal,normal)>1e-16 ? normalize(normal):v.normal.xyz;
    return v;
}
struct Instance {float4x4 model;float4 tint;};
struct Uniforms {float4x4 viewProjection;float4x4 lightVP;float4x4 inverseVP;float4 cameraTime;float4 sunExposure;float4 options;float4 sky;float4 environment;float4 weatherBounds;float4 skyTimes;float4 rigPosition;float4 rigColor;float4 rigParams;float4 lookSurface;float4 lookLight;float4 lookGrade;};
float3 artSaturation(float3 c,float amount){float y=dot(c,float3(.2126,.7152,.0722));return max(mix(float3(y),c,amount),0.);}
float3 artSky(float3 c,constant Uniforms &u){return artSaturation(c,u.lookLight.z)*u.lookLight.x;}
// ATMOSPHERE
struct Varying {float4 position [[position]];float3 world;float3 local;float3 normal;float3 color;float4 shadow;float kind;float ao;float4 groom;};
// GROOM_COVERAGE_BEGIN
float groomRandom(uint seed) {
    uint x=seed+0x9e3779b9u;x=(x^(x>>16))*2246822519u;x=(x^(x>>13))*3266489917u;
    return float((x^(x>>16))&0xffffffu)/16777216.;
}
float groomPulseIntegral(float x,float duty) {return floor(x)*duty+clamp(fract(x)-.5+duty*.5,0.,duty);}
float groomCoverageAt(float4 q,float2 footprint) {
    if(q.z<=0.)return 1.;
    float t=clamp(q.y,0.,1.),phase=q.x+.12*sin(t*11.)*sin(t*3.14159265359);
    float span=max(.002,abs(footprint.x)),duty=q.z*(1.-.55*smoothstep(.55,1.,t));
    float localPhase=fract(phase);
    float pulse=clamp((groomPulseIntegral(localPhase+span*.5,duty)-groomPulseIntegral(localPhase-span*.5,duty))/span,0.,1.);
    float variation=max(.001,q.w),end=1.-variation*groomRandom(uint(max(0.,floor(phase))));
    float feather=max(.008,abs(footprint.y));
    float individual=1.-smoothstep(end-feather,end,t),mean=clamp((1.-t)/variation,0.,1.);
    return clamp(pulse*mix(individual,mean,smoothstep(.6,1.6,span)),0.,1.);
}
// GROOM_COVERAGE_END
float groomCoverage(float4 q) {
    float phase=q.x+.12*sin(q.y*11.)*sin(q.y*PI);
    return groomCoverageAt(q,float2(fwidth(phase),fwidth(q.y)));
}
float hash(float2 p){return fract(sin(dot(p,float2(127.1,311.7)))*43758.5453);}
float noise(float2 p){float2 i=floor(p),f=fract(p);f=f*f*(3-2*f);return mix(mix(hash(i),hash(i+float2(1,0)),f.x),mix(hash(i+float2(0,1)),hash(i+1),f.x),f.y);}
float filteredNoise(float2 p){float footprint=max(length(dfdx(p)),length(dfdy(p)));if(footprint>=1.5)return .5;return mix(noise(p),.5,smoothstep(.3,1.5,footprint));}
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
    float3x3 normalMatrix=float3x3(cross(m[1],m[2]),cross(m[2],m[0]),cross(m[0],m[1]));
    o.normal=normalize(normalMatrix*v.normal.xyz);o.color=v.color.xyz*inst.tint.xyz;o.shadow=u.lightVP*float4(p,1);o.kind=inst.tint.w;o.ao=v.color.w;o.groom=v.groom;return o;
}
vertex Varying surfaceVertex(uint id [[vertex_id]],uint instanceID [[instance_id]],const device Vertex *verts [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]],const device uint *instanceIDs [[buffer(7)]],const device SkinWeight *weights [[buffer(10)]],constant float4x4 *bones [[buffer(11)]],constant uint &skinCount [[buffer(12)]],constant CorrectiveUniform *correctives [[buffer(13)]],constant uint &correctiveCount [[buffer(14)]]) {Varying o=evaluateVertex(skinVertex(verts[id],id,weights,bones,skinCount,correctives,correctiveCount),instances[instanceIDs[instanceID]],u,flow);o.local=verts[id].position.xyz;return o;}
struct Meshlet {uint4 ranges;float4 sphere;};
using SurfaceMesh=metal::mesh<Varying,void,64,124,metal::topology::triangle>;
struct MeshPayload {uint meshlets[32];uint instance;};
[[object]] void surfaceObject(object_data MeshPayload &payload [[payload]],mesh_grid_properties grid,
    uint tid [[thread_index_in_threadgroup]],uint3 group [[threadgroup_position_in_grid]],
    constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],
    const device Meshlet *meshlets [[buffer(4)]],const device uint *instanceIDs [[buffer(7)]],
    constant float4 *planes [[buffer(8)]],constant uint &meshletCount [[buffer(9)]],constant uint &skinCount [[buffer(12)]]) {
    uint id=group.x*32+tid,instance=instanceIDs[group.y];Instance inst=instances[instance];
    bool visible=id<meshletCount;
    if(visible && skinCount==0) {
        Meshlet m=meshlets[id];float3 center=(inst.model*float4(m.sphere.xyz,1)).xyz;
        float scale=max(length(inst.model[0].xyz),max(length(inst.model[1].xyz),length(inst.model[2].xyz)));
        float top=max(0.,m.sphere.y+m.sphere.w);
        float deformation=(inst.tint.w==2 || inst.tint.w==4 || inst.tint.w==8) ? .35*sqrt(2.)*abs(u.options.y)*pow(top/5.,2.)*.75:(inst.tint.w==3 ? .35*sqrt(2.)*abs(u.options.y)*.65:0.);
        float radius=m.sphere.w*scale+deformation;
        for(uint i=0;i<6;i++)visible=visible && dot(planes[i],float4(center,1))>=-radius;
    }
    uint rank=simd_prefix_exclusive_sum(uint(visible)),count=simd_sum(uint(visible));
    if(visible)payload.meshlets[rank]=id;
    if(tid==0){payload.instance=instance;grid.set_threadgroups_per_grid(uint3(count,1,1));}
}
[[mesh]] void surfaceMesh(SurfaceMesh output,const object_data MeshPayload &payload [[payload]],uint tid [[thread_index_in_threadgroup]],uint3 group [[threadgroup_position_in_grid]],const device Vertex *verts [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]],const device Meshlet *meshlets [[buffer(4)]],const device uint *indices [[buffer(5)]],const device uchar *triangles [[buffer(6)]],const device SkinWeight *weights [[buffer(10)]],constant float4x4 *bones [[buffer(11)]],constant uint &skinCount [[buffer(12)]],constant CorrectiveUniform *correctives [[buffer(13)]],constant uint &correctiveCount [[buffer(14)]]) {
    Meshlet m=meshlets[payload.meshlets[group.x]];Instance inst=instances[payload.instance];
    if(tid<m.ranges.z){uint id=indices[m.ranges.x+tid];Varying o=evaluateVertex(skinVertex(verts[id],id,weights,bones,skinCount,correctives,correctiveCount),inst,u,flow);o.local=verts[id].position.xyz;output.set_vertex(tid,o);}
    for(uint i=tid;i<m.ranges.w*3;i+=64)output.set_index(i,triangles[m.ranges.y+i]);
    if(tid==0)output.set_primitive_count(m.ranges.w);
}
struct GrassBlade {float4 anchorAngle;float2 size;uint color;uint reserved;};
Vertex grassVertexSource(GrassBlade blade,uint id) {
    float3 p=blade.anchorAngle.xyz;float angle=blade.anchorAngle.w,h=blade.size.x,w=blade.size.y;
    float3 d=float3(cos(angle)*w,0,sin(angle)*w),n=normalize(float3(sin(angle)*.3,.9,-cos(angle)*.3));
    float3 bend=float3(cos(angle+.8),0,sin(angle+.8))*h*.32;
    float3 middle=p+float3(0,h*.55,0)+bend*.3,tip=p+float3(0,h,0)+bend;
    float3 c=float3(blade.color&1023,(blade.color>>10)&1023,(blade.color>>20)&1023)/1023.;
    Vertex v;v.position=float4(id==0 ? p-d:(id==1 ? p+d:(id==2 ? middle+d*.55:(id==3 ? middle-d*.55:tip))),1);
    v.normal=float4(n,id<2 ? 0.:(id<4 ? .4:1.));v.color=float4(c*(id<2 ? 1.:(id<4 ? 1.08:1.18)),1);v.groom=0;return v;
}
constant uint grassIndices[9]={0,1,2,0,2,3,3,2,4};
vertex Varying grassVertex(uint id [[vertex_id]],uint instanceID [[instance_id]],const device GrassBlade *blades [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]],const device uint *instanceIDs [[buffer(7)]]) {
    return evaluateVertex(grassVertexSource(blades[id/9],grassIndices[id%9]),instances[instanceIDs[instanceID]],u,flow);
}
[[mesh]] void grassMesh(SurfaceMesh output,const object_data MeshPayload &payload [[payload]],uint tid [[thread_index_in_threadgroup]],uint3 group [[threadgroup_position_in_grid]],const device GrassBlade *blades [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]],const device Meshlet *patches [[buffer(4)]],const device uint *instanceIDs [[buffer(7)]],constant float4 *planes [[buffer(8)]]) {
    Meshlet m=patches[payload.meshlets[group.x]];Instance inst=instances[payload.instance];
    if(tid<m.ranges.z*5)output.set_vertex(tid,evaluateVertex(grassVertexSource(blades[m.ranges.x+tid/5],tid%5),inst,u,flow));
    for(uint i=tid;i<m.ranges.z*9;i+=64)output.set_index(i,(i/9)*5+grassIndices[i%9]);
    if(tid==0)output.set_primitive_count(m.ranges.z*3);
}
vertex float4 shadowVertex(uint id [[vertex_id]],uint instanceID [[instance_id]],const device Vertex *verts [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]],const device uint *instanceIDs [[buffer(7)]],const device SkinWeight *weights [[buffer(10)]],constant float4x4 *bones [[buffer(11)]],constant uint &skinCount [[buffer(12)]],constant CorrectiveUniform *correctives [[buffer(13)]],constant uint &correctiveCount [[buffer(14)]]) {
    Vertex v=skinVertex(verts[id],id,weights,bones,skinCount,correctives,correctiveCount);Instance inst=instances[instanceIDs[instanceID]];float3 p=(inst.model*v.position).xyz;
    return u.lightVP*float4(deform(p,v.position.xyz,v.normal.w,inst.tint.w,u,flow),1);
}
struct GroomShadowVarying {float4 position [[position]];float4 groom;};
vertex GroomShadowVarying groomShadowVertex(uint id [[vertex_id]],uint instanceID [[instance_id]],const device Vertex *verts [[buffer(0)]],constant Uniforms &u [[buffer(1)]],const device Instance *instances [[buffer(2)]],const device float4 *flow [[buffer(3)]],const device uint *instanceIDs [[buffer(7)]],const device SkinWeight *weights [[buffer(10)]],constant float4x4 *bones [[buffer(11)]],constant uint &skinCount [[buffer(12)]],constant CorrectiveUniform *correctives [[buffer(13)]],constant uint &correctiveCount [[buffer(14)]]) {
    Vertex v=skinVertex(verts[id],id,weights,bones,skinCount,correctives,correctiveCount);Instance inst=instances[instanceIDs[instanceID]];
    float3 p=(inst.model*v.position).xyz;
    return {u.lightVP*float4(deform(p,v.position.xyz,v.normal.w,inst.tint.w,u,flow),1),v.groom};
}
fragment void groomShadowFragment(GroomShadowVarying in [[stage_in]]) {
    if(in.groom.z<=0.)return;
    float coverage=groomCoverage(in.groom);
    // The existing single-sample shadow map stores stable ordered fractional
    // coverage; its ordinary PCF reconstructs mean occlusion. No temporal noise.
    constexpr uint thresholds[16]={0,8,2,10,12,4,14,6,3,11,1,9,15,7,13,5};
    uint2 pixel=uint2(in.position.xy);
    float threshold=(float(thresholds[(pixel.y&3)*4+(pixel.x&3)])+.5)/16.;
    if(coverage<threshold)discard_fragment();
}
float3 display(float3 c){c=max(c,0.);return clamp((c*(2.51*c+.03))/(c*(2.43*c+.59)+.14),0.,1.);}
float3 brdf(float3 n,float3 v,float3 l,float3 base,float rough,float metal) {
    float3 h=normalize(v+l);float NoL=max(dot(n,l),0.),NoV=max(dot(n,v),.001),NoH=max(dot(n,h),0.),VoH=max(dot(v,h),0.);
    float a=rough*rough,a2=a*a,q=NoH*NoH*(a2-1)+1,D=a2/(PI*q*q);
    float vis=.5/max(NoL*sqrt(NoV*NoV*(1-a2)+a2)+NoV*sqrt(NoL*NoL*(1-a2)+a2),.001);
    float3 F0=mix(float3(.04),base,metal),F=F0+(1-F0)*pow(1-VoH,5.);
    return ((1-F)*base*(1-metal)/PI+D*vis*F)*NoL;
}
// Anisotropic GGX surface approximation for guide-aligned fibre detail. This
// replaces the ordinary GGX specular term; it does not add energy or claim the
// transmission/internal scattering of a Marschner hair model.
float3 groomBRDF(float3 n,float3 tangent,float3 v,float3 l,float3 base,float rough,float metal) {
    float3 bitangent=normalize(cross(n,tangent)),h=normalize(v+l);
    float NoL=max(dot(n,l),0.),NoV=max(dot(n,v),.001),NoH=max(dot(n,h),0.);
    float ax=max(.045,rough*.22),ay=max(.12,rough*.82);
    float TxH=dot(tangent,h),BxH=dot(bitangent,h);
    float q=pow(TxH/ax,2.)+pow(BxH/ay,2.)+NoH*NoH;
    float D=1./max(PI*ax*ay*q*q,1e-7);
    float lambdaV=sqrt(pow(ax*dot(tangent,v),2.)+pow(ay*dot(bitangent,v),2.)+NoV*NoV);
    float lambdaL=sqrt(pow(ax*dot(tangent,l),2.)+pow(ay*dot(bitangent,l),2.)+NoL*NoL);
    float visibility=.5/max(NoL*lambdaV+NoV*lambdaL,.001);
    float3 F0=mix(float3(.04),base,metal),F=F0+(1-F0)*pow(1-max(dot(v,h),0.),5.);
    return ((1-F)*base*(1-metal)/PI+D*visibility*F)*NoL;
}
float3 bumpNormal(float3 p,float3 n,float height,float strength) {
    float3 dx=dfdx(p),dy=dfdy(p),r1=cross(dy,n),r2=cross(n,dx);float det=dot(dx,r1);
    if(abs(det)<1e-12)return n;
    float3 grad=(r1*dfdx(height)+r2*dfdy(height))/det;
    return normalize(n-grad*strength);
}
kernel void sceneLightState(texture2d<float> trans [[texture(0)]],texture2d<float> previousSky [[texture(1)]],texture2d<float> sky [[texture(2)]],texture2d<float> clearIrradiance [[texture(3)]],constant Uniforms &u [[buffer(0)]],device float4 *output [[buffer(1)]]) {
    constexpr sampler s(filter::linear,address::clamp_to_edge);
    if(u.environment.x==4) {output[0]=float4(u.rigColor.rgb*u.rigColor.w,1);output[1]=0;return;}
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
struct SurfaceLayerUniform {float4 center;float4 shape;float4 color;float4 finish;};
void surfaceLayers(float3 p,constant SurfaceLayerUniform *layers,uint count,thread float3 &color,thread float &rough,thread float &metal,thread float &bump,thread float &strength) {
    for(uint i=0;i<count;i++) {
        SurfaceLayerUniform l=layers[i];float3 q=(p-l.center.xyz)*l.shape.xyz;
        float envelope=pow(max(0.,1-dot(q,q)),2.);
        if(envelope<=0)continue;
        float2 uv=p.xy*l.shape.w+p.z*.47+l.center.w;
        float noise=filteredNoise(uv),pattern=1.;
        if(l.finish.w==1)pattern=smoothstep(.2,.8,noise);
        else if(l.finish.w==2) {float edge=abs(noise-.5);pattern=1-smoothstep(.018,.05+fwidth(noise),edge);}
        else if(l.finish.w==3) {float phase=p.y*l.shape.w+filteredNoise(p.xz*5+l.center.w)*4;pattern=.5+.5*sin(phase);pattern=mix(pattern,.5,smoothstep(.4,2.,fwidth(phase)));}
        float coverage=envelope*l.color.w*pattern;
        color=mix(color,l.color.xyz,coverage);
        if(l.finish.x>=0)rough=mix(rough,l.finish.x,coverage);
        if(l.finish.y>=0)metal=mix(metal,l.finish.y,coverage);
        // Preserve the existing recipe's height contribution when adding fields.
        bump=bump*strength+coverage*l.finish.z;strength=1;
    }
}
// PROJECT_SURFACES
float4 shadeSurface(Varying in,bool front,constant Uniforms &u,constant float4 &lighting,constant float4 &material,constant SurfaceLayerUniform *layers,uint layerCount,depth2d<float> shadowMap,texture2d<float> sky,texture2d<float> irradiance,texture2d<float> trans,texture2d<float> previousSky,texture2d<float> previousIrradiance) {
    constexpr sampler shadowSampler(coord::normalized,address::clamp_to_edge,filter::linear,compare_func::less_equal);
    constexpr sampler env(filter::linear,address::clamp_to_edge);
    // Negative material.w is a study-only neutral surface override. The vertex
    // and shadow stages still use the original kind, geometry and pose data.
    bool clay=material.w<-.5;
    int kind=clay ? 7:(materialKind<0 ? int(in.kind):materialKind);
    float3 n=normalize(in.normal),color=in.color;float rough=.8,bump=0,strength=.04;
    if(!front)n=-n;
    float broad=filteredNoise(in.world.xz*.18),grain=filteredNoise(in.local.xz*18+in.local.y*3);
    // -2 retains diagnostic vertex colours for native sculpt influence review.
    if(clay)color=material.w < -1.5 ? in.color:float3(.57);
    else projectSurface(kind,in.local,in.world,n,color,rough,bump,strength);
    if(material.z>.5)return float4(color*8*u.sunExposure.w,1);
    if(material.w>.5 && abs(n.y)>.9) {
        float spacing=pow(10.,floor(log10(max(u.rigParams.z,0.001))));
        float2 grid=in.world.xz/spacing,edge=abs(fract(grid-.5)-.5)/max(fwidth(grid),.001);
        float fade=1-smoothstep(.25,.7,max(fwidth(grid.x),fwidth(grid.y)));
        color*=1-.18*fade*(1-smoothstep(0.,1.,min(edge.x,edge.y)));
    }
    if(material.x>=0)rough=material.x;
    float metal=material.y;
    if(hasSurfaceLayers && !clay)surfaceLayers(in.local,layers,layerCount,color,rough,metal,bump,strength);
    rough=clamp(rough+u.lookSurface.y,.045,1.);
    if(kind!=1 && !clay)color=mix(in.color,color,u.lookSurface.x);
    color=artSaturation(color,u.lookSurface.z);
    n=bumpNormal(in.world,n,bump,strength*u.lookSurface.x);
    float wet=u.environment.y;rough=mix(rough,max(.22,rough*.5),wet);color*=mix(1.,.78,wet);
    float3 base=pow(max(color,0.),float3(2.2)),v=normalize(u.cameraTime.xyz-in.world),l=u.sunExposure.xyz;
    float3 sunlight=lighting.xyz;
    if(u.rigPosition.w>.5) {
        float3 delta=u.rigPosition.xyz-in.world;float d2=max(dot(delta,delta),pow(u.rigParams.z*u.rigParams.x,2.));
        l=normalize(delta);sunlight/=max(d2,1e-10);
        if(u.rigPosition.w>1.5) {
            float cone=dot(-l,-u.sunExposure.xyz);
            sunlight*=smoothstep(u.rigParams.w,.93,cone);
        }
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
        float spread=u.rigPosition.w>.5 ? 1+u.rigParams.x*20:1;
        if(spread>1.0001) {
            // Pairing adjacent hardware texels cannot be stretched: doing so
            // breaks the filter weights at every texel boundary. A wide finite
            // source uses fixed, continuous receiver-relative sample locations.
            for(int y=-2;y<=2;y++)for(int x=-2;x<=2;x++) {
                float2 offset=float2(x,y)*spread/2048.;
                visibility+=shadowMap.sample_compare(shadowSampler,suv+offset,sc.z+dot(gradient,offset)-bias)/25.;
            }
        } else {
            // Exact adjacent-texel pairing for the compact bilinear 5×5 box.
            float2 pixel=suv*2048.-.5,cell=floor(pixel),f=fract(pixel);
            float3 wx=float3(2-f.x,2,1+f.x),wy=float3(2-f.y,2,1+f.y);
            float3 ox=float3(-2+1/wx.x,.5,2+f.x/wx.z),oy=float3(-2+1/wy.x,.5,2+f.y/wy.z);
            for(uint y=0;y<3;y++)for(uint x=0;x<3;x++) {
                float2 uv=(cell+.5+float2(ox[x],oy[y]))/2048.,offset=uv-suv;
                visibility+=shadowMap.sample_compare(shadowSampler,uv,sc.z+dot(gradient,offset)-bias)*(wx[x]*wy[y]/25.);
            }
        }
    }
    float3 ambient=mix(previousIrradiance.sample(env,skyUV(n)).rgb,irradiance.sample(env,skyUV(n)).rgb,u.environment.z);
    ambient=u.environment.x==4 ? float3(u.rigParams.y):artSky(ambient,u);
    ambient*=u.lookLight.y;
    float3 directBRDF=brdf(n,v,l,base,rough,metal);
    if(kind==13) {
        float2 across=float2(dfdx(in.groom.x),dfdy(in.groom.x));
        float3 tangent=dfdy(in.world)*across.x-dfdx(in.world)*across.y;
        tangent-=n*dot(tangent,n);
        if(dot(tangent,tangent)>1e-14)directBRDF=groomBRDF(n,normalize(tangent),v,l,base,rough,metal);
    }
    float ao=clamp(in.ao,0.,1.);float3 lit=base*(1-metal)*ambient*ao+directBRDF*sunlight*visibility;

    if(kind==9)lit+=base*ambient*.12*pow(1-max(dot(n,v),0.),3.)*ao*(1-wet);
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
fragment float4 surfaceFragment(Varying in [[stage_in]],bool front [[front_facing]],constant SurfaceLayerUniform *layers [[buffer(5)]],constant uint &layerCount [[buffer(6)]],constant Uniforms &u [[buffer(1)]],constant float4 &lighting [[buffer(2)]],constant float4 &material [[buffer(4)]],depth2d<float> shadowMap [[texture(0)]],texture2d<float> sky [[texture(1)]],texture2d<float> irradiance [[texture(2)]],texture2d<float> trans [[texture(3)]],texture2d<float> previousSky [[texture(4)]],texture2d<float> previousIrradiance [[texture(5)]]) {
    float coverage=hasGroomCoverage ? groomCoverage(in.groom):1.;
    if(hasGroomCoverage && coverage<=.001)discard_fragment();
    float4 color=shadeSurface(in,front,u,lighting,material,layers,layerCount,shadowMap,sky,irradiance,trans,previousSky,previousIrradiance);
    color.a=coverage;return color;
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
    surface.color=float3(.43);surface.shadow=u.lightVP*float4(p,1);surface.kind=7;surface.ao=1;surface.groom=0;
    float4 color=shadeSurface(surface,true,u,lighting,material,nullptr,0,shadowMap,sky,irradiance,trans,previousSky,previousIrradiance);
    // Far ground remains behind scene geometry but in front of the sky at depth 1.
    return {color,clamp(surface.position.z/surface.position.w,0.,0.99999994)};
}
fragment float4 skyFragment(SkyOut in [[stage_in]],constant Uniforms &u [[buffer(1)]],constant float4 *lighting [[buffer(2)]],texture2d<float> sky [[texture(1)]],texture2d<float> previousSky [[texture(4)]],texture2d<float> clearSky [[texture(6)]]) {
    float4 far=u.inverseVP*float4(in.uv*2-1,1,1);float3 ray=normalize(far.xyz);
    if(u.environment.x==4)return float4(u.rigPosition.w>1.5 ? float3(0):float3(.045,.039,.032),1);
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
    // Fixed world-space quadrature: adjacent normals see the same sky samples.
    // Rotating a sparse hemisphere with each normal imprinted sampling noise
    // and a tangent-frame pole discontinuity onto otherwise smooth surfaces.
    float3 L=0;
    float3 ground=skyLight(sky,float3(0,1,0))*float3(.18,.20,.12)+max(u.sunExposure.y,0.)*float3(.04,.045,.025);
    for(uint i=0;i<128;i++) {
        float y=(i+.5)/128.,r=sqrt(max(0.,1-y*y)),phi=i*2.39996323;
        float3 d=float3(r*cos(phi),y,r*sin(phi));
        float cosine=dot(n,d);
        if(cosine>0)L+=skyLight(sky,d)*cosine;
        L+=ground*max(-cosine,0.);
    }
    // Full-sphere solid angle 4π / N; diffuse output stores irradiance / π.
    output.write(float4(L*(4./256.),1),id);
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
