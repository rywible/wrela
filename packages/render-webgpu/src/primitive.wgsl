// Appended to scene.wgsl. Conservative projected rectangles bound raster work;
// near-plane and camera-inside cases retain the complete viewport.
struct PrimitiveQuery {
  worldToUnit:mat4x4f, unitToLocal:mat4x4f,
  inverseVP:mat4x4f, inverseLightVP:mat4x4f,
  cameraRectangle:vec4f, lightRectangle:vec4f,
};
@group(2) @binding(0) var<storage,read> primitiveQueries:array<PrimitiveQuery>;
struct PrimitiveVarying {
  @builtin(position) clip:vec4f,
  // Analytic coverage comes from the ray hit, not the rectangle rasterizer.
  // Evaluate each MSAA sample so a silhouette can cover part of a pixel.
  @location(0) @interpolate(perspective,sample) uv:vec2f,
  @location(1) @interpolate(flat) identity:vec3f,
  @location(2) @interpolate(flat) queryIndex:u32,
};
fn primitiveRectangleVertex(index:u32,instance:u32,rectangle:vec4f)->PrimitiveVarying {
  var out:PrimitiveVarying;
  let corners=array<vec2f,6>(vec2f(0,0),vec2f(1,0),vec2f(0,1),vec2f(0,1),vec2f(1,0),vec2f(1,1));
  let p=mix(rectangle.xy,rectangle.zw,corners[index]);
  out.clip=vec4f(p,0.0,1.0); out.uv=p; out.identity=instances[instance].identity.xyz; out.queryIndex=instance;
  return out;
}
@vertex fn primitiveVertex(@builtin(vertex_index) index:u32,@builtin(instance_index) instance:u32)->PrimitiveVarying {
  return primitiveRectangleVertex(index,instance,primitiveQueries[instance].cameraRectangle);
}
@vertex fn primitiveShadowVertex(@builtin(vertex_index) index:u32,@builtin(instance_index) instance:u32)->PrimitiveVarying {
  return primitiveRectangleVertex(index,instance,primitiveQueries[instance].lightRectangle);
}
struct PrimitiveHit { world:vec3f, unit:vec3f, depth:f32, valid:bool };
fn primitiveIntersection(uv:vec2f,inverseVP:mat4x4f,vp:mat4x4f,query:PrimitiveQuery)->PrimitiveHit {
  var hit:PrimitiveHit; hit.valid=false;
  let nearH=inverseVP*vec4f(uv,0.0,1.0);
  let farH=inverseVP*vec4f(uv,1.0,1.0);
  let origin=nearH.xyz/nearH.w;
  let direction=normalize(farH.xyz/farH.w-origin);
  let o=(query.worldToUnit*vec4f(origin,1.0)).xyz;
  let d=(query.worldToUnit*vec4f(direction,0.0)).xyz;
  let a=dot(d,d); let b=dot(o,d); let c=dot(o,o)-1.0;
  let closest=o-d*(b/a);
  let discriminant=a*(1.0-dot(closest,closest));
  if(a<=0.0 || discriminant<0.0) { return hit; }
  let root=sqrt(discriminant);
  let q=-b-select(root,-root,b<0.0);
  var first=-b/a; var second=first;
  if(q!=0.0) { first=q/a; second=c/q; }
  let near=min(first,second); let far=max(first,second);
  let distance=select(far,near,near>=0.0);
  if(distance<0.0) { return hit; }
  hit.world=origin+direction*distance; hit.unit=o+d*distance;
  let clip=vp*vec4f(hit.world,1.0); hit.depth=clip.z/clip.w;
  hit.valid=clip.w>0.0 && hit.depth>=0.0 && hit.depth<=1.0;
  return hit;
}
struct PrimitiveOutput { @location(0) color:vec4f, @location(1) motion:vec4f, @builtin(frag_depth) depth:f32 };
fn primitiveShade(v:PrimitiveVarying,procedural:bool)->PrimitiveOutput {
  let query=primitiveQueries[v.queryIndex];
  let hit=primitiveIntersection(v.uv,query.inverseVP,g.vp,query);
  if(!hit.valid) { discard; }
  var surface:Varying;
  surface.clip=v.clip; surface.world=hit.world;
  surface.normal=normalize((transpose(query.worldToUnit)*vec4f(hit.unit,0.0)).xyz);
  surface.color=vec4f(1.0,1.0,1.0,0.0); surface.bindColor=vec4f(0.28,0.33,0.42,0);
  surface.identity=v.identity; surface.local=(query.unitToLocal*vec4f(hit.unit,1.0)).xyz;
  surface.localNormal=vec4f(normalize(hit.unit),0);surface.thinUV=vec4f(0.0);
  surface.backRadiance=automaticDiffuse(obj.radianceProbes.zw,-surface.normal);
  if(automaticRadianceReady()) {
    surface.indirectProof=obj.radianceProbes;
    surface.history.z=automaticEnclosed(obj.radianceProbes.xy);surface.history.w=automaticEnclosed(obj.radianceProbes.zw);
    if(g.params.z<0.5||g.params.z>4.5){surface.bindColor=automaticDiffuse(obj.radianceProbes.xy,surface.normal);}
  }
  let front=dot(surface.normal,g.camera.xyz-hit.world)>0.0;
  let priorWorld=instances[v.queryIndex].previousModel*vec4f(surface.local,1.0);
  var out:PrimitiveOutput; out.color=shade(surface,front,procedural); out.depth=hit.depth;
  out.motion=temporalMotion(g.previousVP*priorWorld,v.identity,instances[v.queryIndex].identity.w,false);return out;
}
@fragment fn primitiveFragment(v:PrimitiveVarying)->PrimitiveOutput { return primitiveShade(v,true); }
@fragment fn primitiveSolid(v:PrimitiveVarying)->PrimitiveOutput { return primitiveShade(v,false); }
@fragment fn primitiveShadow(v:PrimitiveVarying)->@builtin(frag_depth) f32 {
  let query=primitiveQueries[v.queryIndex];
  let hit=primitiveIntersection(v.uv,query.inverseLightVP,g.lightVP,query);
  // Differentiate the implicit tangent plane with respect to NDC, rather than
  // differencing hit depths: miss lanes have no surface depth at silhouettes.
  // For H=inverseVP*[uv,z,1], d(world)/d(uv or z)=(dH.xyz-world*dH.w)/H.w;
  // n.d(world)=0 cancels the common H.w and gives the exact local dz/du,dz/dv.
  let normal=(transpose(query.worldToUnit)*vec4f(hit.unit,0.0)).xyz;
  let dx=query.inverseLightVP*vec4f(dpdx(v.uv),0.0,0.0);
  let dy=query.inverseLightVP*vec4f(dpdy(v.uv),0.0,0.0);
  let depthDirection=query.inverseLightVP[2];
  let denominator=dot(normal,depthDirection.xyz-hit.world*depthDirection.w);
  let safeDenominator=select(-1.0,1.0,denominator>=0.0)*max(abs(denominator),0.00000001);
  let depthDx=-dot(normal,dx.xyz-hit.world*dx.w)/safeDenominator;
  let depthDy=-dot(normal,dy.xyz-hit.world*dy.w)/safeDenominator;
  // True tangent slopes diverge. Limit this shading bias to eight shadow-texel
  // lengths along light depth, so a silhouette singularity cannot erase a caster.
  let slopeLimit=max(4.0*g.ground.w*g.sunlight.w,0.0000001);
  let slope=min(max(abs(depthDx),abs(depthDy)),slopeLimit);
  let biased=min(1.0,hit.depth+2.0*slope+0.00000024);
  if(!hit.valid) { discard; }
  return biased;
}
