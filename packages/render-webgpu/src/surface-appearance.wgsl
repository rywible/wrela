// Ordered procedural coatings. All stochastic detail uses pixel-footprint filtering.
fn authoredWovenPlane(phase:vec2f,dx:vec2f,dy:vec2f)->f32 {
  // The exact box integral retains sinc side lobes above Nyquist. Apply a
  // reconstruction low-pass as threads become subpixel to prevent moire.
  let width=max(length(dx),length(dy));
  let visibility=1.0-smoothstep(1.8,3.6,width);
  return mix(0.25,wovenCoverage(phase,dx,dy),visibility);
}
fn authoredWovenCoverage(p:vec3f)->f32 {
  let tau=6.283185307179586;
  let dx=dpdx(p)*tau;let dy=dpdy(p)*tau;
  let normal=abs(creatureSafeDirection(cross(dx,dy),vec3f(0,0,1)));
  let weights=normal*normal*normal*normal;
  let phase=p*tau+vec3f(obj.patternOrigins.z,obj.noiseOrigins[0].y*tau,obj.patternOrigins.w);
  // Project onto the surface-facing planes so front-facing fabric does not
  // collapse one carrier into rings around extrema of a curved surface.
  return (authoredWovenPlane(phase.yz,dx.yz,dy.yz)*weights.x+
    authoredWovenPlane(phase.xz,dx.xz,dy.xz)*weights.y+
    authoredWovenPlane(phase.xy,dx.xy,dy.xy)*weights.z)/max(dot(weights,vec3f(1)),1e-6);
}
// Integrate the smooth threshold over a uniform noise distribution. It is an
// occupancy estimate at subpixel scale, rather than thresholding the mean to 0/1.
fn surfaceThresholdIntegral(x:f32,low:f32,high:f32)->f32 {
  let t=clamp((x-low)/(high-low),0.0,1.0);
  return (high-low)*(t*t*t-0.5*t*t*t*t)+max(x-high,0.0);
}
fn authoredNoiseMask(threshold:f32,softness:f32,noise:f32,width:f32)->f32 {
  let low=threshold-softness;let high=threshold+softness;
  let mean=surfaceThresholdIntegral(1.0,low,high)-surfaceThresholdIntegral(0.0,low,high);
  return mix(smoothstep(low,high,noise),mean,smoothstep(0.25,0.85,width));
}
fn authoredSurfaceMask(layer:SurfaceLayer,p:vec3f,n:vec3f,detail:f32,footprint:f32)->f32 {
  var value=1.0;
  let kind=layer.properties.w;
  if(kind>0.5&&kind<1.5) {value=authoredNoiseMask(layer.mask.y,layer.mask.z,detail,footprint*layer.mask.x);}
  if(kind>1.5&&kind<2.5) {value=smoothstep(layer.mask.y-layer.mask.z,layer.mask.y+layer.mask.z,max(n.y,0.0));}
  if(kind>2.5&&kind<3.5) {value=smoothstep(layer.height.x-layer.mask.z,layer.height.x,p.y)*(1.0-smoothstep(layer.height.y,layer.height.y+layer.mask.z,p.y));}
  if(kind>3.5) {
    let noise=authoredNoiseMask(layer.mask.y,layer.mask.z,detail,footprint*layer.mask.x);
    let slope=smoothstep(layer.reserved.z-layer.reserved.w,layer.reserved.z+layer.reserved.w,max(n.y,0.0));
    let height=smoothstep(layer.height.x-layer.mask.z,layer.height.x,p.y)*(1.0-smoothstep(layer.height.y,layer.height.y+layer.mask.z,p.y));
    value=noise*mix(1.0,slope,layer.reserved.x)*mix(1.0,height,layer.reserved.y);
  }
  return mix(value,1.0-value,layer.mask.w)*layer.properties.y;
}
// Coatings obscure the underlying skin, leaf, fiber or clearcoat response.
// Keep the complete uncoated response, including explicit creature overrides.
fn authoredSurfaceDirect(base:vec3f,n:vec3f,view:vec3f,light:vec3f,tangent:vec3f,rough:f32,metallic:f32,irradiance:f32,visibility:f32,substrate:f32)->vec3f {
  if(LIGHTING_ABLATION==6u){return vec3f(0.0);}
  if(substrate>=1.0) {return creatureDirect(base,n,view,light,tangent,rough,metallic,irradiance,visibility);}
  let substance=creatureDirect(base,n,view,light,tangent,rough,metallic,irradiance,visibility);
  let f0=mix(vec3f(0.04),base,metallic);
  let fresnel=f0+(vec3f(1.0)-f0)*pow(clamp(1.0-dot(view,creatureSafeDirection(view+light,n)),0.0,1.0),5.0);
  let coating=base*(1.0-metallic)*(vec3f(1)-fresnel)*irradiance/BRDF_PI+brdfGGX(n,view,light,rough,mix(vec3f(0.04),base,metallic))*visibility;
  return mix(coating,substance,substrate);
}
fn authoredFoliageAmbient(base:vec3f,metallic:f32,frontSky:vec3f,backSky:vec3f,substrate:f32)->vec3f {
  let budget=foliageBudget(base,obj.creatureScatter.xyz,obj.creature.z,obj.creature.w);
  let thin=(budget.reflection*frontSky+budget.transmission*backSky)*(1.0-metallic);
  return mix(base*(1.0-metallic)*frontSky,thin,substrate);
}
fn authoredSurfaceHistory(p:vec3f,n:vec3f,footprint:f32)->vec3f {
  let frequency=obj.surfaceHistory.y;
  let detail=filteredNoise(p*frequency,obj.surfaceOrigin.xyz,footprint*frequency);
  let dirt=obj.surface.z*clamp(0.25+0.55*max(n.y,0.0)+0.4*detail,0.0,1.0);
  let damage=obj.surface.w*authoredNoiseMask(0.65,0.15,detail,footprint*frequency);
  return vec3f(detail,dirt,damage);
}
// Opaque environment-transmission approximation: does not claim scene refraction or transparency.
fn authoredGlass(base:vec3f,n:vec3f,view:vec3f,position:vec3f,rough:f32,reflected:vec3f)->vec3f {
  let ior=obj.surfaceHistory.w;
  let f0=pow((ior-1.0)/(ior+1.0),2.0);
  let fresnel=f0+(1.0-f0)*pow(1.0-max(dot(n,view),0.0),5.0);
  let refracted=refract(-view,n,1.0/ior);
  let ray=creatureSafeDirection(refracted,-view);
  let local=compiledIndirectSample(position,-n,ray,vec2f(rough,-1.0),true);
  let transmitted=compiledReflectionRadiance(local.reflection,ray,rough)*base;
  return transmitted*(1.0-fresnel)*obj.surfaceHistory.z+reflected*fresnel*(1.0-rough*0.5);
}

