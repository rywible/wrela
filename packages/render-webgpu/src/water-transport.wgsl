// Bounded screen-space transport of opaque scene radiance. These are snapshots,
// never the active attachments. Missing reflection rays use the physical sky;
// missing transmission rays approach the deep-water single-scattering source.
@group(0) @binding(5) var waterOpaqueColor:texture_2d<f32>;
@group(0) @binding(6) var waterOpaqueDepth:texture_2d<f32>;
@group(0) @binding(7) var waterTransportSampler:sampler;
struct WaterScreenHit { position:vec3f, uv:vec2f, distance:f32, confidence:f32 };
fn waterProjectClip(p:vec4f)->vec4f {
  if(p.w<=0.00001) { return vec4f(0.0); }
  let q=p.xyz/p.w;
  let uv=vec2f(q.x*0.5+0.5,0.5-q.y*0.5);
  let valid=all(uv>vec2f(0.001))&&all(uv<vec2f(0.999))&&q.z>=0.0&&q.z<=1.0;
  return vec4f(uv,q.z,select(0.0,1.0,valid));
}
fn waterProject(world:vec3f)->vec4f { return waterProjectClip(g.vp*vec4f(world,1.0)); }
fn waterRawDepth(uv:vec2f)->f32 {
  let size=textureDimensions(waterOpaqueDepth);
  let pixel=min(vec2u(max(uv,vec2f(0.0))*vec2f(size)),size-vec2u(1));
  return textureLoad(waterOpaqueDepth,vec2i(pixel),0).r;
}
fn waterScenePosition(uv:vec2f)->vec4f {
  let size=textureDimensions(waterOpaqueDepth);
  let pixel=min(vec2u(max(uv,vec2f(0.0))*vec2f(size)),size-vec2u(1));
  let depth=textureLoad(waterOpaqueDepth,vec2i(pixel),0).r;
  if(depth>=0.999999 || depth<0.0) { return vec4f(0.0); }
  let uvCenter=(vec2f(pixel)+vec2f(0.5))/vec2f(size);
  let world=g.inverseVP*vec4f(uvCenter.x*2.0-1.0,1.0-uvCenter.y*2.0,depth,1.0);
  if(abs(world.w)<0.000001) { return vec4f(0.0); }
  return vec4f(world.xyz/world.w,1.0);
}
// Pay the four-neighbor reconstruction only at candidate hits and the stable
// transmission fallback. Coarse marching and bisection use one depth fetch.
fn waterScenePositionCoherent(uv:vec2f)->vec4f {
  let size=textureDimensions(waterOpaqueDepth);
  let pixel=min(vec2u(max(uv,vec2f(0.0))*vec2f(size)),size-vec2u(1));
  var coordinate=(vec2f(pixel)+vec2f(0.5))/vec2f(size);
  // Interpolate depth only across a coherent surface. Nearest-only depth turns
  // a planar riverbed into stairs and makes neighboring rays alternate between
  // accepted intersections and dark misses. Never interpolate silhouette gaps.
  let p=uv*vec2f(size)-vec2f(0.5); let base=vec2i(floor(p));
  let maximumPixel=vec2i(size)-vec2i(1);
  let a=textureLoad(waterOpaqueDepth,clamp(base,vec2i(0),maximumPixel),0).r;
  let b=textureLoad(waterOpaqueDepth,clamp(base+vec2i(1,0),vec2i(0),maximumPixel),0).r;
  let c=textureLoad(waterOpaqueDepth,clamp(base+vec2i(0,1),vec2i(0),maximumPixel),0).r;
  let d=textureLoad(waterOpaqueDepth,clamp(base+vec2i(1,1),vec2i(0),maximumPixel),0).r;
  let f=fract(p);
  var depth=select(select(a,b,f.x>=0.5),select(c,d,f.x>=0.5),f.y>=0.5);
  let minimumDepth=min(min(a,b),min(c,d)); let maximumDepth=max(max(a,b),max(c,d));
  if(maximumDepth<0.999999 && maximumDepth-minimumDepth<max(0.0000002,(1.0-minimumDepth)*0.025)) {
    depth=mix(mix(a,b,f.x),mix(c,d,f.x),f.y);coordinate=uv;
  }
  if(depth>=0.999999 || depth<0.0) { return vec4f(0.0); }
  let world=g.inverseVP*vec4f(coordinate.x*2.0-1.0,1.0-coordinate.y*2.0,depth,1.0);
  if(abs(world.w)<0.000001) { return vec4f(0.0); }
  return vec4f(world.xyz/world.w,1.0);
}
// Project the ray once. Depth comparisons need only one scalar texture read;
// full inverse projection is deferred until a candidate hit is found.
fn waterReflectionTrace(origin:vec3f,direction:vec3f,normal:vec3f)->WaterScreenHit {
 var hit:WaterScreenHit;hit.confidence=0.0;
 let o=g.vp*vec4f(origin,1.0);let d=g.vp*vec4f(direction,0.0);
 let gradient=vec2f(d.x*o.w-o.x*d.w,-d.y*o.w+o.y*d.w);
 var distance=0.015;var lod=min(4u,textureNumLevels(waterOpaqueDepth)-1u);
 for(var iteration=0u;iteration<48u;iteration++){
  let projected=waterProjectClip(o+d*distance);if(projected.w==0.0||distance>=80.0){break;}
  let size=textureDimensions(waterOpaqueDepth,lod);let pixel=vec2u(projected.xy*vec2f(size));
  let boundary=(vec2f(pixel)+select(vec2f(0.0),vec2f(1.0),gradient>=vec2f(0.0)))/vec2f(size);
  let ndc=boundary*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
  let denominator=d.xy-ndc*d.w;
  let crossing=(ndc*o.w-o.xy)/(sign(denominator)*max(abs(denominator),vec2f(1e-12))+vec2f(select(0.0,1e-12,denominator.x==0.0),select(0.0,1e-12,denominator.y==0.0)));
  let edge=min(select(81.0,crossing.x,crossing.x>distance+1e-6),select(81.0,crossing.y,crossing.y>distance+1e-6));
  let next=min(edge+0.00001,80.0);let end=waterProjectClip(o+d*next);
  if(end.w==0.0){break;}
  let nearest=textureLoad(waterOpaqueDepth,vec2i(min(pixel,size-1u)),i32(lod)).r;
  if(nearest>=0.999999||max(projected.z,end.z)<nearest-0.0000001){distance=next;lod=min(lod+1u,min(6u,textureNumLevels(waterOpaqueDepth)-1u));continue;}
  if(lod>0u){lod--;continue;}
  if(projected.z<=nearest+0.000001&&end.z>=nearest){
   var lo=distance;var hi=next;
   for(var step=0u;step<3u;step++){let mid=(lo+hi)*0.5;let q=waterProjectClip(o+d*mid);if(q.z>=waterRawDepth(q.xy)){hi=mid;}else{lo=mid;}}
   let t=(lo+hi)*0.5;let q=waterProjectClip(o+d*t);let surface=waterScenePositionCoherent(q.xy);
   let error=length(surface.xyz-(origin+direction*t));let thickness=0.08+t*0.035;
   if(surface.w>0.0&&error<thickness&&dot(surface.xyz-origin,normal)>-0.03){
    let border=min(min(q.x,1.0-q.x),min(q.y,1.0-q.y));hit.position=surface.xyz;hit.uv=q.xy;hit.distance=t;
    hit.confidence=smoothstep(0.0,0.05,border)*(1.0-smoothstep(thickness*0.2,thickness,error))*(1.0-smoothstep(55.0,80.0,t));return hit;
   }
  }
  distance=next;lod=1u;
 }
 return hit;
}
// Thin screen-space refraction: intersect the refracted ray with the opaque
// endpoint's view-depth plane, then correct that endpoint once. This is an
// explicit approximation, not scene ray traversal, and retains stable original
// transmission across disocclusion, foreground silhouettes and screen borders.
fn waterRefractedEndpoint(world:vec3f,direction:vec3f,normal:vec3f,beneath:vec4f)->WaterScreenHit {
  var hit:WaterScreenHit;hit.confidence=0.0;
  let forward=dot(direction,g.forward.xyz);
  if(beneath.w==0.0 || forward<=0.05) { return hit; }
  let first=clamp(dot(beneath.xyz-world,g.forward.xyz)/forward,0.0,24.0);
  let query=waterProject(world+direction*first);
  if(query.w==0.0) { return hit; }
  let candidate=waterScenePosition(query.xy);
  if(candidate.w==0.0) { return hit; }
  let corrected=dot(candidate.xyz-world,g.forward.xyz)/forward;
  if(corrected<0.0 || corrected>24.0) { return hit; }
  let projected=waterProject(world+direction*corrected);
  if(projected.w==0.0) { return hit; }
  let surface=waterScenePositionCoherent(projected.xy);
  let side=-dot(surface.xyz-world,normal);
  if(surface.w==0.0 || side< -0.035 || dot(surface.xyz-world,g.forward.xyz)<0.0) { return hit; }
  let error=abs(dot(world+direction*corrected-surface.xyz,g.forward.xyz));
  let allowance=0.15+corrected*0.1;
  let edge=min(min(projected.x,1.0-projected.x),min(projected.y,1.0-projected.y));
  hit.position=surface.xyz;hit.uv=projected.xy;hit.distance=length(surface.xyz-world);
  hit.confidence=smoothstep(0.0,0.04,edge)*(1.0-smoothstep(allowance*0.2,allowance,error))
    *smoothstep(-0.035,0.08,side);
  return hit;
}
fn waterFresnel(cosine:f32)->f32 {
  return 0.02037+0.97963*pow(1.0-clamp(cosine,0.0,1.0),5.0);
}
// integratedSky already contains the correlated micro-normal Fresnel response.
// Replacing it by interpolation preserves positivity; an additive macro sky
// subtraction could otherwise create negative radiance at narrow highlights.
fn waterTransportWithOptics(world:vec3f,normal:vec3f,view:vec3f,base:vec3f,rough:f32,integratedSky:vec3f,absorption:vec3f,caustics:bool,curvature:vec3f)->vec3f {
  let n=normalize(normal);
  let front=dot(n,view)>=0.0;
  let facing=select(-n,n,front);
  let cosine=max(dot(facing,view),0.0);
  let fresnel=waterFresnel(cosine);
  let reflected=reflect(-view,facing);
  var reflection:WaterScreenHit;reflection.confidence=0.0;
  if(rough<0.45){reflection=waterReflectionTrace(world+facing*0.015,reflected,facing);}
  var reflectionColor=max(integratedSky,vec3f(0.0));
  if(reflection.confidence>0.0) {
    let radiance=textureSampleLevel(waterOpaqueColor,waterTransportSampler,reflection.uv,0.0).rgb;
    let confidence=reflection.confidence*(1.0-smoothstep(0.12,0.6,rough));
    reflectionColor=mix(reflectionColor,max(radiance,vec3f(0.0))*fresnel,confidence);
  }
  let eta=select(1.333,1.0/1.333,front);
  let refracted=refract(-view,facing,eta);
  if(dot(refracted,refracted)<0.000001) {
    // Total internal reflection has no transmitted energy.
    return max(reflectionColor/max(fresnel,0.00001),vec3f(0.0));
  }

  let tint=clamp(base,vec3f(0.0),vec3f(1.0));
  // Metre-based absorption: long red paths disappear faster than blue/green.

  let ambient=max(physicalSkyWithoutSun(world,vec3f(0.0,1.0,0.0)),vec3f(0.0));
  var scattering=tint*ambient*0.25;
  if(caustics){scattering=waterBodyScattering(world,tint);}
  var transmitted=scattering;
  // A trace miss is not evidence of infinitely deep water. The undisplaced
  // opaque sample gives continuous shore contact and a stable transmission
  // fallback where a refracted ray leaves the screen or crosses a silhouette.
  let projected=waterProject(world);
  let beneath=waterScenePositionCoherent(projected.xy);
  let transmission=waterRefractedEndpoint(world,refracted,facing,beneath);
  if(projected.w>0.0 && beneath.w>0.0 && dot(beneath.xyz-world,g.forward.xyz)>-0.015 && dot(beneath.xyz-world,facing)<0.035) {
    let thickness=max(0.0,-dot(beneath.xyz-world,facing));
    let path=select(length(g.camera.xyz-world),thickness/max(abs(dot(refracted,facing)),0.05),front);
    let attenuation=exp(-absorption*min(path,1000.0));
    let behind=max(textureSampleLevel(waterOpaqueColor,waterTransportSampler,projected.xy,0.0).rgb,vec3f(0.0));
    var receiver=behind;if(caustics){receiver=waterBodyReceiverCaustic(beneath.xyz,behind,curvature,path);}
    transmitted=receiver*attenuation+scattering*(vec3f(1.0)-attenuation);
  }
  if(transmission.confidence>0.0) {
    let path=select(length(g.camera.xyz-world),transmission.distance,front);
    let attenuation=exp(-absorption*min(path,1000.0));
    let behind=max(textureSampleLevel(waterOpaqueColor,waterTransportSampler,transmission.uv,0.0).rgb,vec3f(0.0));
    var receiver=behind;if(caustics){receiver=waterBodyReceiverCaustic(transmission.position,behind,curvature,path);}
    let resolved=receiver*attenuation+scattering*(vec3f(1.0)-attenuation);
    transmitted=mix(transmitted,resolved,transmission.confidence);
  }
  return reflectionColor+(1.0-fresnel)*transmitted;
}
fn waterTransport(world:vec3f,n:vec3f,view:vec3f,base:vec3f,rough:f32)->vec3f {
  let fresnel=waterFresnel(abs(dot(normalize(n),view)));
  return waterTransportWithSky(world,n,view,base,rough,physicalSkyRoughReflection(reflect(-view,n),rough)*fresnel);
}

fn waterTransportWithSky(world:vec3f,normal:vec3f,view:vec3f,base:vec3f,rough:f32,integratedSky:vec3f)->vec3f {
  return waterTransportWithOptics(world,normal,view,base,rough,integratedSky,vec3f(0.42,0.13,0.065)*(vec3f(1.25)-clamp(base,vec3f(0.0),vec3f(1.0))),false,vec3f(0.0));
}
