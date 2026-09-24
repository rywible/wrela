// Procedural local weather volume. This remains a bounded artistic density model,
// not a fluid simulation or a converged cloud multiple-scattering solution.
const PHYSICAL_CLOUD_BASE:f32=850.0;
const PHYSICAL_CLOUD_BASE_VARIATION:f32=1100.0;
const PHYSICAL_CLOUD_TOP:f32=3300.0;
const PHYSICAL_CLOUD_EXTINCTION:f32=0.008;
fn physicalCloudTop()->f32 {
  return max(PHYSICAL_CLOUD_TOP+3200.0*physicalFrame().cloudShape.y,physicalCloudForms().z);
}
// Explicit envelopes and the weather field share this conservative vertical bound.
fn physicalCloudSupportTop()->f32 { return physicalCloudTop(); }
override PHYSICAL_CLOUD_COMPILED_NOISE:bool=true;
override PHYSICAL_CLOUD_VIEW_STEPS:u32=32u;
override PHYSICAL_CLOUD_FORMATION_STEPS:u32=0u;
override PHYSICAL_CLOUD_JITTER:bool=false;
// Small fixed spatial offsets suppress coherent bands at the balanced budget.
// High quality and dense references retain the undisturbed Gauss rule.
override PHYSICAL_CLOUD_PHASE_SPREAD:f32=0.0;
override PHYSICAL_CLOUD_ADAPTIVE:bool=true;
override PHYSICAL_CLOUD_TARGET_STEP:f32=35.0;
@group(0) @binding(17) var physicalCloudNoiseTexture:texture_3d<f32>;
@group(0) @binding(19) var physicalCloudLightTexture:texture_3d<f32>;
@group(0) @binding(21) var physicalCloudAmbientTexture:texture_3d<f32>;
override PHYSICAL_CLOUD_CACHED_LIGHTING:bool=true;
fn physicalCloudNoiseChannels(position:vec3f)->vec2f {
  if(!PHYSICAL_CLOUD_COMPILED_NOISE) {
    return vec2f(physicalCloudNoiseReference3(position),physicalCloudWorleyReference3(position));
  }
  // The 64-cell period has a one-voxel border on each face. This makes the
  // existing clamp sampler periodic in all three axes without another binding.
  let uv=(fract(position/8.0)*64.0+1.0)/66.0;
  return textureSampleLevel(physicalCloudNoiseTexture,physicalAtmosphereSampler,uv,0.0).xy;
}
fn physicalCloudNoise3(position:vec3f)->f32 {return physicalCloudNoiseChannels(position).x;}
fn physicalCloudRotate(p:vec3f)->vec3f {
  return vec3f(0.8*p.x+0.6*p.z,-0.36*p.x+0.8*p.y+0.48*p.z,-0.48*p.x-0.6*p.y+0.64*p.z);
}
// Planet center is rebased with the scene, so density, shadows and both sky
// products sample the same advected field after a render-origin change.
fn physicalCloudPosition(world:vec3f)->vec3f {
  // Cancel the planetary terms before introducing the local position. Adding
  // the radius to each sample first rounded kilometre-scale cloud detail to
  // half-metre height steps in f32, including the surface-shadow query.
  let localOrigin=physicalFrame().planetCenter.xyz+vec3f(0.0,compiledAtmosphereTable[0].x*1000.0,0.0);
  let p=world-localOrigin;
  // Rationalized spherical altitude avoids subtracting two planet-sized f32s.
  let radius=compiledAtmosphereTable[0].x*1000.0;
  let radial=radius+p.y;
  let curvature=dot(p.xz,p.xz)/(sqrt(radial*radial+dot(p.xz,p.xz))+radial);
  return vec3f(p.x,p.y+curvature,p.z);
}
// Stable ray/spherical-shell intersection in metres. The first interval is
// valid below, within, or above the cloud layer, including grazing horizons.
fn physicalCloudSphereRoots(world:vec3f,ray:vec3f,height:f32)->vec2f {
  let radius=compiledAtmosphereTable[0].x*1000.0;
  let localOrigin=physicalFrame().planetCenter.xyz+vec3f(0.0,radius,0.0);
  let p=world-localOrigin;
  let b=dot(p,ray)+radius*ray.y;
  let c=dot(p.xz,p.xz)+(p.y-height)*(2.0*radius+p.y+height);
  let discriminant=b*b-c;
  if(discriminant<0.0) {return vec2f(-1.0);}
  let root=sqrt(discriminant);
  let q=-b-select(-root,root,b>=0.0);
  if(abs(q)<0.0001) {return vec2f(-b);}
  return vec2f(min(q,c/q),max(q,c/q));
}
fn physicalCloudLayerRange(world:vec3f,ray:vec3f,base:f32,top:f32)->vec2f {
  let outer=physicalCloudSphereRoots(world,ray,top);
  let inner=physicalCloudSphereRoots(world,ray,base);
  let height=physicalCloudPosition(world).y;
  var entry=max(0.0,outer.x);var exit=outer.y;
  if(height<base) {entry=max(entry,inner.y);}
  else if(inner.x>entry) {exit=min(exit,inner.x);}
  return vec2f(entry,max(entry,exit));
}
fn physicalCloudFilteredNoise(position:vec3f,footprint:f32)->f32 {
  // Frequency removal is a quadrature heuristic, not a coverage/error bound.
  // Unresolved octaves converge toward their mean instead of folding into bands.
  if(footprint>=0.9) {return 0.5;}
  if(footprint<=0.3) {return physicalCloudNoise3(position);}
  return mix(physicalCloudNoise3(position),0.5,smoothstep(0.3,0.9,footprint));
}
fn physicalCloudFilteredShape(position:vec3f,footprint:f32,cellularWeight:f32)->f32 {
  if(footprint>=0.9) {return 0.5;}
  let channels=physicalCloudNoiseChannels(position);
  let shape=mix(channels.x,channels.y,cellularWeight);
  return mix(shape,0.5,smoothstep(0.3,0.9,footprint));
}
fn physicalCloudWeatherSample(world:vec3f,footprint:f32,light:vec3f,shade:bool)->vec4f {
  let frame=physicalFrame();let cover=clamp(frame.cloud.x,0.0,1.0);
  let stable=physicalCloudPosition(world);
  let span=PHYSICAL_CLOUD_TOP+3200.0*frame.cloudShape.y-PHYSICAL_CLOUD_BASE;
  let layerHeight=(stable.y-PHYSICAL_CLOUD_BASE)/span;
  if(cover<=0.001||layerHeight<=0.0||layerHeight>=1.0) {return vec4f(0.0,0.5,0.0,0.0);}
  let drift=vec3f(frame.cloud.y,0.0,frame.cloud.z)*frame.cloud.w;
  let p=compiledCloudCoordinates(stable,drift)+vec3f(8.2,1.9,3.7);
  // Weather varies over kilometres. Mesoscale organization controls both the
  // gaps between banks and local convection, rather than scattering identical
  // cotton balls uniformly over the whole sky.
  let region=physicalCloudNoise3(vec3f(p.x*0.23,3.17,p.z*0.23));
  let coverMargin=physicalCloudForms().y-(1.0-region);
  if(coverMargin<=0.0) {
    // Quintic value noise has axis slope <= 1.875. The 8-bit, eight-samples
    // per-cell cache adds <= 8/255. In X/Z world space the norm is bounded by
    // sqrt(2)*(1.875+8/255)*0.23*0.00062 < 0.0004 per metre.
    // This clearance certifies a whole empty weather interval, not a heuristic
    // SDF step. Authored formation support is intersected with it below.
    return vec4f(0.0,0.5,-coverMargin/0.0004,0.0);
  }
  let presence=smoothstep(0.0,0.10,coverMargin);
  let formationField=physicalCloudNoiseChannels(vec3f(p.x*0.61+9.3,7.13,p.z*0.61-4.8));
  let formation=formationField.x;
  let updraft=smoothstep(0.30,0.85,formationField.y);
  // Whole weather groups occupy different elevations, from low banks to
  // higher detached formations. A smooth independent weather field ensures
  // cloud bases do not all read as one horizontal shelf.
  let elevation=smoothstep(0.15,0.85,physicalCloudNoise3(vec3f(p.x*0.21+2.37,1.63,p.z*0.21-5.74)));
  let baseOffset=PHYSICAL_CLOUD_BASE_VARIATION*elevation/span;
  let layerThickness=1.0-baseOffset;
  let height=(layerHeight-baseOffset)/layerThickness;
  if(height<=0.0) {return vec4f(0.0,0.5,0.0,0.0);}
  let direction=frame.frontDirection.xy/max(0.0001,length(frame.frontDirection.xy));
  let frontDistance=dot((stable.xz-drift.xz)-frame.frontOrigin.xy,direction);
  let front=frame.cloudShape.w*(smoothstep(-frame.frontOrigin.z,frame.frontOrigin.z,frontDistance)-0.5);
  // Background changes occupancy of coherent weather groups. It must not
  // leave residual banks at zero or turn every distant cloud into thin dust.
  let localCover=clamp(cover+(region-0.5)*1.45+(formation-0.5)*0.35+(formationField.y-0.5)*0.45+front,0.0,1.0);
  if(localCover<=0.005) {return vec4f(0.0,0.5,0.0,0.0);}
  // Local maturity spans shallow remnants, growing cumulus and taller towers.
  // Expand the weather range before clamping so developed weather does not
  // collapse almost every formation onto the same ceiling.
  let maturity=smoothstep(0.20,0.80,formation);
  let development=clamp(frame.cloudShape.x*(0.20+maturity*0.55+updraft*updraft*0.80),0.0,1.0);
  let top=mix(0.22,0.96,development);
  if(height>=top) {
    // Regional ceiling Lipschitz bound from the compiled weather channels;
    // spherical altitude is 1-Lipschitz just like the former planar height.
    let ceilingSlope=0.0053*clamp(frame.cloudShape.x,0.0,1.0)+1.7/span;
    return vec4f(0.0,0.5,(height-top)*layerThickness/(ceilingSlope+1.0/span),0.0);
  }
  let stratiform=(1.0-development)*smoothstep(0.45,0.9,localCover);
  let shear=height*height*(0.35+frame.cloudShape.y*1.4);
  // Translate a coherent body with its weather group; avoid stretching its
  // noise coordinates into hanging curtains as the local ceiling changes.
  let bodyPosition=p-vec3f(0.0,baseOffset*span*COMPILED_CLOUD_WORLD_SCALE.y,0.0);
  let warped=physicalCloudRotate((bodyPosition+vec3f((region-0.5)*1.7+shear,0.0,(formation-0.5)*1.3-shear*0.35)));
  let broad=physicalCloudFilteredNoise(warped*0.73,footprint*0.00062*0.73);
  let threshold=0.62-localCover*0.27+(1.0-presence)*0.50;
  // Every remaining term is bounded; don't evaluate fine noise in proven air.
  if(broad*0.55+0.45<=threshold) {return vec4f(0.0,0.5,0.0,0.0);}
  let lobeScale=2.1;
  let billows=physicalCloudFilteredShape(physicalCloudRotate(warped)*lobeScale+vec3f(11.4,7.3,2.8),footprint*0.00062*lobeScale,0.8);
  let body=broad*0.55+billows*0.45;
  // Shallow banks taper early; convective bodies retain vertical shoulders.
  let capStart=top*mix(0.30,0.52,development);
  let cap=smoothstep(capStart,top,height)*0.68;
  let base=smoothstep(0.0,0.09+(1.0-broad)*0.12,height);
  let shape=body-threshold-cap-(1.0-base)*0.45;
  if(shape<=0.0) {return vec4f(0.0,0.5,0.0,0.0);}
  // Domain shear and mixed cellular/value erosion break round lobes into
  // uneven turbulent edges. Flatter banks retain thin connected veils.
  let detailScale=vec3f(7.9);
  let detailPosition=physicalCloudRotate(warped)*detailScale+vec3f(billows*1.4,14.8,formation*2.0);
  let detail=physicalCloudFilteredShape(detailPosition,footprint*0.00062*7.9,0.7);
  let fine=physicalCloudFilteredShape(warped*20.3+vec3f(17.1,4.6,8.3),footprint*0.00062*20.3,0.5);
  let evaporation=clamp((1.0-updraft)*0.65+stratiform*0.25,0.0,1.0);
  let erosion=((1.0-detail)*mix(0.16,0.24,evaporation)+(1.0-fine)*0.065)*mix(1.0,0.8,stratiform);
  let densitySlope=mix(24.0,16.0,evaporation);
  let density=clamp((shape-erosion)*densitySlope,0.0,1.0);
  var escape=0.5;var nearOptical=0.0;
  if(shade&&density>0.001) {
    // Light the broad volume, not every erosion octave. Fine density detail
    // belongs on the silhouette; shading it as a normal made opaque interiors
    // look like a pile of pebbles.
    let delta=physicalCloudRotate(light*0.00062*35.0);
    let nearBroad=physicalCloudFilteredNoise(warped*0.73+delta*0.73,footprint*0.00062*0.73);
    let nearBillow=physicalCloudFilteredShape(physicalCloudRotate(warped)*lobeScale+vec3f(11.4,7.3,2.8)+physicalCloudRotate(delta)*lobeScale,footprint*0.00062*lobeScale,0.8);
    let nearHeight=height+light.y*35.0/(span*layerThickness);
    let nearCap=smoothstep(capStart,top,nearHeight)*0.68;
    let nearBase=smoothstep(0.0,0.09+(1.0-nearBroad)*0.12,nearHeight);
    let difference=(broad-nearBroad)*0.55+(billows-nearBillow)*0.45-cap+nearCap+(base-nearBase)*0.45;
    escape=clamp(0.5+difference*16.0,0.0,1.0);
    // Medium-scale erosion carries folds in the optical boundary. Keep the
    // finest octave out of directional lighting so it cannot become pebble normals.
    let nearDetail=physicalCloudFilteredShape(detailPosition+physicalCloudRotate(delta)*detailScale,
      footprint*0.00062*7.9,0.7);
    let opticalDifference=difference+(detail-nearDetail)*mix(0.16,0.24,evaporation)*mix(1.0,0.8,stratiform);
    let raw=(shape-erosion)*densitySlope;
    nearOptical=compiledCloudDensityIntegral(raw,raw-opticalDifference*densitySlope*(300.0/35.0))*300.0*PHYSICAL_CLOUD_EXTINCTION;
  }
  return vec4f(density*smoothstep(0.0,0.025,height),escape,0.0,nearOptical);

}
// Large semantic envelopes carry identity; noise only breaks their boundaries.
// The support is the authored oriented box, including shear and all lobes.
fn physicalCloudLobe(q:vec3f,center:vec3f,radius:vec3f)->f32 {
  return 1.0-length((q-center)/radius);
}
fn physicalCloudJoin(a:f32,b:f32)->f32 {
  let h=max(0.16-abs(a-b),0.0)/0.16;
  return max(a,b)+h*h*0.04;
}
// x: raw extinction field, y: broad lighting field, z: finite support.
// Keeping the unclamped boundary makes the near-light integral resolve soft
// billows instead of deriving a noisy surface normal from saturated density.
fn physicalCloudFormationField(stable:vec3f,footprint:f32,form:PhysicalCloudFormation,index:u32)->vec3f {
  let offset=stable-form.origin.xyz;
  let local=vec3f(form.detail.x*offset.x+form.detail.y*offset.z,offset.y,
    -form.detail.y*offset.x+form.detail.x*offset.z);
  let q=local/form.size.xyz;let h=q.y;
  if(h<=0.0||h>=1.0||abs(q.x)>=1.0||abs(q.z)>=1.0||form.size.w<=0.0) {return vec3f(-2.0,-2.0,0.0);}
  let seed=vec3f(form.detail.w*0.173,form.detail.w*0.317,form.detail.w*0.137);
  var shape=-4.0;var activity=0.0;
  for(var lobe=0u;lobe<6u;lobe++) {
    let growth=physicalGrowth(index,lobe);
    let field=physicalCloudLobe(q,growth.center.xyz,growth.radius.xyz);
    if(field>shape) {activity=growth.center.w;}
    shape=physicalCloudJoin(shape,field);
  }
  // No bounded noise term can reach this point. Skip four detail fetches in
  // the open space between updrafts, keeping all lobes inside authored support.
  if(shape < -0.75) {return vec3f(-2.0,-2.0,0.0);}
  if(form.origin.w>=1.5) {
    // Dissipating wisps retain a torn, sheared field. The denser joined-lobe
    // boundary used by growing clouds would turn this family into a solid bar.

    let broad=physicalCloudFilteredNoise(local*0.0008+seed,footprint*0.0008);
    let billow=physicalCloudFilteredShape(local*0.0032+seed,footprint*0.0032,0.85);
    let detail=physicalCloudFilteredShape(physicalCloudRotate(local)*vec3f(0.0018,0.011,0.007)+seed,footprint*0.011,0.6);
    let fine=physicalCloudFilteredNoise(local*0.022+seed,footprint*0.022);
    let boundary=shape*0.55+(broad-0.5)*0.46+(billow-0.5)*0.36;
    let erosion=((1.0-detail)*0.28+(1.0-fine)*0.085)*(0.55+form.detail.z);
    let support=smoothstep(0.0,0.13,h)*(1.0-smoothstep(0.82,1.0,max(abs(q.x),abs(q.z))))*
      (1.0-smoothstep(0.91,1.0,h))*form.size.w;
    return vec3f((boundary-erosion)*8.0,(boundary-(1.0-detail)*0.10*(0.55+form.detail.z))*8.0,support*0.35);
  }
  let growing=1.0-smoothstep(0.5,1.5,form.origin.w);
  let broad=physicalCloudFilteredShape(local*0.00085+seed,footprint*0.00085,0.8);
  let billow=physicalCloudFilteredShape(physicalCloudRotate(local)*0.0022+seed+vec3f(2.8,7.1,1.7),footprint*0.0022,0.8);
  let detail=physicalCloudFilteredShape(physicalCloudRotate(local)*mix(vec3f(0.0018,0.011,0.007),vec3f(0.007),growing)+seed,
    footprint*0.011,0.65);
  let fine=physicalCloudFilteredNoise(local*0.026+seed,footprint*0.026);
  // Young tips retain coherent billows. Mature shoulders lose density in
  // irregular sheets and fragments; detail amplitude is never uniform.
  let evaporation=clamp((1.0-activity)*0.65+form.growth.x*0.5+(broad-0.5)*0.45,0.0,1.0);
  let boundary=shape*0.65+(broad-0.5)*0.48+(billow-0.5)*mix(0.28,0.38,activity);
  let erosion=((1.0-detail)*mix(0.11,0.27,evaporation)+(1.0-fine)*mix(0.025,0.075,evaporation))*(0.55+form.detail.z);
  let base=smoothstep(0.0,mix(0.035,0.13,form.origin.w/2.0),h);
  let support=base*(1.0-smoothstep(0.87,1.0,max(abs(q.x),abs(q.z))))*(1.0-smoothstep(0.95,1.0,h))*form.size.w;
  let extinctionSlope=mix(9.0,5.0,evaporation);
  return vec3f((boundary-erosion)*extinctionSlope,
    (boundary-(1.0-detail)*mix(0.035,0.11,evaporation)*(0.55+form.detail.z))*extinctionSlope,support);
}
fn physicalCloudFormationSample(stable:vec3f,footprint:f32,light:vec3f,shade:bool,form:PhysicalCloudFormation,index:u32)->vec4f {
  let offset=stable-form.origin.xyz;
  let local=vec3f(form.detail.x*offset.x+form.detail.y*offset.z,offset.y,
    -form.detail.y*offset.x+form.detail.x*offset.z);
  let outside=max(max(abs(local.x)-form.size.x,abs(local.z)-form.size.z),max(-local.y,local.y-form.size.y));
  // The authored box remains conservative under spherical altitude mapping.
  if(outside>0.0) {return vec4f(0.0,0.5,outside*0.70710678,0.0);}
  let field=physicalCloudFormationField(stable,footprint,form,index);
  let density=clamp(field.x,0.0,1.0)*field.z;
  var escape=0.5;var optical=0.0;
  if(shade&&density>0.001) {
    let near=physicalCloudFormationField(stable+light*60.0,footprint,form,index);
    let difference=field.y-near.y;
    escape=clamp(0.5+difference*0.55,0.0,1.0);
    optical=compiledCloudDensityIntegral(field.x,field.x-difference*5.0)*field.z*300.0*PHYSICAL_CLOUD_EXTINCTION;
  }
  return vec4f(density,escape,0.0,optical);
}
fn physicalCloudSample(world:vec3f,footprint:f32,light:vec3f,shade:bool)->vec4f {
  let frame=physicalFrame();
  var result=vec4f(0.0,0.5,100000.0,0.0);
  if(physicalCloudForms().y>0.001) {
    result=physicalCloudWeatherSample(world,footprint,light,shade);
  }
  if(frame.cloud.x<=0.001) {return vec4f(0.0,0.5,0.0,0.0);}
  let drift=vec3f(frame.cloud.y,0.0,frame.cloud.z)*frame.cloud.w;
  let stable=physicalCloudPosition(world)-drift;
  for(var i=0u;i<u32(physicalCloudForms().x);i++) {
    let form=physicalFormation(i);
    let field=physicalCloudFormationSample(stable,footprint,light,shade,form,i);
    let support=min(result.z,field.z);
    if(field.x>result.x) {result=field;}
    result.z=support;
  }
  result.x*=smoothstep(0.0,0.15,frame.cloud.x);
  return result;
}
fn physicalCloudDensityFiltered(world:vec3f,footprint:f32)->f32 {
  return physicalCloudSample(world,footprint,vec3f(0.0),false).x;
}

