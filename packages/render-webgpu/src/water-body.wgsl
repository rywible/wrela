@group(1) @binding(5) var<storage,read> waterBody:array<vec4f>;
fn waterBodyRecord(index:u32,channel:u32)->vec4f {
 let value=waterBody[index];if(channel==0u){return value;}
 let wet=floor(value.w*0.5);
 if(channel==3u){return vec4f(wet/255.0,0.0,0.0,0.0);}
 return vec4f(value.xyz,value.w-wet*2.0);
}
fn waterBodyGrid(xz:vec2f,channel:u32)->vec4f {
  let contact=channel==0u;
  let n=u32(select(waterBody[1].x,waterBody[7].x,contact));
  if(n==0u){return vec4f(waterBody[2].x,waterBody[8].xy,0.0);}
  let spacing=select(waterBody[0].zw,waterBody[7].zw,contact);
  let p=clamp((xz-waterBody[0].xy)/spacing,vec2f(0.0),vec2f(f32(n-1u)));
  let cell=min(vec2u(p),vec2u(n-2u));let f=p-vec2f(cell);
  let start=u32(select(waterBody[1].y,waterBody[7].y,contact));
  let a=start+cell.y*n+cell.x;let b=a+1u;let c=a+n;let d=c+1u;
  let va=waterBodyRecord(a,channel);let vb=waterBodyRecord(b,channel);let vc=waterBodyRecord(c,channel);let vd=waterBodyRecord(d,channel);
  if(f.x+f.y<=1.0){return va+(vb-va)*f.x+(vc-va)*f.y;}
  return vd+(vc-vd)*(1.0-f.x)+(vb-vd)*(1.0-f.y);
}
fn waterBodyWet(xz:vec2f)->f32 {
  if(waterBody[1].x<1.0){return 1000.0;}
  let p=(xz-waterBody[0].xy)/waterBody[0].zw;
  if(any(p<vec2f(0.0))||any(p>vec2f(waterBody[1].x-1.0))){return 0.0;}
  return max(0.0,waterBodyGrid(xz,1u).x-waterBodyGrid(xz,0u).x);
}
// Lowered carrier dot products; unresolved wave energy becomes slope variance.
struct WaterBodyWave { value:vec4f, lateral:vec2f, jacobian:vec3f, curvature:vec3f };
fn waterBodyWaveRange(xz:vec2f,time:f32,spacing:f32,count:u32)->WaterBodyWave {
  var value=vec4f(0.0);var lateral=vec2f(0.0);var jacobian=vec3f(1.0,0.0,1.0);var curvature=vec3f(0.0);
  for(var i=0u;i<count;i++) {
    let at=u32(waterBody[1].w)+i*2u;let phase=waterBody[at];let shape=waterBody[at+1u];
    let keep=1.0-smoothstep(0.2,0.6,spacing/shape.y);
    value.w+=0.5*shape.x*shape.x*dot(phase.xy,phase.xy)*(1.0-keep*keep);
    if(keep==0.0){continue;}
    let angle=dot(phase.xy,xz)+phase.z*time+phase.w;
    value.x+=shape.x*sin(angle)*keep;
    value.y+=shape.x*phase.x*cos(angle)*keep;
    value.z+=shape.x*phase.y*cos(angle)*keep;
    lateral+=shape.zw*shape.x*cos(angle)*keep*waterBody[2].y;
    jacobian-=shape.x*sin(angle)*keep*waterBody[2].y*vec3f(shape.z*phase.x,shape.z*phase.y,shape.w*phase.y);
    if(i<12u){curvature-=shape.x*sin(angle)*keep*vec3f(phase.x*phase.x,phase.x*phase.y,phase.y*phase.y);}
  }
  return WaterBodyWave(value,lateral,jacobian,curvature);
}
fn waterBodyWaves(xz:vec2f,time:f32,spacing:f32)->WaterBodyWave {
  return waterBodyWaveRange(xz,time,spacing,u32(waterBody[1].z));
}
fn waterBodyBaseNormal(xz:vec2f,previous:bool)->vec3f {
  if(waterBody[1].x<1.0){return vec3f(0.0,1.0,0.0);}
  let channel=select(1u,2u,previous);let step=waterBody[0].zw;
  let x=(waterBodyGrid(xz+vec2f(step.x,0.0),channel).x-waterBodyGrid(xz-vec2f(step.x,0.0),channel).x)/(2.0*step.x);
  let z=(waterBodyGrid(xz+vec2f(0.0,step.y),channel).x-waterBodyGrid(xz-vec2f(0.0,step.y),channel).x)/(2.0*step.y);
  return normalize(vec3f(-x,1.0,-z));
}
@group(1) @binding(6) var waterSpectrumMap:texture_2d_array<f32>;
@group(1) @binding(7) var waterSpectrumSampler:sampler;
// Fine carrier interference should read as a sheen rather than a field of pits.
// Preserve its energy in the roughness closure instead of removing the waves.
fn waterResolvedSlope(band:u32)->f32 {return select(select(0.25,0.7,band==1u),1.0,band==0u);}
fn waterBodyFilteredWaves(xz:vec2f,footprint:f32)->WaterBodyWave {
  if(waterBody[5].w<0.5){return waterBodyWaves(xz,g.params.x,footprint*1.5);}
  let authored=waterBodyWaveRange(xz,g.params.x,footprint*1.5,u32(waterBody[1].z)-54u);
  var value=authored.value;var jac=authored.jacobian;
  for(var band=0u;band<3u;band++) {
    let period=waterBody[5][band];let uv=xz/period;
    let size=f32(textureDimensions(waterSpectrumMap).x);
    let lod=clamp(log2(max(footprint*size/period,1.0)),0.0,log2(size));
    let choppy=waterBody[10].x>3.0;let layer=select(band,band*2u,choppy);
    let slopes=textureSampleLevel(waterSpectrumMap,waterSpectrumSampler,uv,i32(layer),lod);

    let retain=waterResolvedSlope(band);let square=retain*retain;
    let unresolved=max(0.0,slopes.z-dot(slopes.xy,slopes.xy))*square+waterBody[10][band+1u]*(1.0-square);
    value+=vec4f(slopes.w,slopes.xy*retain,unresolved);
    if(choppy){jac+=textureSampleLevel(waterSpectrumMap,waterSpectrumSampler,uv,i32(layer+1u),lod).xyz;}
  }
  return WaterBodyWave(value,vec2f(0.0),jac,vec3f(0.0));
}
// Recover the filtered surface Hessian in world metres for shallow focusing.
fn waterBodyCurvature(slope:vec2f,dx:vec3f,dy:vec3f)->vec3f {
  let sx=dpdx(slope);let sy=dpdy(slope);let det=dx.x*dy.z-dx.z*dy.x;
  let reciprocal=sign(det)/max(abs(det),1e-10);
  let x=(sx*dy.z-sy*dx.z)*reciprocal;let z=(sy*dx.x-sx*dy.x)*reciprocal;
  return vec3f(x.x,(x.y+z.x)*0.5,z.y);
}
fn waterBodyCaustic(hessian:vec3f,path:f32)->f32 {
  if(obj.waterField.x<0.5){return 1.0;}
  let strength=waterBody[3].w;
  if(strength==0.0||path>5.0){return 1.0;}
  let focus=clamp(path,0.0,2.0)*0.25;
  let jacobian=(1.0-focus*hessian.x)*(1.0-focus*hessian.z)-focus*focus*hessian.y*hessian.y;
  let amplification=clamp(1.0/max(abs(jacobian),0.25),0.5,3.0)-1.0;
  return 1.0+strength*amplification*exp(-path*0.45)*smoothstep(0.0,0.2,g.sun.y);
}
// Bounded single-scattering source, in inverse metres, independent of artistic surface tint.
fn waterBodyScattering(world:vec3f,tint:vec3f)->vec3f {
  let ambient=max(physicalDiffuseSky(vec3f(0.0,1.0,0.0)),vec3f(0.0));
  let view=normalize(g.camera.xyz-world);let anisotropy=waterBody[6].w;
  let phase=(1.0-anisotropy*anisotropy)/pow(max(0.05,1.0+anisotropy*anisotropy-2.0*anisotropy*dot(-view,g.sun.xyz)),1.5);
  let sun=g.sunlight.xyz*g.sun.w*waterSunTransmission(world)*max(g.sun.y,0.0);
  let albedo=waterBody[6].xyz/max(waterBody[3].xyz+waterBody[6].xyz,vec3f(0.0001));
  return albedo*(ambient+sun*phase/12.566371);
}
// Caustic redistribution is limited to an estimated direct receiver term.
// It never multiplies the already-lit (including indirect/shadowed) opaque color.
fn waterBodyReceiverCaustic(receiver:vec3f,behind:vec3f,hessian:vec3f,path:f32)->vec3f {
  if(waterBody[3].w<=0.0||path>5.0){return behind;}
  let direct=g.sunlight.xyz*g.sun.w*waterSunTransmission(receiver)*max(g.sun.y,0.0)*shadow(receiver,vec3f(0.0,1.0,0.0))/3.141593;
  let ambient=physicalDiffuseSky(vec3f(0.0,1.0,0.0));
  let albedo=clamp(behind/max(direct+ambient,vec3f(0.001)),vec3f(0.0),vec3f(1.0));
  return max(vec3f(0.0),behind+albedo*direct*(waterBodyCaustic(hessian,path)-1.0));
}
// A falling/overturning sheet has a short optical path. Its background already
// contains the base water, captured between the two water passes.
fn waterSheetTransport(world:vec3f,n:vec3f,view:vec3f,reflected:vec3f,fresnel:f32)->vec3f {
 let at=u32(waterBody[13].x)+u32(obj.waterField.z)*3u;
 let thickness=clamp(waterBody[at+1u].x*0.02,0.005,0.08);
 let facing=select(-n,n,dot(n,view)>=0.0);
 let refracted=refract(-view,facing,1.0/1.333);
 let q=waterProject(world+refracted*thickness/max(dot(-refracted,facing),0.1));
 var behind=waterBodyScattering(world,obj.color.xyz);
 if(q.w>0.0){behind=textureSampleLevel(waterOpaqueColor,waterTransportSampler,q.xy,0.0).rgb;}
 let transmission=exp(-(waterBody[3].xyz+waterBody[6].xyz)*thickness/max(abs(dot(n,view)),0.1));
 return reflected+(1.0-fresnel)*(behind*transmission+waterBodyScattering(world,obj.color.xyz)*(1.0-transmission));
}
// Advected foam forms connected strands along the current/wind, not round holes.
fn waterFoamBreakup(p:vec2f,footprint:f32)->f32 {
 let along=waterBody[12].xy;
 let q=vec2f(dot(p,vec2f(-along.y,along.x))*3.2,dot(p,along)*0.4);
 return filteredNoise(vec3f(q,0.0),vec3f(0.0),footprint*3.2);
}
fn waterBodyResponse(world:vec3f,parameter:vec2f,baseNormal:vec3f,dx:vec3f,dy:vec3f,base:vec3f,rough:f32)->vec3f {
  let footprint=max(length(dx),length(dy));
  let depth=waterBodyWet(world.xz);let damp=min(1.0,depth/0.5);
  let sampled=waterBodyFilteredWaves(parameter,footprint);var wave=sampled.value;
  let jac=sampled.jacobian;let determinant=max(jac.x*jac.z-jac.y*jac.y,0.1);
  let fluid=waterBodyGrid(world.xz,1u);
  if(waterBody[1].x>0.0){
    // Two offset flow phases avoid texture reset jumps; only sub-grid slopes are advected.
    let local=fluid.yz-waterBody[8].xy;let phase=fract(g.params.x/4.0);
    let p0=parameter-local*phase*4.0;let p1=parameter-local*fract(phase+0.5)*4.0;
    let tile=waterBody[5].z;let lod=clamp(log2(max(footprint*waterBody[9].w/tile,1.0)),0.0,8.0);
    if(waterBody[5].w>0.5){
      let layer=select(2,4,waterBody[10].x>3.0);
      let original=textureSampleLevel(waterSpectrumMap,waterSpectrumSampler,parameter/tile,layer,lod).xy;
      let a=textureSampleLevel(waterSpectrumMap,waterSpectrumSampler,p0/tile,layer,lod).xy;
      let b=textureSampleLevel(waterSpectrumMap,waterSpectrumSampler,p1/tile,layer,lod).xy;
      let flow=mix(a,b,abs(phase*2.0-1.0))-original;
      wave.y+=flow.x*waterResolvedSlope(2u);wave.z+=flow.y*waterResolvedSlope(2u);
    }
  }
  let slope=vec2f(wave.y*jac.z-wave.z*jac.y,wave.z*jac.x-wave.y*jac.y)/determinant;
  let curvature=waterBodyCurvature(slope,dx,dy);
  let baseSlope=-baseNormal.xz/max(baseNormal.y,0.15);
  var n=normalize(vec3f(-baseSlope.x-slope.x*damp,1.0,-baseSlope.y-slope.y*damp));
  if(obj.waterField.y>0.5){n=normalize(baseNormal+vec3f(-slope.x,0.0,-slope.y)*0.15);}
  let view=normalize(g.camera.xyz-world);
  if(dot(baseNormal,view)>=0.0){n=normalize(n+view*max(0.0,0.03-dot(n,view)));}
  // Statistical closure for unresolved bands, not an exact GGX integral.
  let azimuth=normalize(view.xz+vec2f(0.00001));let covariance=waterBody[9].xyz;
  let directional=max(0.05,(azimuth.x*azimuth.x*covariance.x+2.0*azimuth.x*azimuth.y*covariance.y+azimuth.y*azimuth.y*covariance.z)/max(covariance.x+covariance.z,0.000001));
  let effectiveRough=clamp(pow(pow(rough,4.0)+wave.w*damp*damp*directional,0.25),0.06,0.65);
  let fresnel=waterFresnel(abs(dot(n,view)));
  let reflectionRay=reflect(-view,n);
  let localLight=compiledIndirectSample(world,n,reflectionRay,vec2f(effectiveRough,-1.0),true);
  var reflected=compiledReflectionRadiance(localLight.reflection,reflectionRay,effectiveRough)*environmentSpecularWeight(max(dot(n,view),0.0),effectiveRough,vec3f(0.02037));
  reflected=waterBodyReflectedEnvironment(world,reflect(-view,n),reflected,fresnel);
  var color=reflected+(1.0-fresnel)*waterBodyScattering(world,base);
  if(obj.waterField.y>0.5){color=waterSheetTransport(world,n,view,reflected,fresnel);}
  else if(waterBody[4].w>0.5){color=waterTransportWithOptics(world,n,view,base,effectiveRough,reflected,waterBody[3].xyz+waterBody[6].xyz,true,curvature);}
  var visibility=1.0;if(waterBody[4].w>0.5){visibility=shadow(world,n);}
  let crestTransmission=smoothstep(0.02,0.55,wave.x)*(1.0-smoothstep(0.85,1.05,determinant))*pow(max(dot(-view,g.sun.xyz),0.0),3.0);
  color+=(1.0-fresnel)*waterBodyScattering(world,base)*crestTransmission*visibility;
  color+=brdfGGX(n,view,g.sun.xyz,effectiveRough,vec3f(0.02037))*g.sunlight.xyz*g.sun.w*waterSunTransmission(world)*visibility;
  for(var i=0u;i<min(u32(g.viewport.z),8u);i++) {
    if((u32(obj.localLighting.x)&(1u<<i))==0u){continue;}
    let offset=g.points[i].position.xyz-world;let distanceSquared=max(dot(offset,offset),0.01);
    let attenuation=localLightAttenuation(distanceSquared,g.points[i].color.w);if(attenuation==0.0){continue;}
    let pointVisibility=localLightVisibility(i,world,n);
    color+=brdfGGX(n,view,normalize(offset),effectiveRough,vec3f(0.02037))*g.points[i].color.xyz*g.points[i].position.w*attenuation*pointVisibility;
  }
  let crest=(1.0-smoothstep(0.65,0.9,determinant))*smoothstep(0.0,0.2,wave.x);
  var residual=fluid.w;
  if(waterBody[1].x<1.0&&waterBody[5].w>0.5&&waterBody[10].x>3.0){residual=textureSampleLevel(waterSpectrumMap,waterSpectrumSampler,parameter/waterBody[5].x,1,clamp(log2(max(footprint*waterBody[9].w/waterBody[5].x,1.0)),0.0,8.0)).w;}
  var impact=0.0;
  if(obj.waterField.y>0.5){
    let froth=filteredNoise(vec3f(world.xz*2.7+waterBody[4].xy,world.y*3.0),vec3f(0.0),footprint*2.7);
    impact=(0.25+0.55*(1.0-abs(baseNormal.y)))*smoothstep(0.22,0.7,froth);
  }
  let coverage=(residual+crest*0.25+impact)*waterBody[2].w;
  if(coverage<0.001){return color;}
  let coordinate=world.xz+waterBody[4].xy+vec2f(world.y*0.83,world.y*0.61);
  // Uniform ocean current needs one continuously advected detail evaluation.
  // Two crossfaded phases are only needed for the spatially varying river flow.
  var breakup=0.5;
  if(waterBody[1].x<1.0){
    breakup=waterFoamBreakup(coordinate-waterBody[8].xy*g.params.x,footprint);
  }else{
    let phase=fract(g.params.x/4.0);let velocity=fluid.yz;
    let a=waterFoamBreakup(coordinate-velocity*phase*4.0,footprint);
    let b=waterFoamBreakup(coordinate-velocity*fract(phase+0.5)*4.0,footprint);
    breakup=mix(a,b,abs(phase*2.0-1.0));
  }
  let foam=clamp(coverage*mix(0.35,0.75,smoothstep(0.28,0.72,breakup)),0.0,0.85);
  let foamLight=vec3f(0.82)*(physicalDiffuseSky(n)+g.sunlight.xyz*g.sun.w*waterSunTransmission(world)*max(dot(n,g.sun.xyz),0.0)*visibility/3.141593);
  return mix(color,foamLight,foam);
}

