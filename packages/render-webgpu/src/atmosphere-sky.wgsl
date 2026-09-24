// Spherical Rayleigh + aerosol + ground mist + ozone atmosphere. Density-to-space is
// compiled; bounded LUT passes integrate direct and isotropic diffuse transport.
// The diffuse closure retains sixteen orders; terrain volumetric shadows remain omitted.
struct PhysicalCloudFormation { origin:vec4f, size:vec4f, detail:vec4f, growth:vec4f };
struct PhysicalCloudGrowth { center:vec4f, radius:vec4f };
struct PhysicalAtmosphereFrame {
  camera:vec4f, sun:vec4f, sunlight:vec4f, planetCenter:vec4f,
  right:vec4f, up:vec4f, forward:vec4f, viewport:vec4f, ground:vec4f, cloud:vec4f,
  moon:vec4f, skyCycle:vec4f, cloudShape:vec4f, frontOrigin:vec4f, frontDirection:vec4f,
};
// Keep the small per-ray frame separate from indexed authoring data: copying an
// array-bearing frame through every density helper spills registers on Metal.
struct PhysicalAtmosphereUniform {
  frame:PhysicalAtmosphereFrame, cloudForms:vec4f, cloudLayers:vec4f, formations:array<PhysicalCloudFormation,8>,
  cloudGrowth:array<PhysicalCloudGrowth,48>,
};
struct PhysicalAtmosphereTransport { radiance:vec3f, transmission:vec3f };
struct PhysicalAtmosphereOpticalDepth { density:vec4f, visible:f32 };
var<private> physicalShadowedAir:bool=false;
const ATMOSPHERE_PI:f32=3.141592653589793;
const ATMOSPHERE_AERIAL_DISTANCE:f32=20000.0;
fn physicalPlanetPoint(world:vec3f)->vec3f {
  let f=physicalFrame(); let p=(world-f.planetCenter.xyz)*0.001;
  let radius=compiledAtmosphereTable[0].x; let r=length(p);
  // Authored terrain may penetrate the ideal sea-level planet. Its atmosphere
  // begins at ground density; geometry remains owned by the actual depth pass.
  return p*(max(radius+0.001,r)/max(r,0.001));
}
fn physicalOzoneAbsorption()->vec3f {
  if(compiledAtmosphereTable[3].z<3.0) {return vec3f(0.0);}
  let tail=4u+u32(compiledAtmosphereTable[3].x)*u32(compiledAtmosphereTable[3].y);
  return compiledAtmosphereTable[tail].xyz;
}
fn physicalOzoneDensity(height:f32)->f32 {
  if(compiledAtmosphereTable[3].z<3.0) {return 0.0;}
  let tail=4u+u32(compiledAtmosphereTable[3].x)*u32(compiledAtmosphereTable[3].y);
  let layer=compiledAtmosphereTable[tail+1u].xyz;
  return max(0.0,min((height-layer.x)/(layer.y-layer.x),(layer.z-height)/(layer.z-layer.y)));
}
fn physicalDensity(height:f32)->vec4f {
  let c=compiledAtmosphereTable[0];
  return vec4f(exp(-max(0.0,height)/vec3f(c.z,c.w,compiledAtmosphereTable[3].w)),physicalOzoneDensity(height));
}
fn physicalExtinction(density:vec4f)->vec3f {
  return compiledAtmosphereTable[1].xyz*density.x+vec3f(compiledAtmosphereTable[2].x*density.y+compiledAtmosphereTable[2].y*density.z)+physicalOzoneAbsorption()*density.w;
}
fn physicalSunOpticalDepth(point:vec3f,direction:vec3f)->PhysicalAtmosphereOpticalDepth {
  let c=compiledAtmosphereTable[0]; let r=length(point); let h=clamp(r-c.x,0.0,c.y);
  let cosine=clamp(dot(point/r,direction),-1.0,1.0);
  let horizon=-sqrt(max(0.0,h*(2.0*c.x+h)))/(c.x+h);
  if(cosine<horizon) { return PhysicalAtmosphereOpticalDepth(vec4f(0.0),0.0); }
  let dims=vec2u(compiledAtmosphereTable[3].yx);
  let uv=vec2f(sqrt(clamp((cosine-horizon)/(1.0-horizon),0.0,1.0)),sqrt(sqrt(h/c.y)));
  let p=uv*vec2f(dims-vec2u(1u)); let base=min(vec2u(p),dims-vec2u(2u)); let f=p-vec2f(base);
  let a=compiledAtmosphereTable[4u+base.y*dims.x+base.x];
  let b=compiledAtmosphereTable[5u+base.y*dims.x+base.x];
  let d=compiledAtmosphereTable[4u+(base.y+1u)*dims.x+base.x];
  let e=compiledAtmosphereTable[5u+(base.y+1u)*dims.x+base.x];
  return PhysicalAtmosphereOpticalDepth(mix(mix(a,b,f.x),mix(d,e,f.x),f.y),1.0);
}
fn physicalSunTransmissionAt(point:vec3f,direction:vec3f)->vec3f {
  let depth=physicalSunOpticalDepth(point,direction);
  return exp(-physicalExtinction(depth.density))*depth.visible;
}
fn physicalSunTransmittance(world:vec3f)->vec3f {
  let sun=normalize(physicalFrame().sun.xyz);
  return physicalSunTransmissionAt(physicalPlanetPoint(world),sun)*(1.0-physicalCloudShadow(world,sun)*0.8);
}
fn physicalRayleighPhase(cosine:f32)->f32 { return 3.0*(1.0+cosine*cosine)/(16.0*ATMOSPHERE_PI); }
fn physicalMiePhase(cosine:f32)->f32 {
  let anisotropy=compiledAtmosphereTable[2].z;
  return (1.0-anisotropy*anisotropy)/(4.0*ATMOSPHERE_PI*pow(max(0.002,1.0+anisotropy*anisotropy-2.0*anisotropy*cosine),1.5));
}
// Exponential segment integration has its continuous vacuum limit.
fn physicalSegmentWeight(extinction:vec3f,length:f32)->vec3f {
  let optical=extinction*length;
  let approximate=vec3f(length)*(vec3f(1.0)-optical*0.5+optical*optical/6.0);
  return select((vec3f(1.0)-exp(-optical))/max(extinction,vec3f(1e-20)),approximate,optical<vec3f(0.001));
}
fn physicalAtmospherePath(point:vec3f,direction:vec3f)->vec2f {
  let c=compiledAtmosphereTable[0]; let b=dot(point,direction); let radius=length(point);
  let top=c.x+c.y; let delta=(top-radius)*(top+radius);
  var far=sqrt(max(0.0,b*b+delta))-b;
  if(b>0.0) { far=max(0.0,delta)/max(0.000001,sqrt(max(0.0,b*b+delta))+b); }
  var ground=0.0;
  let under=b*b-(radius-c.x)*(radius+c.x);
  if(b<0.0 && under>0.0) {
    let entry=(radius-c.x)*(radius+c.x)/max(0.000001,-b+sqrt(under));
    if(entry<far) { far=max(0.0,entry); ground=1.0; }
  }
  return vec2f(max(0.0,far),ground);
}
fn physicalIntegrateAtmosphere(world:vec3f,direction:vec3f,maximumDistance:f32,steps:u32)->PhysicalAtmosphereTransport {
  let frame=physicalFrame(); let origin=physicalPlanetPoint(world); let path=physicalAtmospherePath(origin,direction);
  let distance=min(maximumDistance,path.x); let sun=normalize(frame.sun.xyz); let cosine=dot(direction,sun);
  let rayleighPhase=physicalRayleighPhase(cosine); let miePhase=physicalMiePhase(cosine);
  let moon=normalize(frame.moon.xyz); let moonCosine=dot(direction,moon);
  let moonRayleigh=physicalRayleighPhase(moonCosine); let moonMie=physicalMiePhase(moonCosine);
  // Resolve shallow mist before spending nodes on the long upper-atmosphere
  // column. A power-of-distance grid skipped the entire 12m layer at zenith.
  let nearScale=compiledAtmosphereTable[3].w/max(0.05,abs(dot(normalize(origin),direction)));
  let logDistance=log(1.0+distance/nearScale);
  var result=PhysicalAtmosphereTransport(vec3f(0.0),vec3f(1.0));
  for(var i=0u;i<steps;i++) {
    let a=f32(i)/f32(steps); let b=f32(i+1u)/f32(steps);
    let start=nearScale*(exp(logDistance*a)-1.0); let end=nearScale*(exp(logDistance*b)-1.0); let midpoint=(start+end)*0.5;
    let point=origin+direction*midpoint;
    let density=physicalDensity(length(point)-compiledAtmosphereTable[0].x);
    let extinction=physicalExtinction(density);
    let scatter=compiledAtmosphereTable[1].xyz*density.x*rayleighPhase+
      vec3f(compiledAtmosphereTable[1].w*density.y+compiledAtmosphereTable[2].y*0.95*density.z)*miePhase;
    let scattering=compiledAtmosphereTable[1].xyz*density.x+vec3f(compiledAtmosphereTable[1].w*density.y+compiledAtmosphereTable[2].y*0.95*density.z);
    var source=vec3f(0.0);
    if(sun.y>-0.3) {
      var sunlight=frame.sunlight.xyz*frame.sun.w*physicalSunTransmissionAt(point,sun);
      if(physicalShadowedAir) {sunlight*=physicalCloudAirVisibility(world+direction*midpoint*1000.0,sun);}
      let diffuse=physicalDiffuseRadiance(point,sun)*frame.sunlight.xyz*frame.sun.w;
      source+=sunlight*scatter+diffuse*scattering;
    }
    if(frame.moon.w>0.0001) {
      let moonScatter=compiledAtmosphereTable[1].xyz*density.x*moonRayleigh+
        vec3f(compiledAtmosphereTable[1].w*density.y+compiledAtmosphereTable[2].y*0.95*density.z)*moonMie;
      let moonColor=vec3f(0.72,0.82,1.0)*frame.moon.w;
      source+=moonColor*(physicalSunTransmissionAt(point,moon)*moonScatter+
        physicalDiffuseRadiance(point,moon)*scattering);
    }
    result.radiance+=result.transmission*source*physicalSegmentWeight(extinction,end-start);
    result.transmission*=exp(-extinction*(end-start));
  }
  return result;
}
fn physicalEvaluateSky(ray:vec3f)->vec3f {
  let frame=physicalFrame(); let origin=physicalPlanetPoint(frame.camera.xyz);
  let path=physicalAtmospherePath(origin,ray);
  let result=physicalIntegrateAtmosphere(frame.camera.xyz,ray,path.x,32u);
  var radiance=result.radiance;
  if(path.y>0.5) {
    let ground=origin+ray*path.x; let n=normalize(ground); let sun=normalize(frame.sun.xyz);
    let direct=physicalSunTransmissionAt(ground,sun)*max(0.0,dot(n,sun))/ATMOSPHERE_PI;
    // Isotropic diffuse irradiance is pi*L; a Lambertian surface returns albedo*L.
    radiance+=result.transmission*frame.ground.xyz*frame.sunlight.xyz*frame.sun.w*(direct+physicalDiffuseRadiance(ground,sun));
    if(frame.moon.w>0.0001) {
      let moon=normalize(frame.moon.xyz);
      let moonDirect=physicalSunTransmissionAt(ground,moon)*max(0.0,dot(n,moon))/ATMOSPHERE_PI;
      radiance+=result.transmission*frame.ground.xyz*vec3f(0.72,0.82,1.0)*frame.moon.w*
        (moonDirect+physicalDiffuseRadiance(ground,moon));
    }
  }
  return max(vec3f(0.0),radiance);
}
