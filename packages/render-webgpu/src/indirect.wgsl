override LOCAL_INDIRECT_LIGHTING:bool=true;
// Compiled static diffuse transport and local reflection context. Conservative
// receiver regions avoid queries; unresolved visibility uses bounded exact tests.
override SURFACE_LIGHTING_CACHE:bool=false;
override SURFACE_TRIANGLE_CACHE:bool=false;
const INDIRECT_VISIBILITY_STEPS:u32=128u;
@group(0) @binding(15) var<storage,read> indirectField:array<vec4f>;
fn compiledIndirectTriangleHit(origin:vec3f,direction:vec3f,distance:f32,triangle:u32)->bool {
  let t=u32(indirectField[3].y)+triangle*3u;let a=indirectField[t].xyz;let ab=indirectField[t+1u].xyz;let ac=indirectField[t+2u].xyz;
  let p=cross(direction,ac);let determinant=dot(ab,p);if(abs(determinant)<0.000000000001){return false;}
  let offset=origin-a;let u=dot(offset,p)/determinant;if(u< -0.000001||u>1.000001){return false;}
  let q=cross(offset,ab);let v=dot(direction,q)/determinant;if(v< -0.000001||u+v>1.000001){return false;}
  let hit=dot(ac,q)/determinant;return hit>0.00001&&hit<distance-0.0001;
}
var<private> receiverTriangleProof:vec4f;
fn compiledTriangleProof(world:vec3f,n:vec3f)->vec4f {
  if(!SURFACE_TRIANGLE_CACHE||obj.localLighting.w<0.5||receiverTriangleProof.x<0.5||indirectField[0].w<0.5){return vec4f(0,0,0,-1);}
  let oct=receiverTriangleProof.zw;var normal=vec3f(oct,1.0-abs(oct.x)-abs(oct.y));
  if(normal.z<0.0){normal=vec3f((vec2f(1)-abs(oct.yx))*select(vec2f(-1),vec2f(1),oct>=vec2f(0)),normal.z);}
  if(dot(n,normalize(normal))<0.5001){return vec4f(0,0,0,-1);}
  let spacing=indirectField[1].xyz;let dims=vec3u(indirectField[2].xyz);
  let worldQ=(world-indirectField[0].xyz)/spacing;
  if(any(worldQ<vec3f(0))||any(worldQ>vec3f(dims-vec3u(1)))){return vec4f(0,0,0,-1);}
  let q=worldQ+n*min(spacing.x,min(spacing.y,spacing.z))*0.02/spacing;
  let c=min(vec3u(floor(clamp(q,vec3f(0),vec3f(dims-vec3u(1))))),dims-vec3u(2));
  if(c.x+dims.x*(c.y+dims.y*c.z)+1u!=u32(receiverTriangleProof.x)){return vec4f(0,0,0,-1);}
  let base=u32(indirectField[3].w);if(base==0u||receiverTriangleProof.y<0.5){return vec4f(0,0,0,-1);}
  let entry=indirectField[base+u32(receiverTriangleProof.y)-1u];return vec4f(entry.xy,f32(base)+entry.z,entry.w);
}
fn compiledIndirectRegion(cell:vec3u,fraction:vec3f)->vec4f {
  let base=u32(indirectField[3].w);
  if(base==0u){return vec4f(0,0,0,-1);}
  let dims=vec3u(indirectField[2].xyz)-vec3u(1);
  let entry=indirectField[base+cell.x+dims.x*(cell.y+dims.y*cell.z)];
  var address=u32(select(entry.z,entry.w,obj.localLighting.y>0.5&&entry.w>0.0));
  if(address==0u){return vec4f(0,0,0,-1);}
  var f=fraction;
  for(var level=0u;level<3u;level++){
    let node=indirectField[base+address];
    if(node.z<=0.0){return vec4f(node.xy,f32(base)+(-node.z-1.0),node.w);}
    let bits=vec3u(f>=vec3f(0.5));
    address=u32(node.z)+bits.x+bits.y*2u+bits.z*4u;f=f*2.0-vec3f(bits);
  }
  return vec4f(0,0,0,-1);
}
fn compiledSegmentVisible(world:vec3f,n:vec3f,probe:vec3f,region:vec4f,corner:u32,cells:bool)->bool {
  let header=indirectField[3];if(header.z<0.5){return true;}
  let bit=1u<<corner;
  if((u32(region.x)&bit)!=0u){return true;}
  if((u32(region.y)&bit)!=0u){return false;}
  let origin=world-indirectField[0].xyz+n*0.0002;
  let delta=probe-indirectField[0].xyz-origin;let distance=length(delta);
  if(distance<0.0004){return true;}
  let direction=delta/distance;
  if(region.w>=0.0){
    for(var item=0u;item<u32(region.w);item++){
      let pair=indirectField[u32(region.z)+item/2u];let component=(item%2u)*2u;
      if((u32(pair[component+1u])&bit)!=0u&&compiledIndirectTriangleHit(origin,direction,distance,u32(pair[component]))){return false;}
    }
    return true;
  }
  if(cells&&header.w>0.0){
    let spacing=indirectField[1].xyz;let dims=vec3u(indirectField[2].xyz)-vec3u(1);
    let q=(world-indirectField[0].xyz)/spacing;
    let biased=clamp(q+n*(min(spacing.x,min(spacing.y,spacing.z))*0.02)/spacing,vec3f(0),vec3f(dims));
    let c=min(vec3u(floor(biased)),dims-vec3u(1));
    let cell=indirectField[u32(header.w)+c.x+dims.x*(c.y+dims.y*c.z)];
    if(cell.y>=0.0){
      for(var item=0u;item<u32(cell.y);item++){
        let triangle=u32(indirectField[u32(header.w+cell.x)+item/4u][item%4u]);
        if(compiledIndirectTriangleHit(origin,direction,distance,triangle)){return false;}
      }
      return true;
    }
  }
  var node=0u;let nodes=u32(header.x);let count=u32(header.z);
  for(var step=0u;step<INDIRECT_VISIBILITY_STEPS&&node<count;step++){
    let index=nodes+node*2u;let lower=indirectField[index];let upper=indirectField[index+1u];
    var lo=0.00001;var hi=distance-0.0001;
    for(var axis=0u;axis<3u;axis++){
      if(abs(direction[axis])<0.000000000001){if(origin[axis]<lower[axis]||origin[axis]>upper[axis]){hi=-1.0;}}
      else {let a=(lower[axis]-origin[axis])/direction[axis];let b=(upper[axis]-origin[axis])/direction[axis];lo=max(lo,min(a,b));hi=min(hi,max(a,b));}
    }
    if(hi<lo){node=u32(lower.w);continue;}
    let packed=u32(upper.w);if(packed==0u){node++;continue;}
    let first=packed/8u;
    for(var item=0u;item<(packed&7u);item++){
      if(compiledIndirectTriangleHit(origin,direction,distance,first+item)){return false;}
    }
    node=u32(lower.w);
  }
  return node==count;
}
// A short segment repairs contact lost to shadow-map receiver bias.
// The map still handles distant and animated casters. Uses only admitted static
// source geometry; no interpolated probe value is treated as a shadow proof.
fn compiledContactVisibility(world:vec3f,n:vec3f,light:vec3f,skyVisibility:f32)->f32 {
  if(!LOCAL_INDIRECT_LIGHTING||indirectField[0].w<0.5||LIGHTING_ABLATION==4u){return 1.0;}
  // A dark enclosure is only a scheduling hint: trace its actual sun visibility
  // over the volume instead of turning sampled darkness into a blocking claim.
  let distance=select(clamp(g.ground.w*4.0,0.006,0.25),length(indirectField[1].xyz*(indirectField[2].xyz-vec3f(1)))*3.0,skyVisibility<0.01);
  return select(0.0,1.0,compiledSegmentVisible(world,n,world+light*distance,vec4f(0,0,0,-1),0u,false));
}
struct CompiledIndirectLight { diffuse:vec4f, reflection:vec4f, coat:vec4f, skyVisibility:f32 };
struct SurfaceProbeWeights { low:vec4f, high:vec4f, valid:bool, radiance:u32, row:u32, f:vec2f };
fn compiledSurfaceWeights(world:vec3f,n:vec3f)->SurfaceProbeWeights {
  let fallback=SurfaceProbeWeights(vec4f(0),vec4f(0),false,0u,0u,vec2f(0));
  if(!SURFACE_LIGHTING_CACHE||(u32(indirectField[0].w)&8u)==0u||obj.localLighting.z<0.5){return fallback;}
  let dims=vec3u(indirectField[2].xyz);let count=dims.x*dims.y*dims.z;
  let base=4u+count*15u+select(0u,count*9u,(u32(indirectField[0].w)&4u)!=0u);
  let header=indirectField[base];let chart=u32(obj.localLighting.z)-1u;
  if(chart>=u32(header.x)){return fallback;}
  let start=base+2u+chart*4u;
  let origin=indirectField[start];let u=indirectField[start+1u];let v=indirectField[start+2u];let normal=indirectField[start+3u];
  // Back faces, changed shading normals and out-of-chart positions use queries.
  if(dot(n,normal.xyz)<0.999999){return fallback;}
  let delta=world-indirectField[0].xyz-origin.xyz;
  let uv=vec2f(dot(delta,u.xyz),dot(delta,v.xyz));let size=vec2u(u32(origin.w),u32(u.w));
  if(abs(dot(delta,normal.xyz))>0.000001||any(uv<vec2f(0))||any(uv>vec2f(size-vec2u(1)))){return fallback;}
  let cell=min(vec2u(floor(uv)),size-vec2u(2));let f=uv-vec2f(cell);
  let tile=u32(normal.w)+cell.x+(size.x-1u)*cell.y;
  if(indirectField[base+u32(header.w)+tile/4u][tile%4u]<0.5){return fallback;}
  let sample=u32(v.w)+cell.x+size.x*cell.y;let data=base+u32(header.z)+sample*2u;
  let output=u32(indirectField[base+1u].y);
  if(output>0u){return SurfaceProbeWeights(vec4f(0),vec4f(0),true,base+output+sample*10u,size.x*10u,f);}
  let row=size.x*2u;
  let low=mix(mix(indirectField[data],indirectField[data+2u],f.x),mix(indirectField[data+row],indirectField[data+row+2u],f.x),f.y);
  let high=mix(mix(indirectField[data+1u],indirectField[data+3u],f.x),mix(indirectField[data+row+1u],indirectField[data+row+3u],f.x),f.y);
  return SurfaceProbeWeights(low,high,true,0u,0u,f);
}
fn compiledSurfaceRadiance(tile:SurfaceProbeWeights,band:u32)->vec4f {
  let i=tile.radiance+band;let row=tile.row;let f=tile.f;
  return mix(mix(indirectField[i],indirectField[i+10u],f.x),mix(indirectField[i+row],indirectField[i+row+10u],f.x),f.y);
}
fn compiledIndirectSample(world:vec3f,n:vec3f,ray:vec3f,rough:vec2f,reflections:bool)->CompiledIndirectLight {
  let fallback=CompiledIndirectLight(vec4f(0),vec4f(0,0,0,1),vec4f(0,0,0,1),1.0);
  if(!LOCAL_INDIRECT_LIGHTING||LIGHTING_ABLATION==1u||indirectField[0].w<0.5){return fallback;}
  let origin=indirectField[0].xyz;let spacing=indirectField[1].xyz;let dims=vec3u(indirectField[2].xyz);
  let q=(world-origin)/spacing;
  if(any(q<vec3f(0))||any(q>vec3f(dims-vec3u(1)))){return fallback;}
  let biased=clamp(q+n*(min(spacing.x,min(spacing.y,spacing.z))*0.02)/spacing,vec3f(0),vec3f(dims-vec3u(1)));
  let cell=min(vec3u(floor(biased)),dims-vec3u(2));let f=biased-vec3f(cell);
  let cached=compiledSurfaceWeights(world,n);
  if(cached.valid&&cached.radiance>0u){
    let diffuse=compiledSurfaceRadiance(cached,0u);
    var reflected=vec4f(0,0,0,1);var coated=reflected;
    if(reflections&&(u32(indirectField[0].w)&4u)!=0u){
      let bands=environmentReflectionBands(rough.x);var coatBands=vec3f(0);
      if(rough.y>=0.0){coatBands=environmentReflectionBands(rough.y);}
      reflected=vec4f(0);coated=vec4f(0);
      for(var band=0u;band<9u;band++){
        let order=select(select(0u,1u,band>0u),2u,band>3u);
        let value=compiledSurfaceRadiance(cached,band+1u)*irradianceBasis(ray,band);
        reflected+=value*bands[order];coated+=value*coatBands[order];
      }
      reflected=vec4f(max(reflected.xyz,vec3f(0)),clamp(reflected.w,0.0,1.0));
      coated=vec4f(max(coated.xyz,vec3f(0)),clamp(coated.w,0.0,1.0));
    }
    return CompiledIndirectLight(vec4f(diffuse.xyz,1),reflected,coated,diffuse.w);
  }
  var region=vec4f(0,0,0,-1);
  if(!cached.valid){
    region=compiledTriangleProof(world,n);
    if(region.w<0.0){region=compiledIndirectRegion(cell,f);}
  }
  var result=vec3f(0);var total=0.0;
  var reflected=vec4f(0);var coated=vec4f(0);var skyVisibility=0.0;
  let localReflections=reflections&&(u32(indirectField[0].w)&4u)!=0u;
  let hasCoat=rough.y>=0.0;
  var reflectionBands=vec3f(0);var coatBands=vec3f(0);
  if(localReflections){reflectionBands=environmentReflectionBands(rough.x);if(hasCoat){coatBands=environmentReflectionBands(rough.y);}}
  for(var corner=0u;corner<8u;corner++){
    if((u32(region.y)&(1u<<corner))!=0u){continue;}
    let bit=vec3u(corner&1u,(corner>>1u)&1u,(corner>>2u)&1u);let c=cell+bit;
    let index=c.x+dims.x*(c.y+dims.y*c.z);let offset=4u+index*15u;
    if(indirectField[offset].w<0.5){continue;}
    var weight=0.0;
    if(cached.valid){weight=select(cached.low[corner%4u],cached.high[corner%4u],corner>=4u);}
    else {
    let placement=vec3f(indirectField[offset+9u].zw,indirectField[offset+10u].z);
    let probe=origin+vec3f(c)*spacing+placement;let delta=world-probe;let distance=length(delta);
    let direction=delta/max(distance,0.000001);var mean=0.0;var second=0.0;var dw=0.0;
    for(var axis=0u;axis<3u;axis++){
      let squared=direction[axis]*direction[axis];let fourth=squared*squared;let weight=fourth*fourth;let lobe=axis*2u+select(0u,1u,direction[axis]<0.0);
      let moments=indirectField[offset+9u+lobe];mean+=moments.x*weight;second+=moments.y*weight;dw+=weight;
    }
    mean/=max(dw,0.000000000001);second/=max(dw,0.000000000001);
    let variance=max(0.000001,second-mean*mean);let excess=max(0.0,distance-mean-min(spacing.x,min(spacing.y,spacing.z))*0.04);
    let chebyshev=variance/(variance+excess*excess);let visibility=select(chebyshev*chebyshev*chebyshev,1.0,excess==0.0);
    let facingCosine=max(0.0,-dot(direction,n));let facing=select(facingCosine*facingCosine,1.0,distance<0.00001);
    let trilinear=select(vec3f(1)-f,f,bit>vec3u(0));weight=visibility*facing*trilinear.x*trilinear.y*trilinear.z;
    if(weight<=0.0000000001||!compiledSegmentVisible(world,n,probe,region,corner,true)){continue;}
    }
    if(weight<=0.0000000001){continue;}
    var irradiance=vec3f(0);
    for(var band=0u;band<9u;band++){irradiance+=indirectField[offset+band].xyz*irradianceBasis(n,band);}
    if(localReflections){
      let start=4u+dims.x*dims.y*dims.z*15u+index*9u;
      skyVisibility+=clamp(indirectField[start].w*0.2820947918,0.0,1.0)*weight;
      var local=vec4f(0);var coat=vec4f(0);
      for(var band=0u;band<9u;band++){
        let order=select(select(0u,1u,band>0u),2u,band>3u);
        let radiance=indirectField[start+band]*irradianceBasis(ray,band);
        local+=radiance*reflectionBands[order];if(hasCoat){coat+=radiance*coatBands[order];}
      }
      reflected+=vec4f(max(local.xyz,vec3f(0)),clamp(local.w,0.0,1.0))*weight;
      coated+=vec4f(max(coat.xyz,vec3f(0)),clamp(coat.w,0.0,1.0))*weight;
    }
    result+=max(irradiance,vec3f(0))*weight;total+=weight;
  }
  if(total<0.00000001){
    let ready=indirectField[2].w;
    return CompiledIndirectLight(vec4f(0,0,0,ready),vec4f(0,0,0,select(1.0,1.0-ready,localReflections)),vec4f(0,0,0,select(1.0,1.0-ready,localReflections)),select(1.0,1.0-ready,localReflections));
  }
  return CompiledIndirectLight(vec4f(result/total,1.0),select(fallback.reflection,reflected/total,localReflections),select(fallback.coat,coated/total,localReflections),select(1.0,skyVisibility/total,localReflections));
}
fn compiledIndirectDiffuse(world:vec3f,n:vec3f)->vec4f {
  return compiledIndirectSample(world,n,n,vec2f(1),false).diffuse;
}
fn compiledReflectionRadiance(context:vec4f,ray:vec3f,rough:f32)->vec3f {
  if(LIGHTING_ABLATION==3u){return vec3f(0);}
  return context.xyz+physicalSkyRoughReflection(ray,rough)*context.w;
}