struct WaterEffectPoint {position:vec3f,normal:vec3f};
fn waterEffectPoint(vertex:vec3f,coordinate:vec3f,time:f32)->WaterEffectPoint {
 let at=u32(waterBody[13].x)+u32(obj.waterField.z)*3u;
 let front=waterBody[at];let shape=waterBody[at+1u];let kind=waterBody[at+2u].x;
 let tangent=normalize(front.zw-front.xy);let direction=vec2f(-tangent.y,tangent.x);
 let u=coordinate.x;let v=coordinate.y;let clock=fract(time/shape.z+shape.w+0.045*sin(u*11.0)+0.025*sin(u*29.0+shape.w*12.0));
 let envelope=smoothstep(0.0,0.15,clock)*(1.0-smoothstep(0.72,1.0,clock));
 let height=shape.x*select(envelope,1.0,kind>1.5)*(0.88+0.09*sin(u*17.0+time*1.3)+0.06*sin(u*41.0-time*2.1));
 var center=mix(front.xy,front.zw,u)+direction*(clock-0.4)*shape.y;
 if(kind>1.5){center=mix(front.xy,front.zw,u);}
 if(coordinate.z>0.5){
  let seed=coordinate.z;let age=fract(time/(0.5+fract(seed*0.37)*0.6)+seed*0.618);
  let lifetime=0.5+fract(seed*0.37)*0.6;let t=age*lifetime;
  let launch=2.2+fract(seed*0.73)*1.6;
  let horizontal=center+direction*(shape.y*0.45+t*(1.0+fract(seed*0.29))*height);
  let y=max(-0.1,launch*t-4.905*t*t)*select(envelope,1.0,kind>1.5);
  let scale=(0.008+fract(seed*0.47)*0.018)*sin(age*3.141593)*select(envelope,1.0,kind>1.5);
  return WaterEffectPoint(vec3f(horizontal.x,waterBody[2].x+y,horizontal.y)+vertex*scale,normalize(vertex));
 }
 // A broad attached shoulder feeds a narrower overturning lip. The sheet
 // remains single-valued at its base and folds only in the hero crest region.
 let edge=pow(max(0.0,sin(u*3.141593)),0.35);
 let h=height*edge;
 var x=0.0;var y=0.0;var dx=0.0;var dy=0.0;
 if(v<0.58){
  let s=v/0.58;let angle=s*1.570796;
  x=-shape.y*0.62*(1.0-s);y=h*sin(angle);
  dx=shape.y*0.62/0.58;dy=h*cos(angle)*1.570796/0.58;
 }else{
  let angle=(v-0.58)/0.42*(2.5+0.8*smoothstep(0.2,0.7,clock));let radius=h*0.38;
  x=radius*sin(angle);y=h-radius*(1.0-cos(angle));
  dx=radius*cos(angle);dy=-radius*sin(angle);
 }
 var horizontal=center+direction*x;
 y+=0.024*sin(u*51.0+v*19.0+time*4.0)*sin(v*3.141593)*envelope;
 var normal=normalize(vec3f(-direction.x*dy,max(0.00001,abs(dx))*sign(dx),-direction.y*dy)+vec3f(0.0,0.00001,0.0));
 if(kind>1.5){
  horizontal=center+direction*(shape.y*v+0.035*sin(u*31.0+time*8.0+v*14.0));
  y=height*(1.0-v*v);normal=normalize(vec3f(direction.x*2.0*height*v,shape.y,direction.y*2.0*height*v));
 }
 return WaterEffectPoint(vec3f(horizontal.x,waterBody[2].x+y,horizontal.y),normal);
}