// Domain-scale detail without a texture atlas. Integer anisotropy preserves the
// periodic, rebased lattice; long wood/bark structures follow the local Y axis.
fn surfaceDetailNoise(p:vec3f,stretch:vec3f,width:f32)->f32 {
  let integral=obj.surfaceDetailOrigin.xyz*(stretch*20.0);
  let fraction=vec3f(obj.surfaceDamage.w,obj.surfaceOrigin.w,obj.surfaceDetail.w)*(stretch*20.0);
  let origin=integral-floor(integral/1024.0)*1024.0+fraction;
  return filteredNoise(p*stretch,origin,width*max(stretch.x,max(stretch.y,stretch.z)));
}
// Sparse rounded grains have actual support and an analytic subpixel mean.
// Keep each grain inside its cell so evaluation needs no neighbouring search.
fn surfaceSoilGrains(p:vec3f,frequency:f32,width:f32)->vec2f {
  let size=width*frequency;
  // 55% occupancy, radius uniformly in [.12,.27]. Integrals of the disc
  // and its squared paraboloid are pi*E[r*r] and one third of that value.
  let mean=vec2f(0.06894225,0.02298075);
  if(size>=0.85) {return mean;}
  let integral=obj.surfaceDetailOrigin.xz*(frequency*20.0);
  let fraction=vec2f(obj.surfaceDamage.w,obj.surfaceDetail.w)*(frequency*20.0);
  let q=p.xz*frequency+integral-floor(integral/1024.0)*1024.0+fraction;
  let cell=floor(q);
  let seed=vec3f(cell.x,19.0,cell.y);
  let r=hash(seed);
  let center=vec2f(0.3)+vec2f(hash(seed+vec3f(17,3,29)),hash(seed+vec3f(37,7,13)))*0.4;
  let occupied=select(0.0,1.0,hash(seed+vec3f(71,11,53))>0.45);
  let delta=(fract(q)-center)/mix(0.12,0.27,r);
  let squared=dot(delta,delta);
  let coverage=1.0-smoothstep(max(0.0,1.0-size*4.0),1.0+size*4.0,squared);
  let dome=pow(max(0.0,1.0-squared),2.0);
  return mix(vec2f(coverage,dome)*occupied,mean,smoothstep(0.2,0.85,size));
}
fn authoredSurfaceDetail(coordinates:vec3f,footprint:f32)->vec2f {
  let p=coordinates*obj.surfaceDetail.y;
  let width=footprint*obj.surfaceDetail.y;
  let strength=obj.surfaceDetail.z;
  var pattern=0.5;var relief=0.0;
  if(obj.surfaceDetail.x>4.5) {
    // Lenticular marks wrap across the branch; the compiler supplies longitudinal Y.
    let marks=surfaceDetailNoise(p,vec3f(5,90,5),width);
    let sheets=surfaceDetailNoise(p,vec3f(9,2,9),width);
    let pores=authoredNoiseMask(0.68,0.12,marks,width*90.0);
    pattern=clamp(0.76-pores*0.68+(sheets-0.5)*0.2,0.0,1.0);
    relief=(sheets-0.5)*0.0005-pores*0.00025;
  } else if(obj.surfaceDetail.x<1.5) {
    // One continuous solid growth field on side, bevel and cut faces. A sparse
    // branch collar bends the fibres as well as the rings; it is not a painted
    // circular decal. Support ends inside each longitudinal cell, without seams.
    let knotY=p.y*1.15+obj.surfaceDetailOrigin.w;
    let seed=vec3f(floor(knotY),obj.surfaceDetailOrigin.w,17.0);
    let angle=hash(seed)*6.2831853;
    let axis=vec2f(cos(angle),sin(angle));
    let across=dot(p.xz,vec2f(-axis.y,axis.x))-mix(-0.045,0.045,hash(seed+vec3f(9,3,7)));
    let along=(fract(knotY)-mix(0.3,0.7,hash(seed+vec3f(2,19,5))))/1.15-dot(p.xz,axis)*0.35;
    let knotRadius=mix(0.018,0.035,hash(seed+vec3f(31,7,11)));
    let knotDistance=length(vec2f(across,along*0.46));
    let knotSupport=(1.0-smoothstep(knotRadius,0.11,knotDistance))*step(0.38,hash(seed+vec3f(13,2,29)));
    let knotVisibility=1.0-smoothstep(0.003,0.018,width);
    let knotCore=(1.0-smoothstep(knotRadius*0.25,knotRadius,knotDistance))*knotVisibility*knotSupport;
    let warped=vec3f(p.x+axis.x*knotSupport*along*0.45,p.y,p.z+axis.y*knotSupport*along*0.45);
    let grain=surfaceDetailNoise(warped,vec3f(18,0.5,18),width);
    let fiber=surfaceDetailNoise(warped,vec3f(62,1.1,62),width);
    let drift=vec2f(surfaceDetailNoise(p,vec3f(0.7,0.55,0.7),width)-0.5,
      surfaceDetailNoise(p+vec3f(3.1,1.7,0.8),vec3f(0.9,0.4,0.9),width)-0.5)*0.035;
    let radius=length(warped.xz+vec2f(0.027,-0.019)+drift);
    let annualVariation=surfaceDetailNoise(vec3f(radius,0.0,0.0),vec3f(57.0,1.0,1.0),width);
    let ringPhase=radius*610.0+(annualVariation-0.5)*5.5+(grain-0.5)*0.65;
    let ringVisibility=1.0-smoothstep(0.002,0.011,width);
    // Narrow latewood bands within broad earlywood, with varying annual width.
    let latewood=smoothstep(0.42,0.96,sin(ringPhase));
    let knotRings=(0.5+0.5*sin(knotDistance*950.0))*knotSupport*knotVisibility;
    // Sparse longitudinal checks become recesses, with subpixel fadeout.
    let checks=authoredNoiseMask(0.79,0.055,grain,width*18.0)*(1.0-smoothstep(0.001,0.006,width));
    pattern=0.43+(grain-0.5)*0.37+(fiber-0.5)*0.22+(latewood-0.25)*0.20*ringVisibility+knotCore*0.34+knotRings*0.07+checks*0.2;
    relief=(grain-0.5)*0.0013+(fiber-0.5)*0.00045+(latewood-0.38)*0.00035*ringVisibility-checks*0.0012-knotCore*0.0004;
  } else if(obj.surfaceDetail.x<2.5) {
    let plate=surfaceDetailNoise(p,vec3f(7,0.8,7),width);
    let grain=surfaceDetailNoise(p,vec3f(24,2.0,24),width);
    let cells=vec2f(p.x*18.0+(grain-0.5)*0.3,p.y*5.0+floor(p.x*18.0)*0.37);
    let edge=abs(fract(cells)-0.5);
    let fissure=1.0-smoothstep(0.025,0.1,min(0.5-edge.x,0.5-edge.y));
    let resolved=1.0-smoothstep(0.2,0.85,width*18.0);
    pattern=0.62+(plate-0.5)*0.65+(grain-0.5)*0.25-fissure*resolved*0.42;
    relief=(plate-0.5)*0.003+(grain-0.5)*0.001-fissure*resolved*0.0035;
  } else if(obj.surfaceDetail.x>3.5) {
    // Metre-scale turf, decimetre clods and centimetre grit have distinct
    // support. Keep the middle band: dropping straight from broad patches to
    // subpixel grains made the terrain read as blurred plastic.
    let ground=vec3f(p.x,0.0,p.z);
    let patches=surfaceDetailNoise(ground,vec3f(0.55),width);
    let clods=surfaceDetailNoise(ground,vec3f(2.8),width);
    let grains=surfaceDetailNoise(ground,vec3f(11),width);
    let grit=surfaceDetailNoise(ground,vec3f(37),width);
    let pebbles=surfaceSoilGrains(ground,17.0,width);
    pattern=0.5+(patches-0.5)*0.25+(clods-0.5)*0.65+(grains-0.5)*0.5+(grit-0.5)*0.2+(pebbles.x-0.06894225)*0.2;
    relief=(clods-0.5)*0.024+(grains-0.5)*0.009+(grit-0.5)*0.0015+pebbles.y*0.003;
  } else {
    let coarse=surfaceDetailNoise(p,vec3f(0.9),width);
    let mineral=surfaceDetailNoise(p,vec3f(31,27,29),width);
    let poreNoise=surfaceDetailNoise(p,vec3f(80),width);
    let pores=1.0-authoredNoiseMask(0.25,0.07,poreNoise,width*80.0);
    let sediment=sin(p.y*13.82+obj.surfaceDetailOrigin.w+(coarse-0.5)*3.0);
    let sedimentVisibility=1.0-smoothstep(0.12,0.45,width*2.2);
    pattern=0.5+(coarse-0.5)*0.65+(mineral-0.5)*0.4+sediment*0.075*sedimentVisibility-pores*0.08;
    relief=(mineral-0.5)*0.0012-pores*0.0008;
  }
  return vec2f(mix(0.5,clamp(pattern,0.0,1.0),strength),relief*strength);
}