fn physicalCloudDensity(world:vec3f)->f32 {return physicalCloudDensityFiltered(world,0.0);}
// Fallback for rays outside the camera-centred cached shadow field.
fn physicalCloudShadow(world:vec3f,ray:vec3f)->f32 {
  if(physicalFrame().cloud.x<=0.001||ray.y<=0.0) {return 0.0;}
  let altitude=physicalCloudPosition(world).y;
  let interval=physicalCloudLayerRange(world,ray,PHYSICAL_CLOUD_BASE,physicalCloudTop());
  let entry=interval.x;let exit=interval.y;
  if(entry>50000.0) {return 0.0;}
  let path=min(16000.0,exit-entry);
  let density=physicalCloudDensityFiltered(world+ray*(entry+path*0.5),path);
  return (1.0-exp(-density*path*PHYSICAL_CLOUD_EXTINCTION))*(1.0-smoothstep(25000.0,50000.0,entry));
}
fn physicalCloudMask(world:vec3f,ray:vec3f)->f32 {return physicalCloudShadow(world,ray);}
fn physicalCloudPhase(cosine:f32,g:f32)->f32 {
  return (1.0-g*g)/(4.0*ATMOSPHERE_PI*pow(max(0.001,1.0+g*g-2.0*g*cosine),1.5));
}
// Keep the inner 24 km at 250 m per texel; spend the border on distant
// cloud lighting out to 64 km. Far sky must not fall back to a fresh light
// march at every view sample. This is a nonuniform light grid, not density LOD.
fn physicalCloudLightCoordinate(offset:vec2f)->vec2f {
  let distance=abs(offset);
  return sign(offset)*select(distance/16000.0,0.75+(distance-12000.0)/52000.0*0.25,distance>vec2f(12000.0))*0.5+0.5;
}
fn physicalCloudLightWorld(uv:vec2f)->vec2f {
  let signedUV=uv*2.0-1.0;let unit=abs(signedUV);
  return sign(signedUV)*select(unit*16000.0,12000.0+(unit-0.75)*208000.0,unit>vec2f(0.75));
}
// The shared light field supplies direct-sun visibility in the air as well as
// on the ground. Diffuse atmospheric light remains unshadowed by this closure.
fn physicalCloudAirVisibility(world:vec3f,sun:vec3f)->f32 {
  let frame=physicalFrame();
  if(frame.cloud.x<=0.001||sun.y<=0.01||frame.skyCycle.x>0.5) {return 1.0;}
  let height=physicalCloudPosition(world).y;
  if(height>physicalCloudTop()) {return 1.0;}
  let entry=physicalCloudLayerRange(world,sun,PHYSICAL_CLOUD_BASE,physicalCloudTop()).x;
  let point=world+sun*entry;
  let stable=physicalCloudPosition(point);
  let center=floor(frame.camera.xz/250.0)*250.0;
  let uv=vec3f(physicalCloudLightCoordinate(point.xz-center),
    (stable.y-PHYSICAL_CLOUD_BASE)/(physicalCloudTop()-PHYSICAL_CLOUD_BASE));
  // Projection lands on the shell base. Roundoff can put its z a fraction
  // below zero; that must not randomly discard an otherwise valid shadow.
  if(any(uv.xy<vec2f(0.0))||any(uv.xy>vec2f(1.0))) {return 1.0;}
  let halfTexel=vec3f(0.5)/vec3f(textureDimensions(physicalCloudLightTexture));
  let depth=textureSampleLevel(physicalCloudLightTexture,physicalAtmosphereSampler,clamp(uv,halfTexel,1.0-halfTexel),0.0).x;
  return exp(-max(0.0,depth));
}
fn physicalCloudSolarDepth(point:vec3f,sun:vec3f)->f32 {
  let frame=physicalFrame();
  let castRay=normalize(select(frame.sun.xyz,frame.moon.xyz,frame.skyCycle.x>0.5));
  let stable=physicalCloudPosition(point);
  let center=floor(frame.camera.xz/250.0)*250.0;
  let uv=vec3f(physicalCloudLightCoordinate(point.xz-center),
    (stable.y-PHYSICAL_CLOUD_BASE)/(physicalCloudTop()-PHYSICAL_CLOUD_BASE));
  if(dot(sun,castRay)>0.999&&all(uv.xy>=vec2f(0.0))&&all(uv.xy<=vec2f(1.0))&&
    stable.y>=PHYSICAL_CLOUD_BASE-1.0&&stable.y<=physicalCloudTop()+1.0) {
    let halfTexel=vec3f(0.5)/vec3f(textureDimensions(physicalCloudLightTexture));
    return textureSampleLevel(physicalCloudLightTexture,physicalAtmosphereSampler,
      clamp(uv,halfTexel,vec3f(1.0)-halfTexel),0.0).x;
  }
  let height=physicalCloudPosition(point).y;
  let edge=select(PHYSICAL_CLOUD_BASE,physicalCloudTop(),sun.y>=0.0);
  let path=min(7000.0,max(0.0,(edge-height)/select(-max(0.025,-sun.y),max(0.025,sun.y),sun.y>=0.0)));
  var depth=0.0;
  // Quadratic spacing resolves nearby self-shadow while retaining the whole
  // supported light column. Three samples replace a single arbitrary offset.
  for(var i=0u;i<3u;i++) {
    let a=f32(i)/3.0;let b=f32(i+1u)/3.0;
    let start=path*a*a;let end=path*b*b;
    depth+=physicalCloudDensityFiltered(point+sun*((start+end)*0.5),end-start)*(end-start);
  }
  return depth*PHYSICAL_CLOUD_EXTINCTION;
}
// The density field and atmosphere are separable from view direction. Compile
// incident direct/diffuse radiance once; keep the scattering phase per ray.
// Isotropic orders 4..7 of the octave scattering approximation. This scalar
// closure restores deep transport without more density samples. The ambient
// cache's alpha channel is reserved for independent diffuse-sky visibility.
fn physicalCloudScatteringTail(optical:f32)->f32 {
  let t=exp(-max(0.0,optical)/16.0);let t2=sqrt(t);let t3=sqrt(t2);let t4=sqrt(t3);
  return 0.2401*t+0.16807*t2+0.117649*t3+0.0823543*t4;
}
fn physicalCloudDiffuseVisibility(point:vec3f)->f32 {
  let height=physicalCloudPosition(point).y;
  var visibility=0.0;
  // Four broad sky directions admit blue light through openings and around
  // shoulders. A vertical-only column aliased in the cache and darkened sides.
  for(var direction=0u;direction<4u;direction++) {
    var ray=vec3f(0.0,1.0,0.0);
    switch direction {
      case 1u: {ray=vec3f(0.7071,0.7071,0.0);}
      case 2u: {ray=vec3f(-0.35355,0.7071,0.61237);}
      case 3u: {ray=vec3f(-0.35355,0.7071,-0.61237);}
      default: {}
    }
    let path=max(0.0,physicalCloudTop()-height)/ray.y;
    var depth=0.0;
    for(var i=0u;i<3u;i++) {
      let a=f32(i)/3.0;let b=f32(i+1u)/3.0;
      let start=path*a*a;let end=path*b*b;
      // Diffuse cones average sub-grid erosion, unlike the directional solar
      // column. Retain broad openings while filtering unresolved occluders.
      depth+=physicalCloudDensityFiltered(point+ray*((start+end)*0.5),max(600.0,end-start))*(end-start);
    }
    visibility+=exp(-depth*PHYSICAL_CLOUD_EXTINCTION*0.35)*0.25;
  }
  // A bounded diffuse transport approximation; not a converged path trace.
  return 0.18+0.82*visibility;
}
struct PhysicalCloudLighting { optical:f32, solar:vec3f, ambient:vec3f, visibility:f32 };
fn physicalCloudLighting(point:vec3f,sun:vec3f,isMoon:bool)->PhysicalCloudLighting {
  let frame=physicalFrame();
  let castRay=normalize(select(frame.sun.xyz,frame.moon.xyz,frame.skyCycle.x>0.5));
  let stable=physicalCloudPosition(point);
  let center=floor(frame.camera.xz/250.0)*250.0;
  let uv=vec3f(physicalCloudLightCoordinate(point.xz-center),
    (stable.y-PHYSICAL_CLOUD_BASE)/(physicalCloudTop()-PHYSICAL_CLOUD_BASE));
  if(PHYSICAL_CLOUD_CACHED_LIGHTING&&isMoon==(frame.skyCycle.x>0.5)&&dot(sun,castRay)>0.999&&
    all(uv>=vec3f(0.0))&&all(uv<=vec3f(1.0))) {
    let halfTexel=vec3f(0.5)/vec3f(textureDimensions(physicalCloudLightTexture));
    let sampleUV=clamp(uv,halfTexel,vec3f(1.0)-halfTexel);
    let direct=textureSampleLevel(physicalCloudLightTexture,physicalAtmosphereSampler,sampleUV,0.0);
    let ambient=textureSampleLevel(physicalCloudAmbientTexture,physicalAtmosphereSampler,sampleUV,0.0);
    return PhysicalCloudLighting(direct.x,direct.yzw,ambient.xyz,ambient.w);
  }
  let optical=physicalCloudSolarDepth(point,sun);
  let planetPoint=physicalPlanetPoint(point);
  let visibility=physicalCloudDiffuseVisibility(point);
  return PhysicalCloudLighting(optical,physicalSunTransmissionAt(planetPoint,sun),
    physicalDiffuseRadiance(planetPoint,sun)*visibility,visibility);
}
// Fixed pixel scrambling avoids marching every camera row at the same depth
// phase. It does not change with time, so an unchanged cached view is stable.
fn physicalCloudQuadraturePhase(pixel:vec2u)->f32 {
  var bits=pixel.x+pixel.y*65537u+0x9e3779b9u;
  bits=(bits^(bits>>16u))*0x7feb352du;
  bits=(bits^(bits>>15u))*0x846ca68bu;
  bits=bits^(bits>>16u);
  return f32(bits&0x00ffffffu)/16777216.0;
}
// Thin middle and ice-cloud decks have independent weather, drift and altitude.
// Spherical intersections keep both layers present through the grazing horizon.
struct PhysicalCloudSheet { opacity:f32, distance:f32, optical:f32 };
fn physicalCloudSheet(world:vec3f,ray:vec3f,pixelAngle:f32,middle:bool)->PhysicalCloudSheet {
  let frame=physicalFrame();let layers=physicalCloudLayers();
  let cover=select(frame.cloudShape.z,layers.x,middle);
  if(cover<=0.001) {return PhysicalCloudSheet(0.0,0.0,0.0);}
  let height=select(layers.w,layers.y,middle);
  let roots=physicalCloudSphereRoots(world,ray,height);
  let distance=select(roots.y,roots.x,roots.x>0.0);
  if(distance<=0.0||distance>360000.0) {return PhysicalCloudSheet(0.0,0.0,0.0);}
  let hit=world+ray*distance;
  let drift=vec2f(frame.cloud.y,frame.cloud.z)*frame.cloud.w*select(1.5,1.18,middle);
  let point=physicalCloudPosition(hit).xz-drift;
  let heading=select(physicalCloudForms().w,layers.z,middle);
  let c=cos(heading);let sn=sin(heading);
  let oriented=vec2f(point.x*c+point.y*sn,-point.x*sn+point.y*c);
  let footprint=distance*pixelAngle;
  let warp=vec2f(physicalCloudFilteredNoise(physicalCloudRotate(vec3f(oriented*0.00009,13.7)),footprint*0.00009),
    physicalCloudFilteredNoise(physicalCloudRotate(vec3f(oriented*0.00011,37.1)),footprint*0.00011))-0.5;
  let q=oriented+warp*1600.0;
  var optical=0.0;
  if(middle) {
    // Broken altocumulus: a coarse weather mask and small, unequal cells.
    let weather=physicalCloudFilteredNoise(physicalCloudRotate(vec3f(q*0.00016,23.4)),footprint*0.00016);
    let cells=physicalCloudFilteredShape(physicalCloudRotate(vec3f(q.x*0.0017,5.1,q.y*0.0023)),footprint*0.0023,0.55);
    let erosion=physicalCloudFilteredNoise(physicalCloudRotate(vec3f(q.x*0.0031,2.7,q.y*0.0047)),footprint*0.0047);
    let group=smoothstep(0.61-cover*0.32,0.79-cover*0.30,weather);
    let fine=physicalCloudFilteredShape(physicalCloudRotate(vec3f(q.x*0.0091,12.7,q.y*0.0117)),footprint*0.0117,0.7);
    optical=group*cover*smoothstep(0.40,0.79,cells*0.60+erosion*0.25+fine*0.15)*0.62;
  } else {
    // Continuous wind-stretched turbulence replaces the repeated parabolic
    // brush envelopes. Several scales break fibers into overlapping veils.
    let weather=physicalCloudFilteredNoise(physicalCloudRotate(vec3f(q*0.00014,17.6)),footprint*0.00014);
    let body=physicalCloudFilteredNoise(physicalCloudRotate(vec3f(q.x*0.00015,8.7,q.y*0.0007)),footprint*0.0007);
    let fall=q.y+(body-0.5)*640.0;
    let filament=physicalCloudFilteredNoise(physicalCloudRotate(vec3f(q.x*0.0004,3.1,fall*0.0060)),footprint*0.0060);
    let fine=physicalCloudFilteredNoise(physicalCloudRotate(vec3f(q.x*0.0011,9.3,fall*0.025)),footprint*0.025);
    let group=smoothstep(0.60-cover*0.26,0.86-cover*0.24,weather*0.7+body*0.3);
    let fibers=smoothstep(0.20,0.80,filament*0.65+fine*0.35);
    let breaks=physicalCloudFilteredNoise(physicalCloudRotate(vec3f(q.x*0.0037,23.1,q.y*0.0043)),footprint*0.0043);
    optical=group*cover*fibers*fibers*(0.3+breaks*0.7)*0.50;
  }
  let incidence=abs(dot(ray,normalize(physicalPlanetPoint(hit))));
  let opacity=(1.0-exp(-optical/max(0.16,incidence)))*(1.0-smoothstep(280000.0,360000.0,distance));
  return PhysicalCloudSheet(opacity,distance,optical);
}
fn physicalCloudSheetComposite(world:vec3f,ray:vec3f,behind:vec3f,sheet:PhysicalCloudSheet,skyFill:vec3f,middle:bool)->vec3f {
  if(sheet.opacity<=0.0001) {return behind;}
  let f=physicalFrame();let point=world+ray*sheet.distance;let planet=physicalPlanetPoint(point);
  let sun=normalize(f.sun.xyz);let cosine=dot(ray,sun);
  let visibility=exp(-sheet.optical*select(1.0,2.3,middle)/max(0.12,abs(sun.y)));
  let scatter=0.06+0.14*pow(max(0.0,cosine),8.0);
  let light=skyFill*(0.28+0.22*visibility)+f.sunlight.xyz*f.sun.w*
    (physicalSunTransmissionAt(planet,sun)*scatter*visibility+physicalDiffuseRadiance(planet,sun)*0.70)+
    vec3f(0.72,0.82,1.0)*f.moon.w*0.045;
  let air=physicalCloudAir(world,ray,sheet.distance,false);
  return mix(behind,light*air.transmission+air.radiance,sheet.opacity);
}
fn physicalCloudAir(world:vec3f,ray:vec3f,distance:f32,useView:bool)->PhysicalAtmosphereTransport {
  return physicalIntegrateAtmosphere(world,ray,distance*0.001,12u);
}
fn physicalCloudSkyFill(world:vec3f)->vec3f {
  return physicalIntegrateAtmosphere(world,vec3f(0.0,1.0,0.0),200.0,24u).radiance;
}
var<private> physicalCloudDepthMoments:vec2f;
fn physicalCloudRadiance(world:vec3f,ray:vec3f,clear:vec3f,phaseOffset:f32,useView:bool,pixelAngle:f32)->vec4f {
  physicalCloudDepthMoments=vec2f(80000.0,0.0);
  let frame=physicalFrame();
  if(ray.y<=-0.04) {return vec4f(clear,1.0);}
  let skyFill=physicalCloudSkyFill(world);
  let high=physicalCloudSheet(world,ray,pixelAngle,false);
  let middle=physicalCloudSheet(world,ray,pixelAngle,true);
  let sheetTransmission=(1.0-high.opacity)*(1.0-middle.opacity);
  let sheetWeight=1.0-sheetTransmission;
  let sheetDepth=middle.distance*middle.opacity+high.distance*high.opacity*(1.0-middle.opacity);
  if(sheetWeight>0.0001) {
    let mean=sheetDepth/sheetWeight;
    let spread=max(select(0.0,abs(mean-high.distance),high.opacity>0.01),
      select(0.0,abs(mean-middle.distance),middle.opacity>0.01));
    physicalCloudDepthMoments=vec2f(mean,spread);
  }
  let behind=physicalCloudSheetComposite(world,ray,
    physicalCloudSheetComposite(world,ray,clear,high,skyFill,false),middle,skyFill,true);
  if(frame.cloud.x<=0.001) {return vec4f(behind,sheetTransmission);}
  let altitude=physicalCloudPosition(world).y;
  let interval=physicalCloudLayerRange(world,ray,PHYSICAL_CLOUD_BASE,physicalCloudSupportTop());
  let entry=interval.x;let exit=interval.y;
  if(exit<=entry||entry>140000.0) {return vec4f(behind,sheetTransmission);}
  let path=min(48000.0,exit-entry);
  // Allocate quadrature to path length. Detail filtering uses the projected
  // pixel footprint, independent of this budget, so references integrate the
  // same authored field. This is a quadrature budget, not an SDF step.
  let desired=u32(ceil(path/PHYSICAL_CLOUD_TARGET_STEP/2.0))*2u;
  let budget=select(PHYSICAL_CLOUD_VIEW_STEPS,max(PHYSICAL_CLOUD_VIEW_STEPS,PHYSICAL_CLOUD_FORMATION_STEPS),physicalCloudForms().x>0.5);
  let count=select(PHYSICAL_CLOUD_VIEW_STEPS,clamp(desired,32u,budget),PHYSICAL_CLOUD_ADAPTIVE);
  let distanceScale=max(2000.0,entry);
  let logPath=log(1.0+path/distanceScale);
  let logStep=logPath/f32(count);
  let sun=normalize(frame.sun.xyz);let cosine=dot(ray,sun);
  let phase=0.75*physicalCloudPhase(cosine,0.78)+0.25*physicalCloudPhase(cosine,-0.3);
  let moon=normalize(frame.moon.xyz);let moonCosine=dot(ray,moon);
  let moonPhase=0.75*physicalCloudPhase(moonCosine,0.78)+0.25*physicalCloudPhase(moonCosine,-0.3);
  let phase2=physicalCloudPhase(cosine,0.325);let phase3=physicalCloudPhase(cosine,0.1625);
  let moonPhase2=physicalCloudPhase(moonCosine,0.325);let moonPhase3=physicalCloudPhase(moonCosine,0.1625);
  let lowSun=1.0-smoothstep(0.15,0.45,sun.y);
  var layerLight:array<vec3f,4>;var layerWeight:array<f32,4>;var layerDepth:array<f32,4>;
  var transmission=1.0;var light=vec3f(0.0);var cloudDepth=0.0;var firstDepth=80000.0;var lastDepth=0.0;
  for(var sample=0u;sample<count;sample++) {
    // Paired Gauss nodes with bounded spatial phase dispersion. No time-varying
    // noise: an unchanged view is stable. Each node integrates a Beer segment.
    let node=select(clamp(select(0.42264973081,0.57735026919,(sample&1u)==1u)+(phaseOffset-0.5)*PHYSICAL_CLOUD_PHASE_SPREAD,0.02,0.98),fract(phaseOffset+f32(sample)*0.61803398875),PHYSICAL_CLOUD_JITTER);
    let start=distanceScale*(exp(logStep*f32(sample))-1.0);
    let end=distanceScale*(exp(logStep*f32(sample+1u))-1.0);
    let step=end-start;
    let distance=entry+mix(start,end,node);
    let point=world+ray*distance;
    let footprint=max(8.0,distance*pixelAngle);
    let field=physicalCloudSample(point,footprint,select(sun,moon,frame.skyCycle.x>0.5),true);
    let density=field.x;
    if(PHYSICAL_CLOUD_ADAPTIVE&&field.z>step*2.0) {
      // Leave a full segment before the certified ceiling and preserve the
      // paired quadrature phase; fixed-step references never take this path.
      let boundary=max(0.0,log(1.0+(start+field.z)/distanceScale)/logStep-2.0);
      let skip=u32(max(0.0,floor(boundary)-f32(sample)))/2u*2u;
      sample+=min(skip,count-sample-1u);
    }
    if(density>0.001) {
      let opacity=1.0-exp(-density*step*PHYSICAL_CLOUD_EXTINCTION);
      var source=vec3f(0.0);
      if(sun.y>-0.18) {
        let lighting=physicalCloudLighting(point,sun,false);
        source+=skyFill*(0.12+0.68*lighting.visibility);
        // Four directional orders, plus cached late isotropic transport at low
        // sun. Normalization preserves zero-depth energy rather than brightening
        // every sunlit contour. This remains an artistic transport closure.
        var optical=lighting.optical;
        if(sun.y>0.1) {
          // Replace the grid's blurred near-light column with the compiler's
          // analytic integral of the local density jet. Long-range occlusion
          // stays in the shared field; this adds no nested density march.
          let farOptical=physicalCloudSolarDepth(point+sun*300.0,sun);
          optical=mix(optical,farOptical+field.w,smoothstep(0.1,0.4,sun.y));
        }
        let t=exp(-optical*0.25);let t2=t*t;
        let early=0.7*t2*phase2+0.49*t*phase3+0.343*sqrt(t)*0.07957747;
        // Preserve the isotropic zero-depth sum: 1.533 / 2.1411733.
        let multiple=mix(early,(early+physicalCloudScatteringTail(optical)*0.07957747)*0.71596260,lowSun);
        let scatter=t2*t2*phase+multiple*(0.25+0.75*field.y);
        let powder=mix(1.0-exp(-density*12.0),1.0,smoothstep(-0.2,0.8,cosine));
        source+=(lighting.ambient*mix(0.75,0.55,smoothstep(0.1,0.4,sun.y))+lighting.solar*scatter*powder)*frame.sunlight.xyz*frame.sun.w;
      }
      if(frame.moon.w>0.0001) {
        let lighting=physicalCloudLighting(point,moon,true);
        if(sun.y<=-0.18) {source+=skyFill*(0.12+0.68*lighting.visibility);}
        let t=exp(-lighting.optical*0.25);let t2=t*t;
        let scatter=t2*t2*moonPhase+(0.7*t2*moonPhase2+0.49*t*moonPhase3+0.343*sqrt(t)*0.07957747)*(0.25+0.75*field.y);
        source+=vec3f(0.72,0.82,1.0)*frame.moon.w*(lighting.ambient*0.75+lighting.solar*scatter);
      }
      cloudDepth+=transmission*opacity*distance;
      firstDepth=min(firstDepth,max(entry,distance-step));lastDepth=max(lastDepth,distance+step);
      let bin=select(select(select(0u,1u,distance>5000.0),2u,distance>15000.0),3u,distance>35000.0);
      let weight=transmission*opacity;
      layerLight[bin]+=weight*source;layerWeight[bin]+=weight;layerDepth[bin]+=weight*distance;
      transmission*=1.0-opacity;
      if(transmission<0.01) {transmission=0.0;break;}
    }
  }
  // Clouds replace only the obscured part of the clear sky. Applying air to
  // the entire composite would double-count foreground atmospheric scattering.
  if(transmission<0.9999) {
    let totalWeight=1.0-transmission+transmission*sheetWeight;
    let mean=(cloudDepth+transmission*sheetDepth)/max(0.0001,totalWeight);
    var farthest=lastDepth;
    if(transmission*high.opacity>0.01) {farthest=max(farthest,high.distance);}
    if(transmission*middle.opacity>0.01) {farthest=max(farthest,middle.distance);}
    physicalCloudDepthMoments=vec2f(mean,max(mean-firstDepth,farthest-mean));
    for(var bin=0u;bin<4u;bin++) {
      if(layerWeight[bin]>0.00001) {
        let air=physicalCloudAir(world,ray,layerDepth[bin]/layerWeight[bin],useView);
        light+=layerLight[bin]*air.transmission+air.radiance*layerWeight[bin];
      }
    }
  }
  let distanceFade=smoothstep(110000.0,140000.0,entry);
  return vec4f(mix(light+behind*transmission,behind,distanceFade),
    mix(transmission*sheetTransmission,sheetTransmission,distanceFade));
}