// Coarse source-derived reflected surroundings fill off-screen gaps. SSR supersedes them on hits.
fn waterBodyReflectedEnvironment(world:vec3f,direction:vec3f,sky:vec3f,fresnel:f32)->vec3f {
 var nearest=80.0;var color=sky;
 for(var i=0u;i<u32(waterBody[11].y);i++){
  let at=u32(waterBody[11].x)+i*3u;let p=waterBody[at];let extent=waterBody[at+1u].xyz;let albedo=waterBody[at+2u].xyz;
  let o=(world-p.xyz)/extent;let d=direction/extent;
  let a=dot(d,d);let b=dot(o,d);let c=dot(o,o)-1.0;let discriminant=b*b-a*c;
  if(discriminant<0.0){continue;}let distance=(-b-sqrt(discriminant))/max(a,0.000001);
  if(distance<=0.04||distance>=nearest){continue;}
  nearest=distance;let position=world+direction*distance;let normal=normalize((position-p.xyz)/(extent*extent));
  let diffuse=physicalDiffuseSky(normal)+g.sunlight.xyz*g.sun.w*waterSunTransmission(position)*max(dot(normal,g.sun.xyz),0.0)/3.141593;
  let impact=length(o+d*(-b/max(a,0.000001)));
  let coverage=0.8*(1.0-smoothstep(0.65,1.0,impact));
  color=mix(sky,albedo*diffuse*fresnel,coverage*(1.0-smoothstep(55.0,80.0,distance)));
 }
 return color;
}
